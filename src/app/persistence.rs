use crate::app::model::PersistedState;
use std::fs;
use std::path::PathBuf;

pub(crate) fn load_state() -> Result<PersistedState, String> {
    let path = state_file_path()?;
    if !path.exists() {
        return Ok(PersistedState::default());
    }

    let raw =
        fs::read_to_string(&path).map_err(|e| format!("failed to read {}: {e}", path.display()))?;
    serde_json::from_str::<PersistedState>(&raw)
        .map_err(|e| format!("failed to parse {}: {e}", path.display()))
}

pub(crate) fn save_state(state: &PersistedState) -> Result<(), String> {
    let path = state_file_path()?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|e| format!("failed to create {}: {e}", parent.display()))?;
    }

    let content = serde_json::to_string_pretty(state)
        .map_err(|e| format!("failed to encode state json: {e}"))?;
    fs::write(&path, content).map_err(|e| format!("failed to write {}: {e}", path.display()))
}

pub(crate) fn state_file_path() -> Result<PathBuf, String> {
    let base = dirs::data_local_dir()
        .or_else(dirs::home_dir)
        .unwrap_or(std::env::current_dir().map_err(|e| format!("failed to resolve cwd: {e}"))?);

    Ok(base.join("elm-ghostty").join("state.json"))
}
