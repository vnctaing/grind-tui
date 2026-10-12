use super::code_editor::{CodeEditor, Effect};
use super::editor::{code_file_path, is_trusted, EditorDef};
use crate::catalog::{Catalog, Difficulty, ListId, Problem};
use crate::state::AppState;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::widgets::ListState;
use std::path::{Path, PathBuf};
use std::time::{Instant, SystemTime};

// ── Shared types ──────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Panel { Description, Code, Output }

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ListFilter { All, Only(ListId) }

impl ListFilter {
    pub fn next(self) -> ListFilter {
        match self {
            ListFilter::All => ListFilter::Only(ListId::Grind75),
            ListFilter::Only(ListId::Grind75) => ListFilter::Only(ListId::NeetCode150),
            ListFilter::Only(ListId::NeetCode150) => ListFilter::All,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            ListFilter::All => "All",
            ListFilter::Only(ListId::Grind75) => "Grind 75",
            ListFilter::Only(ListId::NeetCode150) => "NeetCode 150",
        }
    }
}

#[derive(Debug, Default, Clone, Copy)]
pub struct Collapsed { pub desc: bool, pub code: bool, pub output: bool }

#[derive(Debug, Default, Clone, Copy)]
pub struct Scroll { pub desc: u16, pub output: u16 }

/// In `Edit`, keys go to the code editor, so page commands like `q` type text.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CodeMode { Navigate, Edit }

pub const UNSAVED: &str = "Unsaved edits. Enter then Ctrl-S saves them, y twice reloads from disk and discards them.";
const CONFLICT: &str = "File changed on disk while you have unsaved edits.\nCtrl-S overwrites it, Ctrl-R twice reloads it (discards your edits).";

/// A key that discards unsaved edits, pressed once. The same key again
/// confirms; any other input cancels.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Confirm { Reload, Sync, Discard }

impl Confirm {
    fn prompt(self) -> &'static str {
        match self {
            Confirm::Reload => "Unsaved changes, press Ctrl-R again to discard them and reload.",
            Confirm::Sync => "Unsaved changes, press y again to discard them and reload.",
            Confirm::Discard => "Unsaved changes will be lost, press Ctrl-Q again.",
        }
    }
}

pub struct ProblemView {
    pub idx: usize,
    pub editor: CodeEditor,
    pub mode: CodeMode,
    /// The code file is watched while the page is open, so edits made from
    /// another terminal tab sync too, not only those from a detached editor.
    pub path: PathBuf,
    pub mtime: Option<SystemTime>,
    /// Disk mtime of an external change the user was told about while holding
    /// unsaved edits. A save overwrites only a change the user has seen.
    pub conflict: Option<SystemTime>,
    pub armed: Option<Confirm>,
    pub focused: Panel,
    pub collapsed: Collapsed,
    pub scroll: Scroll,
    /// Name of the editor spawned in the background, if any.
    pub detached: Option<String>,
    pub message: String,
}

impl ProblemView {
    fn open(idx: usize, problem: &Problem) -> ProblemView {
        let (path, code) = load_code(problem);
        ProblemView::new(idx, path, &code)
    }

    pub fn new(idx: usize, path: PathBuf, code: &str) -> ProblemView {
        let mut view = ProblemView {
            idx,
            editor: CodeEditor::new(code),
            mode: CodeMode::Navigate,
            mtime: modified(&path),
            path,
            conflict: None,
            armed: None,
            focused: Panel::Code,
            collapsed: Collapsed::default(),
            scroll: Scroll::default(),
            detached: None,
            message: String::new(),
        };
        view.loaded("");
        view
    }

    /// Reports `done`, plus a note when the file mixed line endings.
    fn loaded(&mut self, done: &str) {
        let note = self.editor.mixed_endings()
            .map(|e| format!("Mixed line endings, normalized to {} on save.", e.label()));
        self.message = match note {
            Some(note) if done.is_empty() => note,
            Some(note) => format!("{done}\n{note}"),
            None => done.to_string(),
        };
    }

    pub fn reload(&mut self, done: &str) -> bool {
        match std::fs::read_to_string(&self.path) {
            Ok(text) => {
                self.editor.reload(&text);
                self.mtime = modified(&self.path);
                self.conflict = None;
                self.loaded(done);
                true
            }
            Err(e) => {
                self.mtime = modified(&self.path);
                self.message = format!("Reload failed: {e}");
                false
            }
        }
    }

    pub fn disarm(&mut self) {
        self.armed = None;
    }

    fn arm(&mut self, confirm: Confirm) {
        self.armed = Some(confirm);
        self.message = confirm.prompt().to_string();
    }

    /// Pulls in an external change, unless that would discard unsaved edits.
    pub fn poll_disk(&mut self) {
        if !self.is_stale() {
            return;
        }
        if self.editor.is_dirty() {
            let disk = modified(&self.path);
            if self.conflict != disk {
                self.conflict = disk;
                self.message = CONFLICT.to_string();
            }
        } else {
            let done = format!("Auto-synced from {}.", self.detached.as_deref().unwrap_or("disk"));
            self.reload(&done);
        }
    }

