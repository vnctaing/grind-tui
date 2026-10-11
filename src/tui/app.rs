use super::editor::{code_file_path, load_editors, EditorDef};
use crossterm::event::KeyCode;
use ratatui::style::Color;

// ── Shared types ──────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq)]
pub enum Difficulty { Easy, Medium, Hard, Unknown }

impl Difficulty {
    pub fn from_str(s: &str) -> Self {
        match s { "Easy" => Self::Easy, "Medium" => Self::Medium, "Hard" => Self::Hard, _ => Self::Unknown }
    }
    pub fn color(&self) -> Color {
        match self { Self::Easy => Color::Green, Self::Medium => Color::Yellow, Self::Hard => Color::Red, Self::Unknown => Color::Gray }
    }
    pub fn label(&self) -> &'static str {
        match self { Self::Easy => "Easy", Self::Medium => "Medium", Self::Hard => "Hard", Self::Unknown => "?" }
    }
}

#[derive(Debug, Clone)]
pub struct Problem {
    pub id: u32,
    pub title: String,
    pub slug: String,
    pub difficulty: Difficulty,
    pub solved: bool,
    pub description: String,
    pub code: String,
    pub starter_code: std::collections::HashMap<String, String>,
    pub test_cases: String,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Panel { List, Description, Code, Output }

#[derive(Debug, Clone, PartialEq)]
pub enum Page { List, Problem }

#[derive(Debug)]
pub enum ExitReason { Quit }

pub(super) enum Action {
    Exit(ExitReason),
    LoadProblem,
    RunTests,
    Submit,
    SyncDetached,
    CopyCommand,
    None,
}

pub struct App {
    pub problems: Vec<Problem>,
    pub filtered: Vec<usize>,
    pub selected: usize,
    pub active_panel: Panel,
    pub focused_panel: Panel,
    pub output: String,
    pub solved_count: usize,
    pub language: String,
    pub searching: bool,
    pub search_query: String,
    pub desc_scroll: u16,
    pub code_scroll: u16,
    pub output_scroll: u16,
    pub client: Option<std::sync::Arc<crate::api::LeetCodeClient>>,
    pub page: Page,
    pub desc_collapsed: bool,
    pub output_collapsed: bool,
    pub code_collapsed: bool,
    pub editors: Vec<EditorDef>,
    /// File currently open in a detached editor (auto-sync watches this)
    pub detached_file: Option<String>,
    pub detached_file_mtime: Option<std::time::SystemTime>,
    /// Name of the detached editor currently open (for display)
    pub detached_editor_name: Option<String>,
}

impl App {
    pub fn new() -> Self {
        App {
            problems: vec![],
            filtered: vec![],
            selected: 0,
            active_panel: Panel::List,
            focused_panel: Panel::Code,
            output: String::new(),
            solved_count: 0,
            language: "python3".to_string(),
            searching: false,
            search_query: String::new(),
            desc_scroll: 0,
            code_scroll: 0,
            output_scroll: 0,
            client: None,
            page: Page::List,
            desc_collapsed: false,
            output_collapsed: false,
            code_collapsed: false,
            editors: load_editors(),
            detached_file: None,
            detached_file_mtime: None,
            detached_editor_name: None,
        }
    }

    pub fn rebuild_filtered(&mut self) {
        let q = self.search_query.to_lowercase();
        self.filtered = self.problems.iter().enumerate()
            .filter(|(_, p)| q.is_empty() || p.title.to_lowercase().contains(&q) || p.id.to_string().contains(&q))
            .map(|(i, _)| i)
            .collect();
        if self.selected >= self.filtered.len() {
            self.selected = self.filtered.len().saturating_sub(1);
        }
    }

    pub fn selected_problem(&self) -> Option<&Problem> {
        self.filtered.get(self.selected).and_then(|&i| self.problems.get(i))
    }

