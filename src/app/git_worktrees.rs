use crate::app::model::{WorktreeInfo, infer_worktree_name};
use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

pub(crate) fn scan_worktrees(git_folder: &str) -> Result<Vec<WorktreeInfo>, String> {
    let repo_root = normalize_path(&PathBuf::from(git_folder))?;
    if !repo_root.exists() {
        return Err(format!(
            "git folder does not exist: {}",
            repo_root.display()
        ));
    }

    let git_meta = repo_root.join(".git");
    if !git_meta.exists() {
        return Err(format!(
            "selected folder is not a git repository root (missing .git): {}",
            repo_root.display()
        ));
    }

    let git_dir = resolve_git_dir(&repo_root, &git_meta)?;
    let common_git_dir = resolve_common_git_dir(&git_dir)?;

    let mut seen_paths = HashSet::<String>::new();
    let mut worktrees = Vec::<WorktreeInfo>::new();

    push_worktree(&mut worktrees, &mut seen_paths, repo_root.clone(), "main");
    if let Some(main_root) = infer_main_worktree_root(&common_git_dir) {
        push_worktree(&mut worktrees, &mut seen_paths, main_root, "main");
    }

    let worktrees_dir = common_git_dir.join("worktrees");
    if worktrees_dir.exists() {
        let mut linked = Vec::<WorktreeInfo>::new();
        for entry in fs::read_dir(&worktrees_dir)
            .map_err(|e| format!("failed to read {}: {e}", worktrees_dir.display()))?
        {
            let entry = entry.map_err(|e| format!("failed to read worktree entry: {e}"))?;
            let admin_dir = entry.path();
            if !admin_dir.is_dir() {
                continue;
            }

            let gitdir_ref_file = admin_dir.join("gitdir");
            if !gitdir_ref_file.exists() {
                continue;
            }

            let gitdir_ref = fs::read_to_string(&gitdir_ref_file).map_err(|e| {
                format!(
                    "failed to read worktree pointer {}: {e}",
                    gitdir_ref_file.display()
                )
            })?;

            let gitdir_path = resolve_relative_path(&admin_dir, gitdir_ref.trim());
            let worktree_root = gitdir_path.parent().ok_or_else(|| {
                format!(
                    "invalid worktree gitdir path without parent: {}",
                    gitdir_path.display()
                )
            })?;

            let normalized_root = normalize_existing_or_absolute(worktree_root)?;
            let normalized_path = normalized_root.to_string_lossy().to_string();
            if !seen_paths.insert(normalized_path.clone()) {
                continue;
            }

            linked.push(WorktreeInfo {
                id: normalized_path.clone(),
                name: infer_worktree_name(&normalized_root, &entry.file_name().to_string_lossy()),
                path: normalized_path,
                missing: !normalized_root.exists(),
            });
        }

        linked.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
        worktrees.extend(linked);
    }

    Ok(worktrees)
}

pub(crate) fn add_worktree(
    git_folder: &str,
    worktree_path: &str,
    branch_name: &str,
) -> Result<(), String> {
    let destination = worktree_path.trim();
    let branch = branch_name.trim();
    if destination.is_empty() {
        return Err(String::from("worktree path cannot be empty"));
    }
    if branch.is_empty() {
        return Err(String::from("branch name cannot be empty"));
    }

    let primary = Command::new("git")
        .arg("-C")
        .arg(git_folder)
        .arg("worktree")
        .arg("add")
        .arg(destination)
        .arg(branch)
        .output()
        .map_err(|error| format!("failed to run git worktree add: {error}"))?;

    if primary.status.success() {
        return Ok(());
    }

    let fallback = Command::new("git")
        .arg("-C")
        .arg(git_folder)
        .arg("worktree")
        .arg("add")
        .arg("-b")
        .arg(branch)
        .arg(destination)
        .output()
        .map_err(|error| format!("failed to run git worktree add -b: {error}"))?;

    if fallback.status.success() {
        return Ok(());
    }

    Err(format!(
        "git worktree add failed: {}",
        stderr_or_status(&fallback)
    ))
}

