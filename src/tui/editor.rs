use crate::catalog::Lang;
use crate::state::config_dir;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

// ── Editor config ─────────────────────────────────────────────────────────────

/// One configured editor. Stored in ~/.grind-tui/editors.json.
/// - detach = false → TUI suspends, waits for the editor to exit, reads file back
/// - detach = true  → editor is spawned in background, TUI auto-syncs on file save
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EditorDef {
    /// Display name shown in the UI
    pub name: String,
    /// Single character key to press (e.g. "e", "C")
    pub key: String,
    /// Command to invoke (e.g. "vim", "cursor", "nvim", "hx")
    pub command: String,
    /// If true, spawn without waiting (Cursor / VS Code style)
    #[serde(default)]
    pub detach: bool,
}

fn default_editors() -> Vec<EditorDef> {
    vec![
        EditorDef { name: "nvim".into(),   key: "e".into(), command: "nvim".into(),   detach: false },
        EditorDef { name: "vim".into(),    key: "v".into(), command: "vim".into(),    detach: false },
        EditorDef { name: "Cursor".into(), key: "C".into(), command: "cursor".into(), detach: true  },
        EditorDef { name: "VSCode".into(), key: "V".into(), command: "code".into(),   detach: true  },
        EditorDef { name: "Zed".into(),    key: "Z".into(), command: "zed".into(),    detach: true  },
    ]
}

fn editors_config_path() -> PathBuf {
    config_dir().join("editors.json")
}

pub fn load_editors() -> Vec<EditorDef> {
    let path = editors_config_path();
    if !path.exists() {
        // Write defaults so the user can discover and edit the file
        if let Some(dir) = path.parent() { std::fs::create_dir_all(dir).ok(); }
        let defaults = default_editors();
        if let Ok(json) = serde_json::to_string_pretty(&defaults) {
            std::fs::write(&path, json).ok();
        }
        return defaults;
    }
    std::fs::read_to_string(&path)
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_else(default_editors)
}

const ALLOWED_EDITORS: &[&str] = &[
    "vim", "nvim", "nano", "hx", "helix",
    "code", "cursor", "zed", "emacs", "micro", "vi",
];

pub fn is_trusted(ed: &EditorDef) -> bool {
    let binary = Path::new(&ed.command)
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("");
    ALLOWED_EDITORS.contains(&binary)
}

// ── File helpers ──────────────────────────────────────────────────────────────

pub fn code_file_path(slug: &str, lang: Lang) -> PathBuf {
    config_dir().join(format!("{}.{}", slug, lang.ext()))
}