    pub fn selected_problem_mut(&mut self) -> Option<&mut Problem> {
        if let Some(&i) = self.filtered.get(self.selected) { self.problems.get_mut(i) } else { None }
    }
}

// ── Key handling ──────────────────────────────────────────────────────────────

pub(super) fn handle_key_list(app: &mut App, key: KeyCode) -> Action {
    if app.searching {
        match key {
            KeyCode::Esc => { app.searching = false; }
            KeyCode::Enter => { app.searching = false; app.rebuild_filtered(); }
            KeyCode::Backspace => { app.search_query.pop(); app.rebuild_filtered(); }
            KeyCode::Char(c) => { app.search_query.push(c); app.rebuild_filtered(); }
            _ => {}
        }
        return Action::None;
    }
    match key {
        KeyCode::Char('q') => Action::Exit(ExitReason::Quit),
        KeyCode::Char('/') => { app.searching = true; app.search_query.clear(); app.rebuild_filtered(); Action::None }
        KeyCode::Enter => Action::LoadProblem,
        KeyCode::Char('j') | KeyCode::Down => {
            if !app.filtered.is_empty() { app.selected = (app.selected + 1).min(app.filtered.len() - 1); }
            Action::None
        }
        KeyCode::Char('k') | KeyCode::Up => {
            if app.selected > 0 { app.selected -= 1; }
            Action::None
        }
        _ => Action::None,
    }
}

pub(super) fn handle_key_problem(app: &mut App, key: KeyCode) -> Action {
    match key {
        KeyCode::Char('q') => Action::Exit(ExitReason::Quit),
        KeyCode::Tab => {
            app.focused_panel = match app.focused_panel {
                Panel::Output | Panel::Code => Panel::Description,
                Panel::Description | Panel::List => Panel::Output,
            };
            Action::None
        }
        KeyCode::Char('b') => {
            app.page = Page::List;
            app.detached_file = None;
            app.detached_file_mtime = None;
            app.detached_editor_name = None;
            Action::None
        }
        KeyCode::Char('c') => { app.code_collapsed = !app.code_collapsed; Action::None }
        KeyCode::Char('p') => {
            if app.desc_collapsed { app.desc_collapsed = false; app.focused_panel = Panel::Description; }
            else if app.focused_panel == Panel::Description { app.desc_collapsed = true; app.focused_panel = Panel::Code; }
            else { app.focused_panel = Panel::Description; }
            Action::None
        }
        KeyCode::Char('o') => {
            if app.output_collapsed { app.output_collapsed = false; app.focused_panel = Panel::Output; }
            else if app.focused_panel == Panel::Output { app.output_collapsed = true; app.focused_panel = Panel::Code; }
            else { app.focused_panel = Panel::Output; }
            Action::None
        }
        KeyCode::Char('y') => Action::SyncDetached,
        KeyCode::Char('Y') => Action::CopyCommand,
        KeyCode::Char('r') => Action::RunTests,
        KeyCode::Char('s') => Action::Submit,
        KeyCode::Char('l') => {
            const LANGS: &[&str] = &["python3","rust","javascript","typescript","java","cpp","go","c"];
            let cur = LANGS.iter().position(|&l| l == app.language.as_str()).unwrap_or(0);
            let new_lang = LANGS[(cur + 1) % LANGS.len()].to_string();
            app.language = new_lang.clone();
            if let Some(p) = app.selected_problem_mut() {
                let path = code_file_path(&p.slug, &new_lang);
                if let Ok(saved) = std::fs::read_to_string(&path) {
                    if !saved.trim().is_empty() {
                        p.code = saved;
                    } else if let Some(starter) = p.starter_code.get(&new_lang).cloned() {
                        p.code = starter;
                        std::fs::write(&path, &p.code).ok();
                    }
                } else if let Some(starter) = p.starter_code.get(&new_lang).cloned() {
                    p.code = starter;
                    std::fs::write(&path, &p.code).ok();
                }
                let mtime = std::fs::metadata(&path).ok().and_then(|m| m.modified().ok());
                app.detached_file = Some(path);
                app.detached_file_mtime = mtime;
            }
            app.code_scroll = 0;
            Action::None
        }
        KeyCode::Char('j') | KeyCode::Down => {
            match app.focused_panel {
                Panel::Description => app.desc_scroll = app.desc_scroll.saturating_add(1),
                Panel::Code => app.code_scroll = app.code_scroll.saturating_add(1),
                Panel::Output => app.output_scroll = app.output_scroll.saturating_add(1),
                Panel::List => {}
            }
            Action::None
        }
        KeyCode::Char('k') | KeyCode::Up => {
            match app.focused_panel {
                Panel::Description => app.desc_scroll = app.desc_scroll.saturating_sub(1),
                Panel::Code => app.code_scroll = app.code_scroll.saturating_sub(1),
                Panel::Output => app.output_scroll = app.output_scroll.saturating_sub(1),
                Panel::List => {}
            }
            Action::None
        }
        _ => Action::None,
    }
}
