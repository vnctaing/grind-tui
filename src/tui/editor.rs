use serde::{Deserialize, Serialize};

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
    ]
}

fn editors_config_path() -> std::path::PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| std::path::PathBuf::from("/tmp"))
        .join(".grind-tui")
        .join("editors.json")
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

// ── File helpers ──────────────────────────────────────────────────────────────

fn ext_for_lang(lang: &str) -> &'static str {
    match lang {
        "python3" | "python" => "py", "rust" => "rs", "javascript" => "js",
        "typescript" => "ts", "java" => "java", "cpp" => "cpp", "go" => "go", "c" => "c", _ => "txt",
    }
}

pub(super) fn code_file_path(slug: &str, lang: &str) -> String {
    let dir = dirs::home_dir().unwrap_or_else(|| std::path::PathBuf::from("/tmp")).join(".grind-tui");
    std::fs::create_dir_all(&dir).ok();
    dir.join(format!("{}.{}", slug, ext_for_lang(lang))).to_string_lossy().to_string()
}
