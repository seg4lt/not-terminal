use iced::{Subscription, stream};
use std::collections::HashSet;
use std::path::Path;
use std::process::Command;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(crate) struct DiffWatchSpec {
    pub(crate) worktree_path: String,
    pub(crate) watch_paths: Vec<String>,
}

impl DiffWatchSpec {
    pub(crate) fn new(worktree_path: String, mut watch_paths: Vec<String>) -> Self {
        let mut seen = HashSet::new();
        watch_paths.retain(|path| seen.insert(path.clone()));
        watch_paths.sort();

        Self {
            worktree_path,
            watch_paths,
        }
    }
}

pub(crate) fn resolve_watch_paths(worktree_path: &str) -> Vec<String> {
    let mut watch_paths = vec![worktree_path.to_string()];

    if let Some(index_path) = resolve_git_index_path(worktree_path) {
        watch_paths.push(index_path);
    }

    DiffWatchSpec::new(worktree_path.to_string(), watch_paths).watch_paths
}

#[cfg(target_os = "macos")]
pub(crate) fn subscription(specs: Vec<DiffWatchSpec>) -> Subscription<String> {
    if specs.is_empty() {
        return Subscription::none();
    }

    Subscription::run_with(specs, watch_stream)
}

#[cfg(not(target_os = "macos"))]
pub(crate) fn subscription(_specs: Vec<DiffWatchSpec>) -> Subscription<String> {
    Subscription::none()
}

#[cfg(target_os = "macos")]
fn watch_stream(specs: &Vec<DiffWatchSpec>) -> iced::futures::stream::BoxStream<'static, String> {
    use iced::futures::SinkExt;
    use iced::futures::StreamExt;
    use notify::{RecursiveMode, Watcher};

    let specs = specs.clone();

    Box::pin(stream::channel(256, async move |mut output| {
        let (event_tx, mut event_rx) = iced::futures::channel::mpsc::unbounded::<String>();
        let mut watchers = Vec::new();

        for spec in specs {
            let worktree_path = spec.worktree_path.clone();
            let callback_tx = event_tx.clone();

            let mut watcher =
                match notify::recommended_watcher(move |event: notify::Result<notify::Event>| {
                    if event.is_ok() {
                        let _ = callback_tx.unbounded_send(worktree_path.clone());
                    }
                }) {
                    Ok(watcher) => watcher,
                    Err(_) => continue,
                };

            let mut watched_any_path = false;
            for watch_path in spec.watch_paths {
                let path = Path::new(&watch_path);
                let recursive_mode = if path.is_dir() {
                    RecursiveMode::Recursive
                } else {
                    RecursiveMode::NonRecursive
                };

                if watcher.watch(path, recursive_mode).is_ok() {
                    watched_any_path = true;
                }
            }

            if watched_any_path {
                watchers.push(watcher);
            }
        }

        drop(event_tx);

        while let Some(worktree_path) = event_rx.next().await {
            if output.send(worktree_path).await.is_err() {
                break;
            }
        }

        drop(watchers);
    }))
}

fn resolve_git_index_path(worktree_path: &str) -> Option<String> {
    let output = Command::new("git")
        .arg("-C")
        .arg(worktree_path)
        .arg("rev-parse")
        .arg("--path-format=absolute")
        .arg("--git-path")
        .arg("index")
        .output()
        .ok()?;

    if !output.status.success() {
        return None;
    }

    let raw_path = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if raw_path.is_empty() {
        return None;
    }
    Some(raw_path)
}
