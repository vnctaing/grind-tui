use std::collections::{HashMap, HashSet};
use std::fs;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize)]
pub struct AppState {
    pub solved: HashSet<u32>,
    pub bookmarks: HashSet<u32>,
    pub last_problem_id: Option<u32>,
    pub notes: HashMap<u32, String>,
    pub language: String,
}

impl Default for AppState {
    fn default() -> Self {
        Self {
            solved: HashSet::new(),
            bookmarks: HashSet::new(),
            last_problem_id: None,
            notes: HashMap::new(),
            language: "python3".to_string(),
        }
    }
}

#[derive(Clone, PartialEq)]
pub enum AppMode {
    Classic,
    Disguise,
}

pub fn config_dir() -> std::path::PathBuf {
    dirs::home_dir().unwrap().join(".leetcode-tui")
}

impl AppState {
    pub fn load() -> Self {
        let path = config_dir().join("state.json");
        match fs::read_to_string(&path) {
            Ok(contents) => serde_json::from_str(&contents).unwrap_or_default(),
            Err(_) => Self::default(),
        }
    }

    pub fn save(&self) {
        let dir = config_dir();
        if let Err(e) = fs::create_dir_all(&dir) {
            eprintln!("Failed to create config dir: {}", e);
            return;
        }
        let path = dir.join("state.json");
        match serde_json::to_string_pretty(self) {
            Ok(contents) => {
                if let Err(e) = fs::write(&path, contents) {
                    eprintln!("Failed to save state: {}", e);
                }
            }
            Err(e) => eprintln!("Failed to serialize state: {}", e),
        }
    }
}
