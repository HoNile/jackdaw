//! Persistence for the "enabled extensions" list.
//!
//! Stores a JSON file at `~/.config/jackdaw/extensions.json`:
//!
//! ```json
//! {
//!   "enabled": [
//!     "jackdaw.core_windows",
//!     "jackdaw.inspector",
//!     "sample"
//!   ]
//! }
//! ```
//!
//! Read once on editor startup to decide which entries in the
//! `ExtensionCatalog` to enable. Re-written whenever the user toggles an
//! extension in the Plugins dialog.

use std::collections::HashSet;
use std::path::PathBuf;

use bevy::prelude::*;
use jackdaw_api::{ExtensionCatalog, enable_extension};
use serde::{Deserialize, Serialize};

/// On-disk shape.
#[derive(Serialize, Deserialize, Default)]
pub struct ExtensionsConfig {
    pub enabled: Vec<String>,
}

fn config_path() -> Option<PathBuf> {
    crate::project::config_dir().map(|d| d.join("extensions.json"))
}

/// Read the enabled-list from disk. Returns `None` if the file doesn't
/// exist — callers should interpret that as "enable everything".
pub fn read_enabled_list() -> Option<Vec<String>> {
    let path = config_path()?;
    let data = std::fs::read_to_string(&path).ok()?;
    let config: ExtensionsConfig = serde_json::from_str(&data).ok()?;
    Some(config.enabled)
}

/// Write the currently-enabled list to disk.
pub fn write_enabled_list(enabled: &[String]) {
    let Some(path) = config_path() else {
        return;
    };
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let config = ExtensionsConfig {
        enabled: enabled.to_vec(),
    };
    if let Ok(data) = serde_json::to_string_pretty(&config) {
        let _ = std::fs::write(&path, data);
    }
}

/// Enable every extension in the catalog whose name is in the persisted
/// list. Falls back to "enable all" if the config file doesn't exist yet
/// (first-run behavior).
pub fn apply_enabled_from_disk(app: &mut App) {
    let world = app.world_mut();
    let to_enable: Vec<String> = {
        let catalog = world.resource::<ExtensionCatalog>();
        let available: Vec<String> = catalog.iter().map(|s| s.to_string()).collect();
        match read_enabled_list() {
            Some(list) => {
                let set: HashSet<String> = list.into_iter().collect();
                available.into_iter().filter(|n| set.contains(n)).collect()
            }
            None => available, // first run: enable everything
        }
    };

    for name in &to_enable {
        enable_extension(world, name);
    }
}

/// Compute the current enabled list from the loaded `Extension` entities
/// and write it to disk.
pub fn persist_current_enabled(world: &mut World) {
    let mut query = world.query::<&jackdaw_api::Extension>();
    let enabled: Vec<String> = query.iter(world).map(|e| e.name.clone()).collect();
    write_enabled_list(&enabled);
}
