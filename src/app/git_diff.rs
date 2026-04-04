use std::path::Path;
use std::process::Command;

#[derive(Debug, Clone)]
pub(crate) struct DiffSnapshot {
    pub(crate) worktree_path: String,
    pub(crate) sections: Vec<DiffSection>,
}

#[derive(Debug, Clone)]
pub(crate) struct DiffSection {
    pub(crate) label: &'static str,
    pub(crate) empty_label: &'static str,
    pub(crate) files: Vec<DiffFile>,
}

#[derive(Debug, Clone)]
pub(crate) struct DiffFile {
    pub(crate) path: String,
    pub(crate) added: usize,
    pub(crate) removed: usize,
    pub(crate) hunks: Vec<DiffHunk>,
}

#[derive(Debug, Clone)]
pub(crate) struct DiffHunk {
    pub(crate) header: String,
    pub(crate) lines: Vec<DiffLine>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum DiffLineKind {
    Context,
    Added,
    Removed,
    Note,
}

#[derive(Debug, Clone)]
pub(crate) struct DiffLine {
    pub(crate) kind: DiffLineKind,
    pub(crate) old_line: Option<usize>,
    pub(crate) new_line: Option<usize>,
    pub(crate) text: String,
}

pub(crate) fn load_snapshot(worktree_path: &str) -> Result<DiffSnapshot, String> {
    let unstaged = load_section(
        worktree_path,
        "Unstaged",
        "No unstaged changes",
        &["diff", "--no-ext-diff"],
    )?;
    let staged = load_section(
        worktree_path,
        "Staged",
        "No staged changes",
        &["diff", "--no-ext-diff", "--staged"],
    )?;

    Ok(DiffSnapshot {
        worktree_path: worktree_path.to_string(),
        sections: vec![unstaged, staged],
    })
}

pub(crate) fn render_loading_html(worktree_path: &str) -> String {
    let title = repo_title(worktree_path);
    render_document(
        &title,
        &format!(
            "<div class=\"hero\"><div class=\"hero-label\">Diff</div><h1>{}</h1><p>Loading working tree changes…</p></div>",
            escape_html(&title)
        ),
    )
}

pub(crate) fn render_error_html(worktree_path: &str, error: &str) -> String {
    let title = repo_title(worktree_path);
    render_document(
        &title,
        &format!(
            "<div class=\"hero\"><div class=\"hero-label\">Diff</div><h1>{}</h1><p class=\"error\">{}</p></div>",
            escape_html(&title),
            escape_html(error)
        ),
    )
}

pub(crate) fn render_snapshot_html(snapshot: &DiffSnapshot) -> String {
    let title = repo_title(&snapshot.worktree_path);
    let sections = snapshot
        .sections
        .iter()
        .map(render_section)
        .collect::<Vec<_>>()
        .join("");
    let body = format!(
        "<div class=\"hero\"><div class=\"hero-label\">Diff</div><h1>{}</h1><p>{}</p></div>{}",
        escape_html(&title),
        escape_html(&snapshot.worktree_path),
        sections
    );
    render_document(&title, &body)
}

fn load_section(
    worktree_path: &str,
    label: &'static str,
    empty_label: &'static str,
    args: &[&str],
) -> Result<DiffSection, String> {
    let output = Command::new("git")
        .arg("-C")
        .arg(worktree_path)
        .args(args)
        .output()
        .map_err(|error| format!("failed to run git {}: {error}", args.join(" ")))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        let message = if stderr.is_empty() {
            format!("git {} failed with {}", args.join(" "), output.status)
        } else {
            stderr
        };
        return Err(message);
    }

    let patch = String::from_utf8(output.stdout)
        .map_err(|error| format!("git {} produced invalid UTF-8: {error}", args.join(" ")))?;

    Ok(DiffSection {
        label,
        empty_label,
        files: parse_patch(&patch),
    })
}

