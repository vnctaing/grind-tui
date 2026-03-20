use crossterm::{
    event::{self, Event, KeyCode, KeyEventKind},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{
    backend::CrosstermBackend,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, ListState, Paragraph, Wrap},
    Frame, Terminal,
};
use std::io;

// ── Shared types (also used by disguise module) ───────────────────────────────

#[derive(Debug, Clone, PartialEq)]
pub enum Difficulty {
    Easy,
    Medium,
    Hard,
    Unknown,
}

impl Difficulty {
    pub fn from_str(s: &str) -> Self {
        match s {
            "Easy" => Self::Easy,
            "Medium" => Self::Medium,
            "Hard" => Self::Hard,
            _ => Self::Unknown,
        }
    }

    pub fn color(&self) -> Color {
        match self {
            Self::Easy => Color::Green,
            Self::Medium => Color::Yellow,
            Self::Hard => Color::Red,
            Self::Unknown => Color::Gray,
        }
    }

    pub fn label(&self) -> &'static str {
        match self {
            Self::Easy => "Easy",
            Self::Medium => "Medium",
            Self::Hard => "Hard",
            Self::Unknown => "?",
        }
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
pub enum Panel {
    List,
    Description,
    Code,
    Output,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Page {
    List,
    Problem,
}

#[derive(Debug)]
pub enum ExitReason {
    Quit,
    SwitchToDisguise,
    SwitchToClassic,
}

enum Action {
    Exit(ExitReason),
    LoadProblem,
    RunTests,
    Submit,
    EnterEditMode,
    None,
}

pub struct App {
    pub problems: Vec<Problem>,
    pub filtered: Vec<usize>,
    pub selected: usize,
    pub active_panel: Panel,
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
    pub editing: bool,
}

impl App {
    pub fn new() -> Self {
        App {
            problems: vec![],
            filtered: vec![],
            selected: 0,
            active_panel: Panel::List,
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
            editing: false,
        }
    }

    pub fn rebuild_filtered(&mut self) {
        let q = self.search_query.to_lowercase();
        self.filtered = self
            .problems
            .iter()
            .enumerate()
            .filter(|(_, p)| {
                q.is_empty()
                    || p.title.to_lowercase().contains(&q)
                    || p.id.to_string().contains(&q)
            })
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
        if let Some(&i) = self.filtered.get(self.selected) {
            self.problems.get_mut(i)
        } else {
            None
        }
    }
}

// ── Syntax highlighting ───────────────────────────────────────────────────────

fn highlight_code(code: &str, lang: &str) -> Vec<Line<'static>> {
    let keywords: &[&str] = match lang {
        "python3" | "python" => &[
            "def", "class", "return", "if", "elif", "else", "for", "while", "in", "and", "or",
            "not", "import", "from", "pass", "break", "continue", "lambda", "with", "as", "try",
            "except", "finally", "raise", "True", "False", "None", "self",
        ],
        "rust" => &[
            "fn", "let", "mut", "pub", "use", "struct", "enum", "impl", "trait", "return", "if",
            "else", "for", "while", "match", "mod", "crate", "super", "self", "Self", "type",
            "where", "async", "await", "move", "ref", "in", "loop", "break", "continue", "const",
            "static",
        ],
        "javascript" | "typescript" => &[
            "function", "const", "let", "var", "return", "if", "else", "for", "while", "class",
            "import", "export", "from", "async", "await", "new", "this", "typeof", "instanceof",
            "null", "undefined", "true", "false",
        ],
        _ => &[],
    };

    code.lines()
        .map(|line| {
            let comment_start = match lang {
                "python3" | "python" => line.find('#'),
                "rust" | "javascript" | "typescript" | "cpp" | "java" | "go" => line.find("//"),
                _ => None,
            };

            if let Some(pos) = comment_start {
                let (code_part, comment_part) = line.split_at(pos);
                let mut spans = tokenize_spans(code_part, keywords);
                spans.push(Span::styled(
                    comment_part.to_string(),
                    Style::default()
                        .fg(Color::DarkGray)
                        .add_modifier(Modifier::ITALIC),
                ));
                Line::from(spans)
            } else {
                Line::from(tokenize_spans(line, keywords))
            }
        })
        .collect()
}

fn tokenize_spans(line: &str, keywords: &[&str]) -> Vec<Span<'static>> {
    let mut spans = Vec::new();
    let mut current = String::new();