    /// A clean buffer is never written, so saving cannot rewrite a file the
    /// user did not change, such as one with mixed line endings.
    pub fn save(&mut self) -> bool {
        if !self.editor.is_dirty() {
            self.message = "No changes to save.".to_string();
            return true;
        }
        let disk = modified(&self.path);
        if self.is_stale() && self.conflict != disk {
            self.conflict = disk;
            self.message = CONFLICT.to_string();
            return false;
        }
        match std::fs::write(&self.path, self.editor.contents().as_bytes()) {
            Ok(()) => {
                self.mtime = modified(&self.path);
                self.conflict = None;
                self.editor.mark_saved();
                self.message = format!("Saved {}.", self.path.display());
                true
            }
            Err(e) => {
                self.message = format!("Save failed: {e}");
                false
            }
        }
    }

    pub fn start_editing(&mut self) {
        self.collapsed.code = false;
        self.focused = Panel::Code;
        self.mode = CodeMode::Edit;
    }

    /// Leaving edit mode saves, but never over an external change: the user
    /// stays in edit mode to choose between Ctrl-S and Ctrl-R.
    pub fn stop_editing(&mut self) {
        if self.editor.is_dirty() {
            if self.conflict.is_some() {
                self.message = CONFLICT.to_string();
                return;
            }
            if !self.save() {
                return;
            }
        }
        self.mode = CodeMode::Navigate;
    }

    /// The way out when neither saving nor reloading works.
    pub fn discard(&mut self) {
        if self.editor.is_dirty() {
            self.editor.discard();
            self.message = "Discarded unsaved changes.".to_string();
        }
        self.conflict = None;
        self.mode = CodeMode::Navigate;
    }

    /// True when the watched file changed on disk since the last read.
    pub fn is_stale(&self) -> bool {
        match (modified(&self.path), self.mtime) {
            (Some(now), Some(prev)) => now > prev,
            (Some(_), None) => true,
            (None, _) => false,
        }
    }

    pub fn scroll_by(&mut self, delta: i16) {
        let s = match self.focused {
            Panel::Description => &mut self.scroll.desc,
            Panel::Code => return self.editor.scroll_by(isize::from(delta)),
            Panel::Output => &mut self.scroll.output,
        };
        *s = s.saturating_add_signed(delta);
    }
}

fn modified(path: &Path) -> Option<SystemTime> {
    std::fs::metadata(path).ok().and_then(|m| m.modified().ok())
}

/// Saved work wins; otherwise the starter is seeded to disk. A problem without
/// a starter (Premium) never writes, so an existing file is never clobbered.
fn load_code(problem: &Problem) -> (PathBuf, String) {
    let path = code_file_path(&problem.slug);
    if let Ok(saved) = std::fs::read_to_string(&path) {
        if !saved.trim().is_empty() {
            return (path, saved);
        }
    }
    let starter = problem.starter();
    if !starter.is_empty() {
        if let Some(dir) = path.parent() { std::fs::create_dir_all(dir).ok(); }
        std::fs::write(&path, starter).ok();
    }
    (path, starter.to_string())
}

pub enum Page { List, Problem(ProblemView) }

pub enum Action { Quit, LaunchEditor(EditorDef), CopyCommand, Clipboard(String), None }

pub struct App {
    pub catalog: Catalog,
    pub progress: AppState,
    pub editors: Vec<EditorDef>,
    pub filter: ListFilter,
    pub difficulty: Option<Difficulty>,
    pub query: String,
    pub searching: bool,
    pub filtered: Vec<usize>,
    pub list_state: ListState,
    pub page: Page,
}

impl App {
    pub fn new(
        catalog: Catalog,
        progress: AppState,
        editors: Vec<EditorDef>,
        filter: ListFilter,
        difficulty: Option<Difficulty>,
    ) -> App {
        let mut app = App {
            catalog,
            progress,
            editors,
            filter,
            difficulty,
            query: String::new(),
            searching: false,
            filtered: vec![],
            list_state: ListState::default(),
            page: Page::List,
        };
        app.refilter();
        let last = app.progress.last_problem_id;
        if let Some(row) = app.filtered.iter().position(|&i| Some(app.catalog.problems()[i].id) == last) {
            app.list_state.select(Some(row));
        }
        app
    }

    pub fn refilter(&mut self) {
        let problems = self.catalog.problems();
        let base: Vec<usize> = match self.filter {
            ListFilter::All => {
                let neetcode_only = self.catalog.list(ListId::NeetCode150).iter()
                    .filter(|&&i| !self.catalog.in_list(i, ListId::Grind75));
                self.catalog.list(ListId::Grind75).iter().chain(neetcode_only).copied().collect()
            }
            ListFilter::Only(id) => self.catalog.list(id).to_vec(),
        };
        let q = self.query.to_lowercase();
        self.filtered = base.into_iter()
            .filter(|&i| {
                let p = &problems[i];
                self.difficulty.is_none_or(|d| p.difficulty == d)
                    && (q.is_empty() || p.title.to_lowercase().contains(&q) || p.id.0.to_string().contains(&q))
            })
            .collect();
        self.list_state = ListState::default()
            .with_selected(if self.filtered.is_empty() { None } else { Some(0) });
    }

