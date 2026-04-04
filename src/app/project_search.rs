use serde::Serialize;
use serde_json::Value;
use std::collections::BTreeMap;
use std::fs;
use std::io::{BufRead, BufReader};
use std::path::PathBuf;
use std::process::{Command, Stdio};

#[derive(Debug, Clone, Serialize)]
pub(crate) struct ProjectSearchRange {
    pub(crate) start: usize,
    pub(crate) end: usize,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct ProjectSearchMatch {
    pub(crate) line: usize,
    pub(crate) column: usize,
    pub(crate) end_column: usize,
    pub(crate) text: String,
    pub(crate) ranges: Vec<ProjectSearchRange>,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct ProjectSearchFile {
    pub(crate) path: String,
    pub(crate) match_count: usize,
    pub(crate) matches: Vec<ProjectSearchMatch>,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct ProjectSearchResponse {
    pub(crate) query: String,
    pub(crate) total_files: usize,
    pub(crate) total_matches: usize,
    pub(crate) truncated: bool,
    pub(crate) files: Vec<ProjectSearchFile>,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct ProjectSearchPreviewDiff {
    pub(crate) old_contents: String,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct ProjectSearchPreview {
    pub(crate) path: String,
    pub(crate) contents: String,
    pub(crate) line_count: usize,
    pub(crate) matches: Vec<ProjectSearchMatch>,
    pub(crate) diff: Option<ProjectSearchPreviewDiff>,
}

const MAX_MATCHES: usize = 4_000;

pub(crate) fn empty_response(query: &str) -> ProjectSearchResponse {
    ProjectSearchResponse {
        query: query.to_string(),
        total_files: 0,
        total_matches: 0,
        truncated: false,
        files: Vec::new(),
    }
}

pub(crate) fn search(worktree_path: &str, query: &str) -> Result<ProjectSearchResponse, String> {
    let trimmed = query.trim();
    if trimmed.is_empty() {
        return browse_files(worktree_path);
    }

    let mut child = Command::new("rg")
        .current_dir(worktree_path)
        .args([
            "--json",
            "--line-number",
            "--smart-case",
            "--hidden",
            "--glob",
            "!.git",
            trimmed,
            ".",
        ])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|error| format!("failed to run rg: {error}"))?;

    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| String::from("failed to capture rg stdout"))?;
    let reader = BufReader::new(stdout);

    let mut grouped = BTreeMap::<String, Vec<ProjectSearchMatch>>::new();
    let mut total_matches = 0usize;
    let mut truncated = false;

    for line_result in reader.lines() {
        let line = line_result.map_err(|error| format!("failed to read rg output: {error}"))?;
        let Some((path, entry)) = parse_rg_json_line(&line) else {
            continue;
        };

        grouped.entry(path).or_default().push(entry);
        total_matches += 1;
        if total_matches >= MAX_MATCHES {
            truncated = true;
            let _ = child.kill();
            break;
        }
    }

    let output = child
        .wait_with_output()
        .map_err(|error| format!("failed to wait for rg: {error}"))?;

    if !output.status.success() && output.status.code() != Some(1) {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        let message = if stderr.is_empty() {
            format!("rg failed with {}", output.status)
        } else {
            stderr
        };
        return Err(message);
    }

    let files = grouped
        .into_iter()
        .map(|(path, matches)| ProjectSearchFile {
            match_count: matches.len(),
            path,
            matches,
        })
        .collect::<Vec<_>>();

    Ok(ProjectSearchResponse {
        query: trimmed.to_string(),
        total_files: files.len(),
        total_matches,
        truncated,
        files,
    })
}

fn browse_files(worktree_path: &str) -> Result<ProjectSearchResponse, String> {
    let output = Command::new("rg")
        .current_dir(worktree_path)
        .args(["--files", "--hidden", "--glob", "!.git"])
        .output()
        .map_err(|error| format!("failed to list files: {error}"))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        let message = if stderr.is_empty() {
            format!("file listing failed with {}", output.status)
        } else {
            stderr
        };
        return Err(message);
    }

    let files = String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(|path| ProjectSearchFile {
            path: path.to_string(),
            match_count: 0,
            matches: Vec::new(),
        })
        .collect::<Vec<_>>();

    Ok(ProjectSearchResponse {
        query: String::new(),
        total_files: files.len(),
        total_matches: 0,
        truncated: false,
        files,
    })
}

pub(crate) fn load_preview(
    worktree_path: &str,
    relative_path: &str,
    matches: Vec<ProjectSearchMatch>,
) -> Result<ProjectSearchPreview, String> {
    let target = PathBuf::from(worktree_path).join(relative_path);
    let bytes = fs::read(&target)
        .map_err(|error| format!("failed to read {}: {error}", target.display()))?;
    let contents = String::from_utf8_lossy(&bytes).into_owned();
    let line_count = contents.lines().count().max(1);
    let diff = load_changed_file_diff(worktree_path, relative_path, &contents);

    Ok(ProjectSearchPreview {
        path: relative_path.to_string(),
        contents,
        line_count,
        matches,
        diff,
    })
}

fn load_changed_file_diff(
    worktree_path: &str,
    relative_path: &str,
    current_contents: &str,
) -> Option<ProjectSearchPreviewDiff> {
    let output = Command::new("git")
        .current_dir(worktree_path)
        .args(["status", "--porcelain=v1", "--", relative_path])
        .output()
        .ok()?;

    if !output.status.success() {
        return None;
    }

    let status = String::from_utf8_lossy(&output.stdout);
    let status_line = status.lines().find(|line| !line.trim().is_empty())?;
    if status_line.starts_with("??") {
        return None;
    }

    let object_spec = format!("HEAD:{relative_path}");
    let old_output = Command::new("git")
        .current_dir(worktree_path)
        .args(["show", &object_spec])
        .output()
        .ok()?;

    if !old_output.status.success() {
        return None;
    }

    let old_contents = String::from_utf8_lossy(&old_output.stdout).into_owned();
    if old_contents == current_contents {
        return None;
    }

    Some(ProjectSearchPreviewDiff { old_contents })
}

fn parse_rg_json_line(line: &str) -> Option<(String, ProjectSearchMatch)> {
    let value = serde_json::from_str::<Value>(line).ok()?;
    if value.get("type")?.as_str()? != "match" {
        return None;
    }

    let data = value.get("data")?;
    let path = data
        .get("path")?
        .get("text")?
        .as_str()
        .map(str::to_string)?;
    let line_number = data.get("line_number")?.as_u64()? as usize;
    let text = data
        .get("lines")?
        .get("text")?
        .as_str()
        .unwrap_or_default()
        .trim_end_matches('\n')
        .trim_end_matches('\r')
        .to_string();

    let ranges = data
        .get("submatches")?
        .as_array()?
        .iter()
        .filter_map(|entry| {
            let start = entry.get("start")?.as_u64()? as usize;
            let end = entry.get("end")?.as_u64()? as usize;
            Some(ProjectSearchRange { start, end })
        })
        .collect::<Vec<_>>();

    let (column, end_column) = ranges
        .first()
        .map(|range| (range.start + 1, range.end + 1))
        .unwrap_or((1, 1));

    Some((
        path,
        ProjectSearchMatch {
            line: line_number,
            column,
            end_column,
            text,
            ranges,
        },
    ))
}