fn parse_patch(patch: &str) -> Vec<DiffFile> {
    let mut files = Vec::new();
    let mut current_file: Option<DiffFile> = None;
    let mut current_hunk: Option<usize> = None;
    let mut old_line = 0usize;
    let mut new_line = 0usize;

    for raw_line in patch.lines() {
        if let Some(path) = raw_line
            .strip_prefix("diff --git ")
            .and_then(parse_diff_git_path)
        {
            if let Some(file) = current_file.take() {
                files.push(file);
            }

            current_file = Some(DiffFile {
                path,
                added: 0,
                removed: 0,
                hunks: Vec::new(),
            });
            current_hunk = None;
            continue;
        }

        let Some(file) = current_file.as_mut() else {
            continue;
        };

        if let Some(path) = raw_line.strip_prefix("+++ ") {
            if let Some(path) = normalize_patch_path(path) {
                file.path = path;
            }
            continue;
        }

        if let Some((next_old_line, next_new_line)) = parse_hunk_header(raw_line) {
            old_line = next_old_line;
            new_line = next_new_line;
            file.hunks.push(DiffHunk {
                header: raw_line.to_string(),
                lines: Vec::new(),
            });
            current_hunk = Some(file.hunks.len() - 1);
            continue;
        }

        let Some(hunk_idx) = current_hunk else {
            continue;
        };
        let hunk = &mut file.hunks[hunk_idx];

        if raw_line.starts_with('+') && !raw_line.starts_with("+++") {
            file.added += 1;
            hunk.lines.push(DiffLine {
                kind: DiffLineKind::Added,
                old_line: None,
                new_line: Some(new_line),
                text: raw_line[1..].to_string(),
            });
            new_line += 1;
        } else if raw_line.starts_with('-') && !raw_line.starts_with("---") {
            file.removed += 1;
            hunk.lines.push(DiffLine {
                kind: DiffLineKind::Removed,
                old_line: Some(old_line),
                new_line: None,
                text: raw_line[1..].to_string(),
            });
            old_line += 1;
        } else if let Some(context) = raw_line.strip_prefix(' ') {
            hunk.lines.push(DiffLine {
                kind: DiffLineKind::Context,
                old_line: Some(old_line),
                new_line: Some(new_line),
                text: context.to_string(),
            });
            old_line += 1;
            new_line += 1;
        } else if let Some(note) = raw_line.strip_prefix('\\') {
            hunk.lines.push(DiffLine {
                kind: DiffLineKind::Note,
                old_line: None,
                new_line: None,
                text: note.trim().to_string(),
            });
        }
    }

    if let Some(file) = current_file.take() {
        files.push(file);
    }

    files
}

fn parse_diff_git_path(line: &str) -> Option<String> {
    let mut parts = line.split_whitespace();
    let _a_path = parts.next()?;
    let b_path = parts.next()?;
    normalize_patch_path(b_path)
}

fn normalize_patch_path(raw: &str) -> Option<String> {
    let trimmed = raw.trim();
    if trimmed == "/dev/null" {
        None
    } else if let Some(path) = trimmed.strip_prefix("a/") {
        Some(path.to_string())
    } else if let Some(path) = trimmed.strip_prefix("b/") {
        Some(path.to_string())
    } else {
        Some(trimmed.to_string())
    }
}

fn parse_hunk_header(line: &str) -> Option<(usize, usize)> {
    if !line.starts_with("@@ ") {
        return None;
    }

    let mut parts = line.split_whitespace();
    let _marker = parts.next()?;
    let old_spec = parts.next()?;
    let new_spec = parts.next()?;
    Some((parse_hunk_range(old_spec)?, parse_hunk_range(new_spec)?))
}

fn parse_hunk_range(spec: &str) -> Option<usize> {
    let without_sign = spec.strip_prefix(['-', '+'])?;
    let number = without_sign.split(',').next()?;
    number.parse().ok()
}

fn render_section(section: &DiffSection) -> String {
    let count = section.files.len();
    let files = if section.files.is_empty() {
        format!(
            "<div class=\"empty-state\">{}</div>",
            escape_html(section.empty_label)
        )
    } else {
        section
            .files
            .iter()
            .map(render_file)
            .collect::<Vec<_>>()
            .join("")
    };

    format!(
        "<section class=\"diff-section\"><div class=\"section-header\"><div class=\"section-title\">{}</div><div class=\"section-count\">{}</div></div>{}</section>",
        escape_html(section.label),
        count,
        files
    )
}

