use crate::models::AppState;
use dirs_next::config_dir;
use std::fs;
use std::io::{Error, ErrorKind};
use std::path::{Path, PathBuf};

pub fn default_state_path() -> PathBuf {
    if let Some(dir) = config_dir() {
        return dir.join("rss-reader").join("state.json");
    }
    PathBuf::from("state.json")
}

pub fn load_state(path: &Path) -> AppState {
    let contents = match fs::read_to_string(path) {
        Ok(contents) => contents,
        Err(err) if err.kind() == ErrorKind::NotFound => return AppState::default(),
        Err(err) => {
            eprintln!("Failed to read state file {}: {err}", path.display());
            return AppState::default();
        }
    };

    let state: AppState = match serde_json::from_str(&contents) {
        Ok(state) => state,
        Err(err) => {
            eprintln!("Failed to parse state file {}: {err}", path.display());
            return AppState::default();
        }
    };

    let expected_version = AppState::default().version;
    if state.version != expected_version {
        eprintln!(
            "State version mismatch (found {}, expected {}) in {}",
            state.version,
            expected_version,
            path.display()
        );
        return AppState::default();
    }

    state
}

pub fn save_state(path: &Path, state: &AppState) -> Result<(), Error> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }

    let payload =
        serde_json::to_vec_pretty(state).map_err(|err| Error::new(ErrorKind::Other, err))?;

    let temp_path = path.with_extension("json.tmp");
    fs::write(&temp_path, payload)?;
    fs::rename(&temp_path, path)?;
    Ok(())
}