    for ch in line.chars() {
        if ch.is_alphanumeric() || ch == '_' {
            current.push(ch);
        } else {
            if !current.is_empty() {
                let word = current.clone();
                if keywords.contains(&word.as_str()) {
                    spans.push(Span::styled(
                        word,
                        Style::default().fg(Color::Magenta).add_modifier(Modifier::BOLD),
                    ));
                } else if word.chars().next().map(|c| c.is_numeric()).unwrap_or(false) {
                    spans.push(Span::styled(word, Style::default().fg(Color::Cyan)));
                } else {
                    spans.push(Span::raw(word));
                }
                current.clear();
            }
            spans.push(Span::raw(ch.to_string()));
        }
    }

    if !current.is_empty() {
        let word = current.clone();
        if keywords.contains(&word.as_str()) {
            spans.push(Span::styled(
                word,
                Style::default().fg(Color::Magenta).add_modifier(Modifier::BOLD),
            ));
        } else if word.chars().next().map(|c| c.is_numeric()).unwrap_or(false) {
            spans.push(Span::styled(word, Style::default().fg(Color::Cyan)));
        } else {
            spans.push(Span::raw(word));
        }
    }

    spans
}

// ── Drawing ───────────────────────────────────────────────────────────────────

fn draw(f: &mut Frame, app: &App, list_state: &mut ListState, textarea: &tui_textarea::TextArea) {
    match app.page {
        Page::List => draw_list_page(f, app, list_state),
        Page::Problem => draw_problem_page(f, app, textarea),
    }
}

// ── Page 1: Problem List ──────────────────────────────────────────────────────

fn draw_list_page(f: &mut Frame, app: &App, list_state: &mut ListState) {
    let area = f.area();
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(0), Constraint::Length(1)])
        .split(area);

    draw_list_full(f, chunks[0], app, list_state);
    draw_list_status(f, chunks[1], app);
}

fn draw_list_full(f: &mut Frame, area: Rect, app: &App, list_state: &mut ListState) {
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(3), Constraint::Min(0)])
        .split(area);

    // Search bar
    let search_text = if app.searching {
        format!("/{}_", app.search_query)
    } else if !app.search_query.is_empty() {
        format!("/{}", app.search_query)
    } else {
        " /  (press / to search)".to_string()
    };
    let search = Paragraph::new(search_text)
        .style(if app.searching {
            Style::default().fg(Color::Yellow)
        } else {
            Style::default().fg(Color::DarkGray)
        })
        .block(Block::default().borders(Borders::ALL).title(" Search "));
    f.render_widget(search, rows[0]);

    // Problem table
    let items: Vec<ListItem> = app
        .filtered
        .iter()
        .map(|&i| {
            let p = &app.problems[i];
            let check = if p.solved { "✓" } else { " " };
            ListItem::new(Line::from(vec![
                Span::styled(
                    format!("{:>4}", p.id),
                    Style::default().fg(Color::DarkGray),
                ),
                Span::raw("  "),
                Span::styled(check, Style::default().fg(Color::Green)),
                Span::raw("  "),
                Span::raw(format!("{:<30}", truncate(&p.title, 30))),
                Span::raw("  "),
                Span::styled(
                    format!("{:<8}", p.difficulty.label()),
                    Style::default().fg(p.difficulty.color()),
                ),
            ]))
        })
        .collect();

    let list = List::new(items)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(format!(" #    ✓  Title                           Difficulty  │ {} problems ", app.filtered.len())),
        )
        .highlight_style(Style::default().bg(Color::DarkGray).add_modifier(Modifier::BOLD))
        .highlight_symbol("▶ ");

    f.render_stateful_widget(list, rows[1], list_state);
}

fn draw_list_status(f: &mut Frame, area: Rect, app: &App) {
    f.render_widget(
        Paragraph::new(format!(
            " [/]search  [Enter]open  [d]disguise  [q]quit  │  Problems: {}  Solved: {}",
            app.filtered.len(),
            app.solved_count,
        ))
        .style(Style::default().bg(Color::DarkGray).fg(Color::White)),
        area,
    );
}

// ── Page 2: Problem Detail ────────────────────────────────────────────────────

fn draw_problem_page(f: &mut Frame, app: &App, textarea: &tui_textarea::TextArea) {
    let area = f.area();
    let outer = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(1), Constraint::Min(0), Constraint::Length(1)])
        .split(area);

    draw_problem_title(f, outer[0], app);
    draw_problem_sections(f, outer[1], app, textarea);
    draw_problem_status(f, outer[2], app);
}