pub(crate) fn remove_worktree(git_folder: &str, worktree_path: &str) -> Result<(), String> {
    let target = worktree_path.trim();
    if target.is_empty() {
        return Err(String::from("worktree path cannot be empty"));
    }

    let primary = Command::new("git")
        .arg("-C")
        .arg(git_folder)
        .arg("worktree")
        .arg("remove")
        .arg(target)
        .output()
        .map_err(|error| format!("failed to run git worktree remove: {error}"))?;

    if primary.status.success() {
        return Ok(());
    }

    let force = Command::new("git")
        .arg("-C")
        .arg(git_folder)
        .arg("worktree")
        .arg("remove")
        .arg("--force")
        .arg(target)
        .output()
        .map_err(|error| format!("failed to run git worktree remove --force: {error}"))?;

    if force.status.success() {
        return Ok(());
    }

    Err(format!(
        "git worktree remove failed: {}",
        stderr_or_status(&force)
    ))
}

fn push_worktree(
    worktrees: &mut Vec<WorktreeInfo>,
    seen_paths: &mut HashSet<String>,
    root_path: PathBuf,
    fallback_name: &str,
) {
    let normalized_path = root_path.to_string_lossy().to_string();
    if !seen_paths.insert(normalized_path.clone()) {
        return;
    }

    worktrees.push(WorktreeInfo {
        id: normalized_path.clone(),
        name: infer_worktree_name(&root_path, fallback_name),
        path: normalized_path,
        missing: !root_path.exists(),
    });
}

fn resolve_git_dir(repo_root: &Path, git_meta: &Path) -> Result<PathBuf, String> {
    if git_meta.is_dir() {
        return normalize_existing_or_absolute(git_meta);
    }

    if !git_meta.is_file() {
        return Err(format!(
            "unsupported .git entry type at {}",
            git_meta.display()
        ));
    }

    let contents = fs::read_to_string(git_meta)
        .map_err(|e| format!("failed to read {}: {e}", git_meta.display()))?;
    let gitdir_value = parse_gitdir_line(&contents)
        .ok_or_else(|| format!("invalid .git file format in {}", git_meta.display()))?;

    let resolved = resolve_relative_path(repo_root, gitdir_value);
    normalize_existing_or_absolute(&resolved)
}

fn parse_gitdir_line(contents: &str) -> Option<&str> {
    contents
        .lines()
        .find_map(|line| line.trim().strip_prefix("gitdir:"))
        .map(str::trim)
}

fn resolve_common_git_dir(git_dir: &Path) -> Result<PathBuf, String> {
    let common_dir_file = git_dir.join("commondir");
    if !common_dir_file.exists() {
        return Ok(git_dir.to_path_buf());
    }

    let raw = fs::read_to_string(&common_dir_file)
        .map_err(|e| format!("failed to read {}: {e}", common_dir_file.display()))?;
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Ok(git_dir.to_path_buf());
    }

    let resolved = resolve_relative_path(git_dir, trimmed);
    normalize_existing_or_absolute(&resolved)
}

fn infer_main_worktree_root(common_git_dir: &Path) -> Option<PathBuf> {
    let file_name = common_git_dir.file_name()?.to_str()?;
    if file_name != ".git" {
        return None;
    }

    common_git_dir.parent().map(Path::to_path_buf)
}

fn resolve_relative_path(base: &Path, raw_path: &str) -> PathBuf {
    let path = PathBuf::from(raw_path);
    if path.is_absolute() {
        path
    } else {
        base.join(path)
    }
}

fn normalize_existing_or_absolute(path: &Path) -> Result<PathBuf, String> {
    if path.exists() {
        fs::canonicalize(path)
            .map_err(|e| format!("failed to canonicalize {}: {e}", path.display()))
    } else if path.is_absolute() {
        Ok(path.to_path_buf())
    } else {
        normalize_path(&path.to_path_buf())
    }
}

fn normalize_path(path: &PathBuf) -> Result<PathBuf, String> {
    if path.exists() {
        fs::canonicalize(path)
            .map_err(|e| format!("failed to canonicalize {}: {e}", path.display()))
    } else if path.is_absolute() {
        Ok(path.clone())
    } else {
        let cwd =
            std::env::current_dir().map_err(|e| format!("failed to resolve current dir: {e}"))?;
        Ok(cwd.join(path))
    }
}

fn stderr_or_status(output: &std::process::Output) -> String {
    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
    if stderr.is_empty() {
        format!("exit status {}", output.status)
    } else {
        stderr
    }
}
