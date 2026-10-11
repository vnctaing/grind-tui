use super::editor::{code_file_path, is_trusted, EditorDef};
use crate::catalog::{Catalog, Difficulty, Lang, ListId, Problem};
use crate::state::AppState;
use crossterm::event::KeyCode;
use ratatui::widgets::ListState;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

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
pub struct Scroll { pub desc: u16, pub code: u16, pub output: u16 }

pub struct ProblemView {
    pub idx: usize,
    pub code: String,
    /// The code file is watched while the page is open, so edits made from
    /// another terminal tab sync too, not only those from a detached editor.
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
    fn open(idx: usize, problem: &Problem, lang: Lang) -> ProblemView {
        let (path, code) = load_code(problem, lang);
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
        self.code = std::fs::read_to_string(&self.path)?;
        self.mtime = modified(&self.path);
        self.scroll.code = 0;
        Ok(())
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
fn load_code(problem: &Problem, lang: Lang) -> (PathBuf, String) {
    let path = code_file_path(&problem.slug, lang);
    if let Ok(saved) = std::fs::read_to_string(&path) {
        if !saved.trim().is_empty() {
            return (path, saved);
        }
    }
    let starter = problem.starter(lang);
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
            let view = ProblemView::open(idx, &self.catalog.problems()[idx], self.progress.language);
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

    pub fn cycle_lang(&mut self) {
        let lang = self.progress.language.next();
        self.progress.language = lang;
        if let Page::Problem(view) = &mut self.page {
            let (path, code) = load_code(&self.catalog.problems()[view.idx], lang);
            view.mtime = modified(&path);
            view.path = path;
            view.code = code;
            view.scroll.code = 0;
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
        ("x", if app.is_solved(view.idx) { "unsolve" } else { "solved" }),
        ("l", app.progress.language.label()),
        ("p", "desc"),
        ("o", "output"),
        ("c", if view.collapsed.code { "show code" } else { "hide code" }),
        ("Tab", "p/o"),
        ("y", "sync"),
        ("Y", "yank"),
        ("q", "quit"),
    ]
}

// ── Key handling ──────────────────────────────────────────────────────────────

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
        KeyCode::Char('l') => { app.cycle_lang(); return Action::None; }
        KeyCode::Char('y') => return Action::SyncDetached,
        KeyCode::Char('Y') => return Action::CopyCommand,
        _ => {}
    }
    let Page::Problem(view) = &mut app.page else { return Action::None };
    match key {
        KeyCode::Tab => {
            view.focused = match view.focused {
                Panel::Output | Panel::Code => Panel::Description,
                Panel::Description => Panel::Output,
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
}