fn draw_problem_title(f: &mut Frame, area: Rect, app: &App) {
    let title = app
        .selected_problem()
        .map(|p| format!(" {} . {} [{}] ", p.id, p.title, p.difficulty.label()))
        .unwrap_or_else(|| " Problem ".to_string());

    let title_style = app
        .selected_problem()
        .map(|p| Style::default().fg(p.difficulty.color()).add_modifier(Modifier::BOLD))
        .unwrap_or_else(|| Style::default().add_modifier(Modifier::BOLD));

    f.render_widget(
        Paragraph::new(title).style(title_style),
        area,
    );
}

fn draw_problem_sections(f: &mut Frame, area: Rect, app: &App, textarea: &tui_textarea::TextArea) {
    // Two columns: description (left) | code+output (right)
    let cols = if app.desc_collapsed {
        Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Length(0), Constraint::Min(0)])
            .split(area)
    } else {
        Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(40), Constraint::Percentage(60)])
            .split(area)
    };

    if !app.desc_collapsed {
        draw_desc_section(f, cols[0], app);
    }

    // Right column: code (top 75%) + output (bottom 25%)
    let output_constraint = if app.output_collapsed {
        Constraint::Length(1)
    } else {
        Constraint::Percentage(25)
    };
    let right_rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(0), output_constraint])
        .split(cols[1]);

    draw_code_section(f, right_rows[0], app, textarea);
    draw_output_section(f, right_rows[1], app);
}

fn draw_desc_section(f: &mut Frame, area: Rect, app: &App) {
    if app.desc_collapsed {
        f.render_widget(
            Paragraph::new(Line::from(vec![
                Span::styled(" [P] Description ", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
                Span::styled(" (collapsed — press p to expand)", Style::default().fg(Color::DarkGray)),
            ]))
            .style(Style::default().bg(Color::DarkGray)),
            area,
        );
    } else {
        let content = app
            .selected_problem()
            .map(|p| p.description.as_str())
            .unwrap_or("Select a problem and press Enter to load it.");

        f.render_widget(
            Paragraph::new(content)
                .block(
                    Block::default()
                        .borders(Borders::ALL)
                        .title(" [P] Description "),
                )
                .wrap(Wrap { trim: false })
                .scroll((app.desc_scroll, 0)),
            area,
        );
    }
}

fn draw_code_section(f: &mut Frame, area: Rect, app: &App, textarea: &tui_textarea::TextArea) {
    let lang_label = if app.editing {
        format!(" \u{270f} Code \u{2014} {} \u{2014} (Esc to save) ", app.language)
    } else {
        format!(" [E] Code \u{2014} {} ", app.language)
    };

    if app.editing {
        let block = Block::default()
            .borders(Borders::ALL)
            .title(lang_label)
            .border_style(Style::default().fg(Color::Yellow));
        let inner = block.inner(area);
        f.render_widget(block, area);
        f.render_widget(&*textarea, inner);
    } else {
        let highlighted = highlight_code(
            app.selected_problem().map(|p| p.code.as_str()).unwrap_or("# Select a problem and press Enter"),
            &app.language,
        );
        f.render_widget(
            Paragraph::new(highlighted)
                .block(Block::default().borders(Borders::ALL).title(lang_label))
                .wrap(Wrap { trim: false })
                .scroll((app.code_scroll, 0)),
            area,
        );
    }
}

fn draw_output_section(f: &mut Frame, area: Rect, app: &App) {
    if app.output_collapsed {
        f.render_widget(
            Paragraph::new(Line::from(vec![
                Span::styled(" [O] Output ", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
                Span::styled(" (collapsed — press o to expand)", Style::default().fg(Color::DarkGray)),
            ]))
            .style(Style::default().bg(Color::DarkGray)),
            area,
        );
    } else {
        let output_style = if app.output.contains("Accepted") || app.output.contains("passed") {
            Style::default().fg(Color::Green)
        } else if app.output.contains("Error") || app.output.contains("Wrong") {
            Style::default().fg(Color::Red)
        } else {
            Style::default()
        };

        f.render_widget(
            Paragraph::new(app.output.as_str())
                .block(
                    Block::default()
                        .borders(Borders::ALL)
                        .title(" [O] Output "),
                )
                .style(output_style)
                .wrap(Wrap { trim: false })
                .scroll((app.output_scroll, 0)),
            area,
        );
    }
}

fn draw_problem_status(f: &mut Frame, area: Rect, app: &App) {
    let status = if app.editing {
        " [Esc]save+exit edit mode".to_string()
    } else {
        format!(
            " [b]back  [e]edit  [r]run  [s]submit  [l]lang:{}  [p]desc  [o]output  [d]disguise  [q]quit",
            app.language,
        )
    };
    f.render_widget(
        Paragraph::new(status)
            .style(Style::default().bg(Color::DarkGray).fg(Color::White)),
        area,
    );
}

// ── Event handling ────────────────────────────────────────────────────────────

fn handle_key(app: &mut App, key: KeyCode) -> Action {
    match app.page {
        Page::List => handle_key_list(app, key),
        Page::Problem => handle_key_problem(app, key),
    }
}

fn handle_key_list(app: &mut App, key: KeyCode) -> Action {
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
        KeyCode::Char('d') => Action::Exit(ExitReason::SwitchToDisguise),

        KeyCode::Char('/') => {
            app.searching = true;
            app.search_query.clear();
            app.rebuild_filtered();
            Action::None
        }

        KeyCode::Enter => Action::LoadProblem,

        KeyCode::Char('j') | KeyCode::Down => {
            if !app.filtered.is_empty() {
                app.selected = (app.selected + 1).min(app.filtered.len() - 1);
            }
            Action::None
        }

        KeyCode::Char('k') | KeyCode::Up => {
            if app.selected > 0 {
                app.selected -= 1;
            }
            Action::None
        }

        _ => Action::None,
    }
}

fn handle_key_problem(app: &mut App, key: KeyCode) -> Action {
    match key {
        KeyCode::Char('q') => Action::Exit(ExitReason::Quit),
        KeyCode::Char('d') => Action::Exit(ExitReason::SwitchToDisguise),
        KeyCode::Char('e') => Action::EnterEditMode,

        KeyCode::Char('b') => {
            app.page = Page::List;
            Action::None
        }

        KeyCode::Char('p') => {
            app.desc_collapsed = !app.desc_collapsed;
            Action::None
        }

        KeyCode::Char('o') => {
            app.output_collapsed = !app.output_collapsed;
            Action::None
        }

        KeyCode::Char('r') => Action::RunTests,
        KeyCode::Char('s') => Action::Submit,

        KeyCode::Char('l') => {
            const LANGS: &[&str] = &["python3", "rust", "javascript", "typescript", "java", "cpp", "go", "c"];
            let cur = LANGS.iter().position(|&l| l == app.language.as_str()).unwrap_or(0);
            let new_lang = LANGS[(cur + 1) % LANGS.len()].to_string();
            app.language = new_lang.clone();
            if let Some(p) = app.selected_problem_mut() {
                if let Some(starter) = p.starter_code.get(&new_lang).cloned() {
                    p.code = starter;
                }
            }
            Action::None
        }

        KeyCode::Char('j') | KeyCode::Down => {
            app.desc_scroll = app.desc_scroll.saturating_add(1);
            Action::None
        }

        KeyCode::Char('k') | KeyCode::Up => {
            app.desc_scroll = app.desc_scroll.saturating_sub(1);
            Action::None
        }

        _ => Action::None,
    }
}

// ── Public entry point ────────────────────────────────────────────────────────

pub async fn run(app: &mut App) -> Result<ExitReason, Box<dyn std::error::Error>> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let mut list_state = ListState::default();
    list_state.select(Some(app.selected));

    let reason = event_loop(&mut terminal, app, &mut list_state)?;

    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;
    Ok(reason)
}