    pub fn selected(&self) -> Option<usize> {
        self.list_state.selected().and_then(|row| self.filtered.get(row)).copied()
    }

    pub fn open_selected(&mut self) {
        if let Some(idx) = self.selected() {
            let view = ProblemView::open(idx, &self.catalog.problems()[idx]);
            self.page = Page::Problem(view);
        }
    }

    pub fn toggle_solved(&mut self) {
        let idx = match &self.page {
            Page::Problem(view) => Some(view.idx),
            Page::List => self.selected(),
        };
        let Some(idx) = idx else { return };
        let id = self.catalog.problems()[idx].id;
        self.progress.toggle_solved(id);
        let solved = self.progress.solved.contains(&id);
        if let Page::Problem(view) = &mut self.page {
            view.message = if solved { "Marked solved.".into() } else { "Marked unsolved.".into() };
        }
    }

    pub fn solved_count(&self) -> usize {
        self.catalog.problems().iter().filter(|p| self.progress.solved.contains(&p.id)).count()
    }

    pub fn is_solved(&self, idx: usize) -> bool {
        self.progress.solved.contains(&self.catalog.problems()[idx].id)
    }
}

// ── Help-bar keys ─────────────────────────────────────────────────────────────

pub fn list_keys(app: &App) -> Vec<(&'static str, &'static str)> {
    if app.searching {
        return vec![("Enter", "apply"), ("Esc", "close")];
    }
    vec![
        ("j/k", "move"),
        ("Enter", "open"),
        ("/", "search"),
        ("t", app.filter.label()),
        ("x", "solved"),
        ("q", "quit"),
    ]
}

pub fn problem_keys(app: &App, view: &ProblemView) -> Vec<(&'static str, &'static str)> {
    let has_diagnostics = !view.editor.diagnostics().is_empty();
    if view.mode == CodeMode::Edit {
        let mut keys = vec![
            ("Esc", "done"),
            ("^S", "save"),
            ("^Z", "undo"),
            ("^Y", "redo"),
            ("^C/^X/^V", "copy/cut/paste"),
            ("^A", "all"),
            ("^R", "reload"),
            ("^Q^Q", "discard"),
        ];
        if has_diagnostics {
            keys.insert(2, ("F8", "next error"));
        }
        return keys;
    }
    let mut keys = vec![
        ("b", "back"),
        ("q", "quit"),
        ("Enter", "edit"),
        ("x", if app.is_solved(view.idx) { "unsolve" } else { "solved" }),
        ("p", "desc"),
        ("o", "output"),
        ("c", if view.collapsed.code { "show code" } else { "hide code" }),
        ("Tab", "focus"),
        ("y", "sync"),
        ("Y", "yank"),
    ];
    if has_diagnostics {
        keys.insert(3, ("F8", "next error"));
    }
    keys
}

// ── Key handling ──────────────────────────────────────────────────────────────

pub fn handle_key(app: &mut App, key: KeyEvent) -> Action {
    match &mut app.page {
        Page::List => handle_list_key(app, key.code),
        Page::Problem(view) if view.mode == CodeMode::Edit => handle_edit_key(view, key),
        Page::Problem(_) => handle_problem_key(app, key.code),
    }
}

pub fn handle_paste(app: &mut App, text: &str) {
    let Page::Problem(view) = &mut app.page else { return };
    view.armed = None;
    if view.mode == CodeMode::Edit {
        view.editor.paste(text, Instant::now());
    }
}

fn handle_edit_key(view: &mut ProblemView, key: KeyEvent) -> Action {
    let ctrl = key.modifiers.contains(KeyModifiers::CONTROL) && !key.modifiers.contains(KeyModifiers::ALT);
    let confirm = match key.code {
        KeyCode::Char('r' | 'R') if ctrl => Some(Confirm::Reload),
        KeyCode::Char('q' | 'Q') if ctrl => Some(Confirm::Discard),
        _ => None,
    };
    let armed = view.armed.take();
    if key.code == KeyCode::Esc {
        view.stop_editing();
        return Action::None;
    }
    match confirm {
        Some(c) if view.editor.is_dirty() && armed != Some(c) => view.arm(c),
        Some(Confirm::Discard) => view.discard(),
        Some(Confirm::Reload | Confirm::Sync) => { view.reload("Reloaded from disk."); }
        None => match view.editor.handle_key(key, Instant::now()) {
            Effect::None => {}
            Effect::Save => { view.save(); }
            Effect::Copy(text) => return Action::Clipboard(text),
        },
    }
    Action::None
}