fn render_file(file: &DiffFile) -> String {
    let hunks = if file.hunks.is_empty() {
        "<div class=\"file-empty\">No textual changes to display.</div>".to_string()
    } else {
        file.hunks
            .iter()
            .map(render_hunk)
            .collect::<Vec<_>>()
            .join("")
    };

    format!(
        "<details class=\"file-card\" open><summary><span class=\"file-path\">{}</span><span class=\"file-stats\"><span class=\"added\">+{}</span><span class=\"removed\">-{}</span></span></summary><div class=\"file-body\">{}</div></details>",
        escape_html(&file.path),
        file.added,
        file.removed,
        hunks
    )
}

fn render_hunk(hunk: &DiffHunk) -> String {
    let rows = render_hunk_rows(&hunk.lines);
    format!(
        "<div class=\"hunk\"><div class=\"hunk-header\">{}</div><div class=\"diff-grid\">{}</div></div>",
        escape_html(&hunk.header),
        rows
    )
}

fn render_hunk_rows(lines: &[DiffLine]) -> String {
    let mut rendered = String::new();
    let mut index = 0usize;

    while index < lines.len() {
        if lines[index].kind != DiffLineKind::Context {
            rendered.push_str(&render_row(&lines[index]));
            index += 1;
            continue;
        }

        let start = index;
        while index < lines.len() && lines[index].kind == DiffLineKind::Context {
            index += 1;
        }
        let context_run = &lines[start..index];
        const CONTEXT_LIMIT: usize = 8;
        const CONTEXT_VISIBLE: usize = 2;

        if context_run.len() > CONTEXT_LIMIT {
            for line in &context_run[..CONTEXT_VISIBLE] {
                rendered.push_str(&render_row(line));
            }
            rendered.push_str(&format!(
                "<div class=\"row row-gap\"><div class=\"line old\"></div><div class=\"line new\"></div><div class=\"code\">{} unmodified lines</div></div>",
                context_run.len() - (CONTEXT_VISIBLE * 2)
            ));
            for line in &context_run[context_run.len() - CONTEXT_VISIBLE..] {
                rendered.push_str(&render_row(line));
            }
        } else {
            for line in context_run {
                rendered.push_str(&render_row(line));
            }
        }
    }

    rendered
}

fn render_row(line: &DiffLine) -> String {
    let (row_class, sign) = match line.kind {
        DiffLineKind::Context => ("row-context", " "),
        DiffLineKind::Added => ("row-added", "+"),
        DiffLineKind::Removed => ("row-removed", "-"),
        DiffLineKind::Note => ("row-note", ""),
    };

    let old_line = line
        .old_line
        .map(|value| value.to_string())
        .unwrap_or_default();
    let new_line = line
        .new_line
        .map(|value| value.to_string())
        .unwrap_or_default();
    let text = if line.kind == DiffLineKind::Note {
        format!("\\ {}", line.text)
    } else {
        format!("{sign}{}", line.text)
    };

    format!(
        "<div class=\"row {}\"><div class=\"line old\">{}</div><div class=\"line new\">{}</div><div class=\"code\">{}</div></div>",
        row_class,
        escape_html(&old_line),
        escape_html(&new_line),
        escape_html(&text)
    )
}

fn repo_title(worktree_path: &str) -> String {
    Path::new(worktree_path)
        .file_name()
        .and_then(|value| value.to_str())
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
        .unwrap_or_else(|| String::from("Diff"))
}

fn render_document(title: &str, body: &str) -> String {
    format!(
        "<!doctype html><html><head><meta charset=\"utf-8\"><meta name=\"viewport\" content=\"width=device-width, initial-scale=1\"><title>{}</title><style>{}</style></head><body>{}</body></html>",
        escape_html(title),
        document_css(),
        body
    )
}

fn document_css() -> &'static str {
    r#"
