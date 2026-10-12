use super::editor::{code_file_path, is_trusted, EditorDef};
use crate::catalog::{Catalog, Difficulty, ListId, Problem};
use crate::state::AppState;
use crossterm::event::KeyCode;
use ratatui::widgets::ListState;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

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
pub struct Scroll { pub desc: u16, pub code: u16, pub output: u16 }

pub struct ProblemView {
    pub idx: usize,
    pub code: String,
    pub path: PathBuf,
    pub mtime: Option<SystemTime>,
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
        ProblemView::new(idx, path, code)
    }

    pub fn new(idx: usize, path: PathBuf, code: String) -> ProblemView {
        ProblemView {
            idx,
            code,
            mtime: modified(&path),
            path,
            focused: Panel::Code,
            collapsed: Collapsed::default(),
            scroll: Scroll::default(),
            detached: None,
            message: String::new(),
        }
    }

    pub fn reload(&mut self) -> std::io::Result<()> {
        self.mtime = modified(&self.path);
        self.code = std::fs::read_to_string(&self.path)?;
        self.scroll.code = 0;
        Ok(())
    }

    pub fn poll_disk(&mut self) {
        if self.is_stale() && self.reload().is_ok() {
            self.message = format!("Auto-synced from {}.", self.detached.as_deref().unwrap_or("disk"));
        }
    }

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
            Panel::Code => &mut self.scroll.code,
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
    match std::fs::read(&path) {
        Ok(bytes) => {
            let saved = String::from_utf8_lossy(&bytes).into_owned();
            if !saved.trim().is_empty() {
                return (path, saved);
            }
        }
        Err(e) if e.kind() != std::io::ErrorKind::NotFound => return (path, String::new()),
        Err(_) => {}
    }
    let starter = problem.starter();
    if !starter.is_empty() {
        if let Some(dir) = path.parent() { std::fs::create_dir_all(dir).ok(); }
        std::fs::write(&path, starter).ok();
    }
    (path, starter.to_string())
}

pub enum Page { List, Problem(ProblemView) }

pub enum Action { Quit, LaunchEditor(EditorDef), SyncDetached, CopyCommand, None }

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
    vec![
        ("b", "back"),
        ("q", "quit"),
        ("x", if app.is_solved(view.idx) { "unsolve" } else { "solved" }),
        ("p", "desc"),
        ("o", "output"),
        ("c", if view.collapsed.code { "show code" } else { "hide code" }),
        ("Tab", "focus"),
        ("y", "sync"),
        ("Y", "yank"),
    ]
}

