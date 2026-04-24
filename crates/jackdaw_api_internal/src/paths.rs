use std::path::PathBuf;

pub fn config_dir() -> Option<PathBuf> {
    dirs::config_dir().map(|d| d.join("jackdaw"))
}

pub fn recent_file_path() -> Option<PathBuf> {
    config_dir().map(|d| d.join("recent.json"))
}

pub fn keybinds_path() -> Option<std::path::PathBuf> {
    config_dir().map(|d| d.join("keybinds.json"))
}