fn handle_list_key(app: &mut App, key: KeyCode) -> Action {
    if app.searching {
        match key {
            KeyCode::Esc | KeyCode::Enter => { app.searching = false; }
            KeyCode::Backspace => { app.query.pop(); app.refilter(); }
            KeyCode::Char(c) => { app.query.push(c); app.refilter(); }
            _ => {}
        }
        return Action::None;
    }
    match key {
        KeyCode::Char('q') => return Action::Quit,
        KeyCode::Char('/') => { app.searching = true; app.query.clear(); app.refilter(); }
        KeyCode::Char('t') => { app.filter = app.filter.next(); app.refilter(); }
        KeyCode::Char('x') => app.toggle_solved(),
        KeyCode::Enter => app.open_selected(),
        KeyCode::Char('j') | KeyCode::Down => {
            if let Some(row) = app.list_state.selected() {
                app.list_state.select(Some((row + 1).min(app.filtered.len() - 1)));
            }
        }
        KeyCode::Char('k') | KeyCode::Up => {
            if let Some(row) = app.list_state.selected() {
                app.list_state.select(Some(row.saturating_sub(1)));
            }
        }
        _ => {}
    }
    Action::None
}

fn handle_problem_key(app: &mut App, key: KeyCode) -> Action {
    let launches = |c: char| app.editors.iter().any(|e| e.key == c.to_string());
    let would_drop_edits = matches!(key, KeyCode::Char(c) if matches!(c, 'q' | 'b') || launches(c));
    let Page::Problem(view) = &mut app.page else { return Action::None };
    let armed = view.armed.take();
    if would_drop_edits && view.editor.is_dirty() {
        view.message = UNSAVED.to_string();
        return Action::None;
    }
    if key == KeyCode::Char('y') {
        if view.editor.is_dirty() && armed != Some(Confirm::Sync) {
            view.arm(Confirm::Sync);
        } else {
            view.reload("Synced.");
        }
        return Action::None;
    }
    if let KeyCode::Char(c) = key {
        if let Some(ed) = app.editors.iter().find(|e| e.key == c.to_string()).cloned() {
            if is_trusted(&ed) {
                return Action::LaunchEditor(ed);
            }
            if let Page::Problem(view) = &mut app.page {
                view.message = format!("Blocked: '{}' is not a trusted editor. Edit ~/.grind-tui/editors.json to use a known editor.", ed.command);
            }
            return Action::None;
        }
    }
    match key {
        KeyCode::Char('q') => return Action::Quit,
        KeyCode::Char('b') => { app.page = Page::List; return Action::None; }
        KeyCode::Char('x') => { app.toggle_solved(); return Action::None; }
        KeyCode::Char('Y') => return Action::CopyCommand,
        _ => {}
    }
    let Page::Problem(view) = &mut app.page else { return Action::None };
    match key {
        KeyCode::Enter => view.start_editing(),
        KeyCode::F(8) => {
            view.collapsed.code = false;
            view.editor.next_diagnostic();
        }
        KeyCode::Tab => {
            view.focused = match view.focused {
                Panel::Code => Panel::Description,
                Panel::Description => Panel::Output,
                Panel::Output => Panel::Code,
            };
        }
        KeyCode::Char('c') => view.collapsed.code = !view.collapsed.code,
        KeyCode::Char('p') => {
            if view.collapsed.desc { view.collapsed.desc = false; view.focused = Panel::Description; }
            else if view.focused == Panel::Description { view.collapsed.desc = true; view.focused = Panel::Code; }
            else { view.focused = Panel::Description; }
        }
        KeyCode::Char('o') => {
            if view.collapsed.output { view.collapsed.output = false; view.focused = Panel::Output; }
            else if view.focused == Panel::Output { view.collapsed.output = true; view.focused = Panel::Code; }
            else { view.focused = Panel::Output; }
        }
        KeyCode::Char('j') | KeyCode::Down => view.scroll_by(1),
        KeyCode::Char('k') | KeyCode::Up => view.scroll_by(-1),
        _ => {}
    }
    Action::None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalog::ProblemId;

    fn app() -> App {
        App::new(Catalog::embedded(), AppState::default(), vec![], ListFilter::All, None)
    }

    fn press(app: &mut App, keys: &str) {
        for c in keys.chars() {
            handle_key(app, KeyEvent::from(KeyCode::Char(c)));
        }
    }

    #[test]
    fn search_then_clear_restores_full_list_from_the_top() {
        let mut app = app();
        press(&mut app, "/tree");
        let matches = app.filtered.len();
        assert!(matches > 0 && matches < 167);
        for _ in 0..4 {
            handle_key(&mut app, KeyEvent::from(KeyCode::Backspace));
        }
        handle_key(&mut app, KeyEvent::from(KeyCode::Enter));
        assert_eq!(app.filtered.len(), 167);
        assert_eq!(app.list_state.offset(), 0);
        assert_eq!(app.list_state.selected(), Some(0));
    }

    #[test]
    fn search_resets_a_stale_scroll_offset() {
        let mut app = app();
        app.list_state.select(Some(100));
        *app.list_state.offset_mut() = 90;
        press(&mut app, "/tr");
        handle_key(&mut app, KeyEvent::from(KeyCode::Enter));
        assert_eq!(app.filtered.len(), 41);
        assert_eq!(app.list_state.selected(), Some(0));
        assert_eq!(app.list_state.offset(), 0);
    }

    #[test]
    fn filter_cycles_through_lists_and_resets_selection() {
        let mut app = app();
        app.list_state.select(Some(160));
        *app.list_state.offset_mut() = 140;
        let mut counts = vec![app.filtered.len()];
        for _ in 0..3 {
            press(&mut app, "t");
            assert_eq!(app.list_state.offset(), 0);
            assert_eq!(app.list_state.selected(), Some(0));
            counts.push(app.filtered.len());
        }
        assert_eq!(counts, vec![167, 75, 150, 167]);
    }

    #[test]
    fn toggle_solved_twice_restores_the_set() {
        let mut app = app();
        app.progress.solved = [1, 349].into_iter().map(ProblemId).collect();
        let before = app.progress.solved.clone();
        press(&mut app, "j");
        press(&mut app, "x");
        assert_ne!(app.progress.solved, before);
        press(&mut app, "x");
        assert_eq!(app.progress.solved, before);
    }

    #[test]
    fn solved_id_outside_the_catalog_survives_toggle_and_round_trip() {
        let progress: AppState =
            serde_json::from_str(r#"{"solved":[1,349],"last_problem_id":20,"language":"javascript"}"#).unwrap();
        let mut app = App::new(Catalog::embedded(), progress, vec![], ListFilter::All, None);
        assert_eq!(app.solved_count(), 1);
        let other = app.selected().unwrap();
        assert_eq!(app.catalog.problems()[other].id, ProblemId(20));
        press(&mut app, "x");

        let json = serde_json::to_string(&app.progress).unwrap();
        let reloaded: AppState = serde_json::from_str(&json).unwrap();
        let expected: std::collections::HashSet<ProblemId> =
            [1, 349, 20].into_iter().map(ProblemId).collect();
        assert_eq!(reloaded.solved, expected);
    }

    #[test]
    fn difficulty_filter_narrows_every_list() {
        let app = App::new(Catalog::embedded(), AppState::default(), vec![], ListFilter::All, Some(Difficulty::Hard));
        assert!(!app.filtered.is_empty());
        assert!(app.filtered.iter().all(|&i| app.catalog.problems()[i].difficulty == Difficulty::Hard));
    }

    struct TempDir(PathBuf);

    impl TempDir {
        fn new(name: &str) -> TempDir {
            let dir = std::env::temp_dir().join(format!("grind-tui-{name}-{}", std::process::id()));
            std::fs::create_dir_all(&dir).unwrap();
            TempDir(dir)
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            std::fs::remove_dir_all(&self.0).ok();
        }
    }

    fn app_with_file(dir: &TempDir, contents: &str) -> (App, PathBuf) {
        let mut app = app();
        let path = dir.0.join("two-sum.js");
        std::fs::write(&path, contents).unwrap();
        app.page = Page::Problem(ProblemView::new(0, path.clone(), contents));
        (app, path)
    }

    fn view(app: &mut App) -> &mut ProblemView {
        match &mut app.page {
            Page::Problem(view) => view,
            Page::List => panic!("not on a problem page"),
        }
    }

    fn key(app: &mut App, code: KeyCode, mods: KeyModifiers) -> Action {
        handle_key(app, KeyEvent::new(code, mods))
    }

    fn touch_later(path: &Path, contents: &str) {
        std::fs::write(path, contents).unwrap();
        let later = SystemTime::now() + std::time::Duration::from_secs(60);
        std::fs::File::options().write(true).open(path).unwrap().set_modified(later).unwrap();
    }

    #[test]
    fn enter_switches_to_edit_mode_where_page_letters_type_text() {
        let dir = TempDir::new("mode");
        let (mut app, path) = app_with_file(&dir, "");
        press(&mut app, "x");
        assert!(!app.progress.solved.is_empty());
        key(&mut app, KeyCode::Enter, KeyModifiers::NONE);
        assert_eq!(view(&mut app).mode, CodeMode::Edit);
        let solved = app.progress.solved.clone();
        for c in "qbxlc".chars() {
            assert!(matches!(key(&mut app, KeyCode::Char(c), KeyModifiers::NONE), Action::None));
        }
        assert_eq!(app.progress.solved, solved);
        assert_eq!(view(&mut app).editor.text(), "qbxlc");
        assert!(!view(&mut app).collapsed.code);

        key(&mut app, KeyCode::Esc, KeyModifiers::NONE);
        assert_eq!(view(&mut app).mode, CodeMode::Navigate);
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "qbxlc");
        assert!(matches!(key(&mut app, KeyCode::Char('q'), KeyModifiers::NONE), Action::Quit));
    }

    #[test]
    fn ctrl_s_writes_the_buffer_and_the_poll_does_not_reload_our_own_write() {
        let dir = TempDir::new("save");
        let (mut app, path) = app_with_file(&dir, "a");
        key(&mut app, KeyCode::Enter, KeyModifiers::NONE);
        key(&mut app, KeyCode::End, KeyModifiers::CONTROL);
        press(&mut app, "bc");
        key(&mut app, KeyCode::Char('s'), KeyModifiers::CONTROL);
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "abc");
        let v = view(&mut app);
        assert!(!v.editor.is_dirty());
        assert!(!v.is_stale());
        assert_eq!(v.mode, CodeMode::Edit);
    }

    #[test]
    fn external_change_reloads_a_clean_buffer() {
        let dir = TempDir::new("reload");
        let (mut app, path) = app_with_file(&dir, "old");
        touch_later(&path, "new");
        view(&mut app).poll_disk();
        assert_eq!(view(&mut app).editor.text(), "new");
    }

    #[test]
    fn external_change_never_clobbers_unsaved_edits() {
        let dir = TempDir::new("conflict");
        let (mut app, path) = app_with_file(&dir, "mine");
        key(&mut app, KeyCode::Enter, KeyModifiers::NONE);
        press(&mut app, "!");
        touch_later(&path, "theirs");

        view(&mut app).poll_disk();
        assert_eq!(view(&mut app).editor.text(), "!mine");
        assert!(view(&mut app).message.contains("changed on disk"));
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "theirs");

        key(&mut app, KeyCode::Char('s'), KeyModifiers::CONTROL);
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "!mine");
    }

    #[test]
    fn save_refuses_once_on_an_unseen_disk_change() {
        let dir = TempDir::new("unseen");
        let (mut app, path) = app_with_file(&dir, "mine");
        key(&mut app, KeyCode::Enter, KeyModifiers::NONE);
        press(&mut app, "!");
        touch_later(&path, "theirs");

        key(&mut app, KeyCode::Esc, KeyModifiers::NONE);
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "theirs");
        assert!(view(&mut app).editor.is_dirty());
        assert_eq!(view(&mut app).mode, CodeMode::Edit);

        key(&mut app, KeyCode::Char('r'), KeyModifiers::CONTROL);
        key(&mut app, KeyCode::Char('r'), KeyModifiers::CONTROL);
        assert_eq!(view(&mut app).editor.text(), "theirs");
        assert!(!view(&mut app).editor.is_dirty());
    }

    #[test]
    fn esc_never_overwrites_a_seen_disk_change() {
        let dir = TempDir::new("seen");
        let (mut app, path) = app_with_file(&dir, "mine");
        key(&mut app, KeyCode::Enter, KeyModifiers::NONE);
        press(&mut app, "!");
        touch_later(&path, "theirs");
        view(&mut app).poll_disk();

        key(&mut app, KeyCode::Esc, KeyModifiers::NONE);
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "theirs");
        let v = view(&mut app);
        assert!(v.editor.is_dirty());
        assert_eq!(v.mode, CodeMode::Edit);
        assert!(v.message.contains("changed on disk"));

        key(&mut app, KeyCode::Char('s'), KeyModifiers::CONTROL);
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "!mine");
        key(&mut app, KeyCode::Esc, KeyModifiers::NONE);
        assert_eq!(view(&mut app).mode, CodeMode::Navigate);
    }

    #[test]
    fn ctrl_r_over_unsaved_edits_needs_a_second_press() {
        let dir = TempDir::new("confirm");
        let (mut app, path) = app_with_file(&dir, "mine");
        key(&mut app, KeyCode::Enter, KeyModifiers::NONE);
        press(&mut app, "!");
        std::fs::write(&path, "theirs").unwrap();

        key(&mut app, KeyCode::Char('r'), KeyModifiers::CONTROL);
        assert_eq!(view(&mut app).editor.text(), "!mine");
        assert!(view(&mut app).message.contains("Ctrl-R again"));
        key(&mut app, KeyCode::Right, KeyModifiers::NONE);
        key(&mut app, KeyCode::Char('r'), KeyModifiers::CONTROL);
        assert_eq!(view(&mut app).editor.text(), "!mine");

        key(&mut app, KeyCode::Char('r'), KeyModifiers::CONTROL);
        assert_eq!(view(&mut app).editor.text(), "theirs");
        std::fs::write(&path, "again").unwrap();
        key(&mut app, KeyCode::Char('r'), KeyModifiers::CONTROL);
        assert_eq!(view(&mut app).editor.text(), "again");
    }

    #[test]
    fn ctrl_alt_r_types_instead_of_reloading() {
        let dir = TempDir::new("altgr");
        let (mut app, _) = app_with_file(&dir, "");
        key(&mut app, KeyCode::Enter, KeyModifiers::NONE);
        key(&mut app, KeyCode::Char('r'), KeyModifiers::CONTROL | KeyModifiers::ALT);
        assert_eq!(view(&mut app).editor.text(), "r");
    }

    #[test]
    fn save_keeps_crlf_line_endings() {
        let dir = TempDir::new("crlf");
        let (mut app, path) = app_with_file(&dir, "a\r\nb");
        key(&mut app, KeyCode::Enter, KeyModifiers::NONE);
        press(&mut app, "x");
        key(&mut app, KeyCode::Char('s'), KeyModifiers::CONTROL);
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "xa\r\nb");
    }

    #[test]
    fn dirty_buffer_blocks_external_editors() {
        let dir = TempDir::new("guard");
        let (mut app, path) = dirty_in_navigate(&dir);
        app.editors = vec![EditorDef { name: "nvim".into(), key: "e".into(), command: "nvim".into(), detach: false }];

        assert!(matches!(key(&mut app, KeyCode::Char('e'), KeyModifiers::NONE), Action::None));
        assert_eq!(view(&mut app).path, path);
        assert_eq!(view(&mut app).editor.text(), "!mine");
        assert!(view(&mut app).message.contains("Unsaved"));

        view(&mut app).editor.mark_saved();
        assert!(matches!(key(&mut app, KeyCode::Char('e'), KeyModifiers::NONE), Action::LaunchEditor(_)));
    }

    /// No key path leaves edit mode with unsaved edits, so the Navigate-mode
    /// guards are reached by switching the mode directly.
    fn dirty_in_navigate(dir: &TempDir) -> (App, PathBuf) {
        let (mut app, path) = app_with_file(dir, "mine");
        key(&mut app, KeyCode::Enter, KeyModifiers::NONE);
        press(&mut app, "!");
        view(&mut app).mode = CodeMode::Navigate;
        (app, path)
    }

    #[test]
    fn quit_and_back_are_blocked_while_dirty() {
        let dir = TempDir::new("quitguard");
        let (mut app, _) = dirty_in_navigate(&dir);
        for c in ['q', 'b'] {
            assert!(matches!(key(&mut app, KeyCode::Char(c), KeyModifiers::NONE), Action::None), "{c}");
            assert!(matches!(app.page, Page::Problem(_)), "{c} left the page");
            assert_eq!(view(&mut app).editor.text(), "!mine");
            assert!(view(&mut app).message.contains("Unsaved"));
        }
    }

    #[test]
    fn y_over_unsaved_edits_needs_a_second_press() {
        let dir = TempDir::new("sync");
        let (mut app, path) = dirty_in_navigate(&dir);
        std::fs::write(&path, "theirs").unwrap();

        press(&mut app, "y");
        assert_eq!(view(&mut app).editor.text(), "!mine");
        assert!(view(&mut app).message.contains("y again"));
        press(&mut app, "jy");
        assert_eq!(view(&mut app).editor.text(), "!mine");

        press(&mut app, "y");
        assert_eq!(view(&mut app).editor.text(), "theirs");
        assert!(!view(&mut app).editor.is_dirty());
    }

    #[test]
    fn paste_cancels_an_armed_reload() {
        let dir = TempDir::new("pastearm");
        let (mut app, path) = app_with_file(&dir, "mine");
        key(&mut app, KeyCode::Enter, KeyModifiers::NONE);
        press(&mut app, "!");
        std::fs::write(&path, "theirs").unwrap();

        key(&mut app, KeyCode::Char('r'), KeyModifiers::CONTROL);
        handle_paste(&mut app, "z");
        key(&mut app, KeyCode::Char('r'), KeyModifiers::CONTROL);
        assert_eq!(view(&mut app).editor.text(), "!zmine");
        assert!(view(&mut app).message.contains("Ctrl-R again"));
    }

    #[test]
    fn disarm_cancels_an_armed_reload() {
        let dir = TempDir::new("disarm");
        let (mut app, path) = app_with_file(&dir, "mine");
        key(&mut app, KeyCode::Enter, KeyModifiers::NONE);
        press(&mut app, "!");
        std::fs::write(&path, "theirs").unwrap();

        key(&mut app, KeyCode::Char('r'), KeyModifiers::CONTROL);
        view(&mut app).disarm();
        key(&mut app, KeyCode::Char('r'), KeyModifiers::CONTROL);
        assert_eq!(view(&mut app).editor.text(), "!mine");
    }

    #[test]
    fn a_failed_auto_reload_does_not_repeat_every_poll() {
        let dir = TempDir::new("badreload");
        let (mut app, path) = app_with_file(&dir, "mine");
        std::thread::sleep(std::time::Duration::from_millis(20));
        std::fs::write(&path, [0xff, 0xfe]).unwrap();

        view(&mut app).poll_disk();
        assert!(view(&mut app).message.contains("Reload failed"));
        view(&mut app).message = "Marked solved.".into();
        view(&mut app).poll_disk();
        assert_eq!(view(&mut app).message, "Marked solved.");
    }

    #[test]
    fn ctrl_q_twice_leaves_edit_mode_when_save_and_reload_both_fail() {
        let dir = TempDir::new("deadend");
        let (mut app, path) = app_with_file(&dir, "mine");
        key(&mut app, KeyCode::Enter, KeyModifiers::NONE);
        press(&mut app, "!");
        std::fs::remove_dir_all(&dir.0).unwrap();

        key(&mut app, KeyCode::Esc, KeyModifiers::NONE);
        assert!(view(&mut app).message.contains("Save failed"));
        key(&mut app, KeyCode::Char('s'), KeyModifiers::CONTROL);
        key(&mut app, KeyCode::Char('r'), KeyModifiers::CONTROL);
        key(&mut app, KeyCode::Char('r'), KeyModifiers::CONTROL);
        assert!(view(&mut app).message.contains("Reload failed"));
        assert_eq!(view(&mut app).mode, CodeMode::Edit);

        key(&mut app, KeyCode::Char('q'), KeyModifiers::CONTROL);
        assert!(view(&mut app).message.contains("press Ctrl-Q again"));
        key(&mut app, KeyCode::Right, KeyModifiers::NONE);
        key(&mut app, KeyCode::Char('q'), KeyModifiers::CONTROL);
        assert_eq!(view(&mut app).mode, CodeMode::Edit);
        assert_eq!(view(&mut app).editor.text(), "!mine");

        key(&mut app, KeyCode::Char('q'), KeyModifiers::CONTROL);
        let v = view(&mut app);
        assert_eq!(v.mode, CodeMode::Navigate);
        assert!(!v.editor.is_dirty());
        assert_eq!(v.editor.text(), "mine");
        assert!(!path.exists());
    }

    #[test]
    fn mixed_line_endings_are_written_only_after_an_edit() {
        let dir = TempDir::new("mixed");
        let mixed = "a\nb\r\nc\n";
        let (mut app, path) = app_with_file(&dir, mixed);
        assert!(view(&mut app).message.contains("Mixed line endings, normalized to LF on save"));

        key(&mut app, KeyCode::Enter, KeyModifiers::NONE);
        key(&mut app, KeyCode::Char('s'), KeyModifiers::CONTROL);
        key(&mut app, KeyCode::Esc, KeyModifiers::NONE);
        assert_eq!(view(&mut app).mode, CodeMode::Navigate);
        assert_eq!(std::fs::read_to_string(&path).unwrap(), mixed);

        key(&mut app, KeyCode::Enter, KeyModifiers::NONE);
        press(&mut app, "x");
        key(&mut app, KeyCode::Char('s'), KeyModifiers::CONTROL);
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "xa\nb\nc\n");
    }

    #[test]
    fn opening_a_problem_seeds_its_javascript_starter_under_the_test_config_dir() {
        let mut app = app();
        key(&mut app, KeyCode::Enter, KeyModifiers::NONE);
        let idx = view(&mut app).idx;
        let problem = &app.catalog.problems()[idx];
        let (slug, starter) = (problem.slug.clone(), problem.starter().to_string());
        let path = view(&mut app).path.clone();
        assert_eq!(path, crate::state::config_dir().join(format!("{slug}.js")));
        assert_eq!(std::fs::read_to_string(&path).unwrap(), starter);
        assert!(starter.contains("var twoSum = function"), "{starter}");
    }

    #[test]
    fn help_bar_follows_the_mode() {
        let dir = TempDir::new("help");
        let (mut app, _) = app_with_file(&dir, "");
        let nav: Vec<_> = { let v = match &app.page { Page::Problem(v) => v, _ => unreachable!() }; problem_keys(&app, v) };
        assert!(nav.contains(&("Enter", "edit")));
        key(&mut app, KeyCode::Enter, KeyModifiers::NONE);
        let Page::Problem(v) = &app.page else { unreachable!() };
        let edit = problem_keys(&app, v);
        assert_eq!(edit[0], ("Esc", "done"));
        assert!(edit.contains(&("^S", "save")));
        assert!(edit.contains(&("^Q^Q", "discard")));
        assert!(!edit.iter().any(|(k, _)| *k == "q"));
        assert!(!edit.iter().any(|(k, _)| *k == "F8"), "a clean buffer hides F8 while editing");
        assert!(!nav.iter().any(|(k, _)| *k == "F8"), "a clean buffer hides F8");
        key(&mut app, KeyCode::Char(':'), KeyModifiers::NONE);
        let Page::Problem(v) = &app.page else { unreachable!() };
        assert_eq!(problem_keys(&app, v)[2], ("F8", "next error"));
    }

    #[test]
    fn f8_in_navigate_mode_moves_to_the_error_and_shows_its_key() {
        let dir = TempDir::new("f8");
        let src = "function f(x) {\n  return (x;\n}\n";
        let (mut app, _) = app_with_file(&dir, src);
        view(&mut app).collapsed.code = true;
        let Page::Problem(v) = &app.page else { unreachable!() };
        assert!(problem_keys(&app, v).contains(&("F8", "next error")));
        key(&mut app, KeyCode::F(8), KeyModifiers::NONE);
        let v = view(&mut app);
        assert!(!v.collapsed.code);
        assert_eq!(v.mode, CodeMode::Navigate);
        assert_eq!(v.editor.caret(), (2, 12));
    }
}