fn event_loop(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    app: &mut App,
    list_state: &mut ListState,
) -> Result<ExitReason, Box<dyn std::error::Error>> {
    let mut textarea = tui_textarea::TextArea::default();

    loop {
        list_state.select(Some(app.selected));
        terminal.draw(|f| draw(f, app, list_state, &textarea))?;

        if let Event::Key(key) = event::read()? {
            if key.kind != KeyEventKind::Press {
                continue;
            }

            // When in edit mode, intercept ALL key events
            if app.editing {
                if key.code == KeyCode::Esc {
                    let new_code = textarea.lines().join("\n");
                    if let Some(p) = app.selected_problem_mut() {
                        p.code = new_code;
                    }
                    app.editing = false;
                } else {
                    textarea.input(tui_textarea::Input::from(key));
                }
                continue;
            }

            match handle_key(app, key.code) {
                Action::None => {}

                Action::EnterEditMode => {
                    let lines: Vec<String> = app
                        .selected_problem()
                        .map(|p| p.code.lines().map(str::to_string).collect())
                        .unwrap_or_default();
                    textarea = tui_textarea::TextArea::new(lines);
                    textarea.set_line_number_style(
                        Style::default().fg(Color::DarkGray),
                    );
                    app.editing = true;
                }

                Action::Exit(reason) => return Ok(reason),

                Action::LoadProblem => {
                    if let Some(slug) = app.selected_problem().map(|p| p.slug.clone()) {
                        if let Some(client) = app.client.clone() {
                            app.output = "Loading\u{2026}".to_string();
                            terminal.draw(|f| draw(f, app, list_state, &textarea))?;
                            match tokio::task::block_in_place(|| {
                                tokio::runtime::Handle::current().block_on(client.get_problem(&slug))
                            }) {
                                Ok(detail) => {
                                    let lang = app.language.clone();
                                    if let Some(p) = app.selected_problem_mut() {
                                        p.id = detail.id;
                                        p.description = detail.description;
                                        p.starter_code = detail.starter_code;
                                        p.test_cases = detail.examples.join("\n");
                                        p.code = p.starter_code.get(&lang)
                                            .or_else(|| p.starter_code.get("python3"))
                                            .cloned()
                                            .unwrap_or_default();
                                    }
                                    app.desc_scroll = 0;
                                    app.output = "Problem loaded.".to_string();
                                    app.page = Page::Problem;
                                }
                                Err(e) => {
                                    app.output = format!("Error: {e}");
                                }
                            }
                        } else {
                            app.output = "No API session \u{2014} start with --session and --csrf".to_string();
                        }
                    }
                }

                Action::RunTests => {
                    if let (Some(p), Some(client)) = (
                        app.selected_problem().map(|p| (p.slug.clone(), p.code.clone(), p.id.to_string(), p.test_cases.clone())),
                        app.client.clone(),
                    ) {
                        let (slug, code, qid, test_cases) = p;
                        if code.trim().is_empty() {
                            app.output = "No code \u{2014} press Enter on the problem to load it first.".to_string();
                            continue;
                        }
                        let lang = app.language.clone();
                        app.output = "Running tests\u{2026}".to_string();
                        terminal.draw(|f| draw(f, app, list_state, &textarea))?;
                        match tokio::task::block_in_place(|| {
                            tokio::runtime::Handle::current().block_on(
                                client.run_tests(&slug, &code, &lang, &test_cases, &qid)
                            )
                        }) {
                            Ok(r) => {
                                let mut out = format!(
                                    "Status: {}\nPassed: {}/{}\nRuntime: {}ms  Memory: {}MB",
                                    r.status,
                                    r.passed, r.total,
                                    r.runtime_ms.map_or("?".into(), |v| v.to_string()),
                                    r.memory_mb.map_or("?".into(), |v| format!("{:.1}", v)),
                                );
                                if let Some(detail) = r.error_detail {
                                    out.push('\n');
                                    out.push_str(&detail);
                                }
                                app.output = out;
                            }
                            Err(e) => { app.output = format!("Error: {e}"); }
                        }
                    } else {
                        app.output = "No API session or no problem selected.".to_string();
                    }
                }

                Action::Submit => {
                    if let (Some(p), Some(client)) = (
                        app.selected_problem().map(|p| (p.slug.clone(), p.code.clone(), p.id.to_string())),
                        app.client.clone(),
                    ) {
                        let (slug, code, qid) = p;
                        if code.trim().is_empty() {
                            app.output = "No code \u{2014} press Enter on the problem to load it first.".to_string();
                            continue;
                        }
                        let lang = app.language.clone();
                        app.output = "Submitting\u{2026}".to_string();
                        terminal.draw(|f| draw(f, app, list_state, &textarea))?;
                        match tokio::task::block_in_place(|| {
                            tokio::runtime::Handle::current().block_on(
                                client.submit(&slug, &code, &lang, &qid)
                            )
                        }) {
                            Ok(r) => {
                                let accepted = r.status == "Accepted";
                                if accepted {
                                    if let Some(&i) = app.filtered.get(app.selected) {
                                        app.problems[i].solved = true;
                                        app.solved_count = app.problems.iter().filter(|p| p.solved).count();
                                    }
                                }
                                app.output = format!(
                                    "Status: {}\nPassed: {}/{}\nRuntime: {}ms  Memory: {}MB",
                                    r.status,
                                    r.passed, r.total,
                                    r.runtime_ms.map_or("?".into(), |v| v.to_string()),
                                    r.memory_mb.map_or("?".into(), |v| format!("{:.1}", v)),
                                );
                            }
                            Err(e) => { app.output = format!("Error: {e}"); }
                        }
                    } else {
                        app.output = "No API session or no problem selected.".to_string();
                    }
                }
            }
        }
    }
}

fn truncate(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        s.to_string()
    } else {
        let cut = s.char_indices().nth(max - 1).map(|(i, _)| i).unwrap_or(max);
        format!("{}\u{2026}", &s[..cut])
    }
}
