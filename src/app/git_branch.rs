use std::process::Command;

pub(crate) fn resolve_branch(worktree_path: &str) -> Option<String> {
    if let Some(branch) = run_git(worktree_path, &["branch", "--show-current"])
        && !branch.is_empty()
    {
        return Some(branch);
    }

    if let Some(branch) = run_git(worktree_path, &["symbolic-ref", "--short", "HEAD"])
        && !branch.is_empty()
    {
        return Some(branch);
    }

    run_git(worktree_path, &["rev-parse", "--short", "HEAD"]).map(|sha| format!("detached@{sha}"))
}

fn run_git(worktree_path: &str, args: &[&str]) -> Option<String> {
    let output = Command::new("git")
        .arg("-C")
        .arg(worktree_path)
        .args(args)
        .output()
        .ok()?;

    if !output.status.success() {
        return None;
    }

    let value = String::from_utf8(output.stdout).ok()?;
    let value = value.trim().to_string();
    if value.is_empty() { None } else { Some(value) }
}