:root {
  color-scheme: dark;
  --bg: #111213;
  --panel: #18191b;
  --panel-2: #1f2023;
  --border: #2c2d31;
  --muted: #8e949d;
  --text: #eef2f6;
  --green-bg: rgba(76, 175, 80, 0.16);
  --green-edge: rgba(71, 214, 114, 0.9);
  --red-bg: rgba(244, 67, 54, 0.16);
  --red-edge: rgba(255, 96, 96, 0.9);
  --ctx-bg: #17181a;
  --gap-bg: #34363a;
  --hunk-bg: #141518;
}
* { box-sizing: border-box; }
html, body {
  margin: 0;
  background: linear-gradient(180deg, #121315 0%, #0f1011 100%);
  color: var(--text);
  font-family: ui-monospace, "SF Mono", Menlo, Monaco, monospace;
}
body {
  min-height: 100vh;
  padding: 18px;
}
.hero {
  margin-bottom: 18px;
  padding: 10px 6px 2px;
}
.hero-label {
  color: var(--muted);
  font-size: 12px;
  text-transform: uppercase;
  letter-spacing: 0.08em;
}
.hero h1 {
  margin: 6px 0 8px;
  font-size: 26px;
  line-height: 1.15;
}
.hero p {
  margin: 0;
  color: var(--muted);
  word-break: break-all;
}
.hero .error {
  color: #ff9b9b;
}
.diff-section {
  margin-bottom: 18px;
}
.section-header {
  display: inline-flex;
  align-items: center;
  gap: 10px;
  margin-bottom: 10px;
  padding: 8px 12px;
  background: rgba(33, 34, 37, 0.96);
  border: 1px solid var(--border);
  border-radius: 999px;
}
.section-title {
  font-size: 16px;
  font-weight: 700;
}
.section-count {
  min-width: 28px;
  padding: 2px 8px;
  border-radius: 999px;
  text-align: center;
  background: rgba(255,255,255,0.08);
  color: #d9dde3;
  font-size: 13px;
}
.empty-state, .file-empty {
  padding: 18px;
  border: 1px solid var(--border);
  border-radius: 16px;
  background: rgba(24, 25, 27, 0.88);
  color: var(--muted);
}
.file-card {
  margin-bottom: 14px;
  border: 1px solid var(--border);
  border-radius: 16px;
  background: rgba(24, 25, 27, 0.96);
  overflow: hidden;
}
.file-card > summary {
  list-style: none;
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 12px;
  padding: 16px 20px;
  cursor: pointer;
  background: rgba(30, 31, 34, 0.95);
}
.file-card > summary::-webkit-details-marker {
  display: none;
}
.file-path {
  font-size: 15px;
  font-weight: 700;
  word-break: break-word;
}
.file-stats {
  display: inline-flex;
  gap: 10px;
  font-size: 14px;
  white-space: nowrap;
}
.added { color: #42d17f; }
.removed { color: #ff5f61; }
.file-body {
  padding: 0 10px 10px;
}
.hunk {
  margin-top: 10px;
  border: 1px solid rgba(55, 57, 62, 0.85);
  border-radius: 12px;
  overflow: hidden;
  background: var(--hunk-bg);
}
.hunk-header {
  padding: 8px 14px;
  background: rgba(29, 30, 34, 0.98);
  color: var(--muted);
  font-size: 12px;
}
.diff-grid {
  display: block;
}
.row {
  display: grid;
  grid-template-columns: 68px 68px minmax(0, 1fr);
  align-items: stretch;
  min-height: 24px;
}
.row .line,
.row .code {
  padding: 3px 10px;
}
.row .line {
  color: #7f8791;
  text-align: right;
  border-right: 1px solid rgba(55, 57, 62, 0.65);
  user-select: none;
}
.row .code {
  white-space: pre-wrap;
  word-break: break-word;
}
.row-context {
  background: var(--ctx-bg);
}
.row-added {
  background: linear-gradient(90deg, rgba(71, 214, 114, 0.16) 0%, var(--green-bg) 100%);
}
.row-added .line.old {
  border-left: 4px solid var(--green-edge);
}
.row-removed {
  background: linear-gradient(90deg, rgba(255, 96, 96, 0.18) 0%, var(--red-bg) 100%);
}
.row-removed .line.old {
  border-left: 4px solid var(--red-edge);
}
.row-note {
  background: rgba(34, 35, 39, 0.94);
  color: var(--muted);
  font-style: italic;
}
.row-gap {
  background: var(--gap-bg);
  color: #c0c5cc;
}
.row-gap .code {
  padding: 6px 12px;
  border-radius: 10px;
  margin: 8px;
  background: rgba(255,255,255,0.06);
}
"#
}

fn escape_html(value: &str) -> String {
    let mut escaped = String::with_capacity(value.len());
    for ch in value.chars() {
        match ch {
            '&' => escaped.push_str("&amp;"),
            '<' => escaped.push_str("&lt;"),
            '>' => escaped.push_str("&gt;"),
            '"' => escaped.push_str("&quot;"),
            '\'' => escaped.push_str("&#39;"),
            _ => escaped.push(ch),
        }
    }
    escaped
}