pub fn handle_key(app: &mut App, key: KeyCode) -> Action {
    match app.page {
        Page::List => handle_list_key(app, key),
        Page::Problem(_) => handle_problem_key(app, key),
    }
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
        KeyCode::Char('y') => return Action::SyncDetached,
        KeyCode::Char('Y') => return Action::CopyCommand,
        _ => {}
    }
    let Page::Problem(view) = &mut app.page else { return Action::None };
    match key {
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

    #[test]
    fn a_solution_that_is_not_utf8_is_shown_and_never_replaced_by_the_starter() {
        let catalog = Catalog::embedded();
        let problem = catalog.problems().iter().find(|p| p.slug == "two-sum").unwrap();
        let path = code_file_path(&problem.slug);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        let mine = b"// caf\xe9 my work\nvar twoSum = 1;\n";
        std::fs::write(&path, mine).unwrap();

        let (_, code) = load_code(problem);
        assert!(code.contains("my work"));
        assert_eq!(std::fs::read(&path).unwrap(), mine);
    }

    fn app() -> App {
        App::new(Catalog::embedded(), AppState::default(), vec![], ListFilter::All, None)
    }

    fn press(app: &mut App, keys: &str) {
        for c in keys.chars() {
            handle_key(app, KeyCode::Char(c));
        }
    }

    #[test]
    fn search_then_clear_restores_full_list_from_the_top() {
        let mut app = app();
        press(&mut app, "/tree");
        let matches = app.filtered.len();
        assert!(matches > 0 && matches < 167);
        for _ in 0..4 {
            handle_key(&mut app, KeyCode::Backspace);
        }
        handle_key(&mut app, KeyCode::Enter);
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
        handle_key(&mut app, KeyCode::Enter);
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
        app.page = Page::Problem(ProblemView::new(0, path.clone(), contents.to_string()));
        (app, path)
    }

    fn view(app: &mut App) -> &mut ProblemView {
        match &mut app.page {
            Page::Problem(view) => view,
            Page::List => panic!("not on a problem page"),
        }
    }

    fn touch_later(path: &Path, contents: impl AsRef<[u8]>) {
        std::fs::write(path, contents).unwrap();
        let later = SystemTime::now() + std::time::Duration::from_secs(60);
        std::fs::File::options().write(true).open(path).unwrap().set_modified(later).unwrap();
    }

    #[test]
    fn typing_on_the_problem_page_never_changes_the_code() {
        let dir = TempDir::new("readonly");
        let (mut app, path) = app_with_file(&dir, "const a = 1;");
        handle_key(&mut app, KeyCode::Enter);
        press(&mut app, "zia!");
        handle_key(&mut app, KeyCode::Backspace);
        assert_eq!(view(&mut app).code, "const a = 1;");
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "const a = 1;");
    }

    #[test]
    fn external_change_auto_syncs_the_code_panel() {
        let dir = TempDir::new("reload");
        let (mut app, path) = app_with_file(&dir, "old");
        view(&mut app).detached = Some("Cursor".into());
        view(&mut app).poll_disk();
        assert_eq!(view(&mut app).message, "");
        touch_later(&path, "new");
        view(&mut app).poll_disk();
        assert_eq!(view(&mut app).code, "new");
        assert_eq!(view(&mut app).message, "Auto-synced from Cursor.");
    }

    #[test]
    fn a_failed_auto_reload_does_not_repeat_every_poll() {
        let dir = TempDir::new("badreload");
        let (mut app, path) = app_with_file(&dir, "mine");
        touch_later(&path, [0xff, 0xfe]);

        view(&mut app).poll_disk();
        assert_eq!(view(&mut app).code, "mine");
        assert!(!view(&mut app).is_stale());
    }

    #[test]
    fn y_returns_sync_and_trusted_editors_launch() {
        let dir = TempDir::new("keys");
        let (mut app, _) = app_with_file(&dir, "");
        app.editors = vec![
            EditorDef { name: "nvim".into(), key: "e".into(), command: "nvim".into(), detach: false },
            EditorDef { name: "Zed".into(), key: "Z".into(), command: "zed".into(), detach: true },
        ];
        assert!(matches!(handle_key(&mut app, KeyCode::Char('y')), Action::SyncDetached));
        assert!(matches!(handle_key(&mut app, KeyCode::Char('e')), Action::LaunchEditor(_)));
        assert!(matches!(handle_key(&mut app, KeyCode::Char('Z')), Action::None));
        assert!(view(&mut app).message.contains("Blocked: 'zed'"));
    }

    #[test]
    fn opening_a_problem_seeds_its_javascript_starter_under_the_test_config_dir() {
        let mut app = app();
        handle_key(&mut app, KeyCode::Enter);
        let idx = view(&mut app).idx;
        let problem = &app.catalog.problems()[idx];
        let (slug, starter) = (problem.slug.clone(), problem.starter().to_string());
        let path = view(&mut app).path.clone();
        assert_eq!(path, crate::state::config_dir().join(format!("{slug}.js")));
        assert_eq!(std::fs::read_to_string(&path).unwrap(), starter);
        assert_eq!(view(&mut app).code, starter);
        assert!(starter.contains("var twoSum = function"), "{starter}");
    }

    #[test]
    fn help_bar_offers_no_edit_mode() {
        let dir = TempDir::new("help");
        let (app, _) = app_with_file(&dir, "");
        let Page::Problem(v) = &app.page else { unreachable!() };
        let keys = problem_keys(&app, v);
        assert!(!keys.iter().any(|(k, _)| *k == "Enter"), "{keys:?}");
        assert!(keys.contains(&("y", "sync")) && keys.contains(&("Y", "yank")));
    }
}
