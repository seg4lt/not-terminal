use serde::{Deserialize, Serialize};
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

static NEXT_ID: AtomicU64 = AtomicU64::new(1);

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub(crate) struct PersistedState {
    pub(crate) version: u32,
    pub(crate) active_project_id: Option<String>,
    pub(crate) projects: Vec<ProjectRecord>,
    pub(crate) ui: UiState,
}

impl Default for PersistedState {
    fn default() -> Self {
        Self {
            version: 1,
            active_project_id: None,
            projects: Vec::new(),
            ui: UiState::default(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default, rename_all = "camelCase")]
pub(crate) struct UiState {
    pub(crate) sidebar_collapsed: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default, rename_all = "camelCase")]
pub(crate) struct ProjectRecord {
    pub(crate) id: String,
    pub(crate) name: String,
    pub(crate) git_folder_path: Option<String>,
    pub(crate) worktrees: Vec<WorktreeRecord>,
    pub(crate) tree_state: TreeStateRecord,
    pub(crate) selected_terminal_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default, rename_all = "camelCase")]
pub(crate) struct WorktreeRecord {
    pub(crate) id: String,
    pub(crate) name: String,
    pub(crate) path: String,
    pub(crate) missing: bool,
    pub(crate) terminals: Vec<TerminalRecord>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default, rename_all = "camelCase")]
pub(crate) struct TerminalRecord {
    pub(crate) id: String,
    pub(crate) name: String,
    pub(crate) manual_name: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default, rename_all = "camelCase")]
pub(crate) struct TreeStateRecord {
    pub(crate) collapsed_projects: Vec<String>,
    pub(crate) collapsed_worktrees: Vec<String>,
}

#[derive(Debug, Clone)]
pub(crate) struct WorktreeInfo {
    pub(crate) id: String,
    pub(crate) name: String,
    pub(crate) path: String,
    pub(crate) missing: bool,
}

pub(crate) fn create_id(prefix: &str) -> String {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|value| value.as_micros() as u64)
        .unwrap_or_default();
    let seq = NEXT_ID.fetch_add(1, Ordering::Relaxed);
    format!("{prefix}-{now:x}-{seq:x}")
}

pub(crate) fn infer_project_name(path: &str) -> String {
    Path::new(path)
        .file_name()
        .and_then(|value| value.to_str())
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
        .unwrap_or_else(|| String::from("Project"))
}

pub(crate) fn infer_worktree_name(path: &Path, fallback: &str) -> String {
    path.file_name()
        .and_then(|value| value.to_str())
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
        .unwrap_or_else(|| fallback.to_string())
}

pub(crate) fn next_project_name(projects: &[ProjectRecord]) -> String {
    format!("Project {}", projects.len() + 1)
}

pub(crate) fn next_terminal_name(terminals: &[TerminalRecord]) -> String {
    format!("Terminal {}", terminals.len() + 1)
}
