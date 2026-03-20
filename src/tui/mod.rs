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

// ── Shared types ──────────────────────────────────────────────────────────────

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
    OpenCursor,
    SyncCursor,
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
    pub editing: bool,
    pub cursor_file: Option<String>,
    pub cursor_file_mtime: Option<std::time::SystemTime>,
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
            editing: false,
            cursor_file: None,
            cursor_file_mtime: None,
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

// ── Cursor file helpers ───────────────────────────────────────────────────────

fn ext_for_lang(lang: &str) -> &'static str {
    match lang {
        "python3" | "python" => "py",
        "rust" => "rs",
        "javascript" => "js",
        "typescript" => "ts",
        "java" => "java",
        "cpp" => "cpp",
        "go" => "go",
        "c" => "c",
        _ => "txt",
    }
}

fn temp_file_path(slug: &str, lang: &str) -> String {
    let dir = dirs::home_dir()
        .unwrap_or_else(|| std::path::PathBuf::from("/tmp"))
        .join(".leetcode-tui");
    std::fs::create_dir_all(&dir).ok();
    dir.join(format!("{}.{}", slug, ext_for_lang(lang)))
        .to_string_lossy()
        .to_string()
}

// ── Syntax highlighting ───────────────────────────────────────────────────────

fn keywords_for_lang(lang: &str) -> &'static [&'static str] {
    match lang {
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
    }
}

fn highlight_code(code: &str, lang: &str) -> Vec<Line<'static>> {
    let keywords = keywords_for_lang(lang);
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
                    Style::default().fg(Color::DarkGray).add_modifier(Modifier::ITALIC),
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
                spans.push(make_word_span(word, keywords));
                current.clear();
            }
            spans.push(Span::raw(ch.to_string()));
        }
    }
    if !current.is_empty() {
        spans.push(make_word_span(current, keywords));
    }
    spans
}

fn make_word_span(word: String, keywords: &[&str]) -> Span<'static> {
    if keywords.contains(&word.as_str()) {
        Span::styled(word, Style::default().fg(Color::Magenta).add_modifier(Modifier::BOLD))
    } else if word.chars().next().map(|c| c.is_numeric()).unwrap_or(false) {
        Span::styled(word, Style::default().fg(Color::Cyan))
    } else {
        Span::raw(word)
    }
}

fn inject_cursor(spans: Vec<Span<'static>>, col: usize) -> Vec<Span<'static>> {
    let cursor_style = Style::default().bg(Color::White).fg(Color::Black);
    let mut result = Vec::new();
    let mut total = 0usize;
    let mut placed = false;

    for span in spans {
        let chars: Vec<char> = span.content.chars().collect();
        let len = chars.len();
        if !placed && col >= total && col < total + len {
            let local = col - total;
            if local > 0 {
                result.push(Span::styled(chars[..local].iter().collect::<String>(), span.style));
            }
            result.push(Span::styled(chars[local].to_string(), cursor_style));
            if local + 1 < len {
                result.push(Span::styled(chars[local + 1..].iter().collect::<String>(), span.style));
            }
            placed = true;
        } else {
            result.push(span);
        }
        total += len;
    }
    if !placed {
        result.push(Span::styled(" ", cursor_style));
    }
    result
}

fn tokenize_line_for_editor(line: &str, keywords: &[&str], lang: &str, cursor_col: Option<usize>) -> Line<'static> {
    let comment_start = match lang {
        "python3" | "python" => line.find('#'),
        "rust" | "javascript" | "typescript" | "cpp" | "java" | "go" => line.find("//"),
        _ => None,
    };
    let spans = if let Some(pos) = comment_start {
        let (code_part, comment_part) = line.split_at(pos);
        let mut s = tokenize_spans(code_part, keywords);
        s.push(Span::styled(comment_part.to_string(), Style::default().fg(Color::DarkGray).add_modifier(Modifier::ITALIC)));
        s
    } else {
        tokenize_spans(line, keywords)
    };
    let spans = if let Some(col) = cursor_col { inject_cursor(spans, col) } else { spans };
    Line::from(spans)
}

// ── Drawing ───────────────────────────────────────────────────────────────────

fn draw(f: &mut Frame, app: &App, list_state: &mut ListState, textarea: &tui_textarea::TextArea) {
    match app.page {
        Page::List => draw_list_page(f, app, list_state),
        Page::Problem => draw_problem_page(f, app, textarea),
    }
}

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

    let search_text = if app.searching {
        format!("/{}_", app.search_query)
    } else if !app.search_query.is_empty() {
        format!("/{}", app.search_query)
    } else {
        " /  (press / to search)".to_string()
    };
    f.render_widget(
        Paragraph::new(search_text)
            .style(if app.searching { Style::default().fg(Color::Yellow) } else { Style::default().fg(Color::DarkGray) })
            .block(Block::default().borders(Borders::ALL).title(" Search ")),
        rows[0],
    );

    let items: Vec<ListItem> = app.filtered.iter().map(|&i| {
        let p = &app.problems[i];
        let check = if p.solved { "✓" } else { " " };
        ListItem::new(Line::from(vec![
            Span::styled(format!("{:>4}", p.id), Style::default().fg(Color::DarkGray)),
            Span::raw("  "),
            Span::styled(check, Style::default().fg(Color::Green)),
            Span::raw("  "),
            Span::raw(format!("{:<30}", truncate(&p.title, 30))),
            Span::raw("  "),
            Span::styled(format!("{:<8}", p.difficulty.label()), Style::default().fg(p.difficulty.color())),
        ]))
    }).collect();

    f.render_stateful_widget(
        List::new(items)
            .block(Block::default().borders(Borders::ALL)
                .title(format!(" #    ✓  Title                           Difficulty  │ {} problems ", app.filtered.len())))
            .highlight_style(Style::default().bg(Color::DarkGray).add_modifier(Modifier::BOLD))
            .highlight_symbol("▶ "),
        rows[1],
        list_state,
    );
}

fn draw_list_status(f: &mut Frame, area: Rect, app: &App) {
    f.render_widget(
        Paragraph::new(format!(
            " [/]search  [Enter]open  [d]disguise  [q]quit  │  Problems: {}  Solved: {}",
            app.filtered.len(), app.solved_count,
        )).style(Style::default().bg(Color::DarkGray).fg(Color::White)),
        area,
    );
}

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
    let title = app.selected_problem()
        .map(|p| format!(" {} . {} [{}] ", p.id, p.title, p.difficulty.label()))
        .unwrap_or_else(|| " Problem ".to_string());
    let style = app.selected_problem()
        .map(|p| Style::default().fg(p.difficulty.color()).add_modifier(Modifier::BOLD))
        .unwrap_or_else(|| Style::default().add_modifier(Modifier::BOLD));
    f.render_widget(Paragraph::new(title).style(style), area);
}

fn draw_problem_sections(f: &mut Frame, area: Rect, app: &App, textarea: &tui_textarea::TextArea) {
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
        draw_desc_section(f, cols[0], app, app.focused_panel == Panel::Description);
    }

    let output_constraint = if app.output_collapsed { Constraint::Length(1) } else { Constraint::Percentage(25) };
    let right_rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(0), output_constraint])
        .split(cols[1]);

    draw_code_section(f, right_rows[0], app, textarea, app.focused_panel == Panel::Code);
    draw_output_section(f, right_rows[1], app, app.focused_panel == Panel::Output);
}

fn draw_desc_section(f: &mut Frame, area: Rect, app: &App, focused: bool) {
    let content = app.selected_problem()
        .map(|p| p.description.as_str())
        .unwrap_or("Select a problem and press Enter to load it.");
    let border_style = if focused { Style::default().fg(Color::Cyan) } else { Style::default() };
    f.render_widget(
        Paragraph::new(content)
            .block(Block::default().borders(Borders::ALL).title(" [P] Description ").border_style(border_style))
            .wrap(Wrap { trim: false })
            .scroll((app.desc_scroll, 0)),
        area,
    );
}

fn draw_code_section(
    f: &mut Frame,
    area: Rect,
    app: &App,
    textarea: &tui_textarea::TextArea,
    focused: bool,
) {
    let cursor_hint = if app.cursor_file.is_some() { " auto-sync ON " } else { "" };
    let lang_label = if app.editing {
        format!(" ✎ editing — {} {}(Esc to save) ", app.language, cursor_hint)
    } else {
        format!(" [e]edit  [C]cursor {}— {} ", cursor_hint, app.language)
    };

    let border_style = if app.editing {
        Style::default().fg(Color::Yellow)
    } else if focused {
        Style::default().fg(Color::Cyan)
    } else {
        Style::default()
    };

    let block = Block::default().borders(Borders::ALL).title(lang_label).border_style(border_style);

    if app.editing {
        let inner = block.inner(area);
        f.render_widget(block, area);

        let lines = textarea.lines();
        let (cursor_row, cursor_col) = textarea.cursor();
        let keywords = keywords_for_lang(&app.language);
        let lang = app.language.as_str();

        let visible = inner.height as usize;
        let scroll_row = if cursor_row >= app.code_scroll as usize + visible {
            (cursor_row + 1).saturating_sub(visible) as u16
        } else if cursor_row < app.code_scroll as usize {
            cursor_row as u16
        } else {
            app.code_scroll
        };

        let highlighted: Vec<Line<'static>> = lines.iter().enumerate().map(|(row, line)| {
            let col = if row == cursor_row { Some(cursor_col) } else { None };
            let mut l = tokenize_line_for_editor(line, keywords, lang, col);
            l.spans.insert(0, Span::styled(format!("{:>3} ", row + 1), Style::default().fg(Color::DarkGray)));
            l
        }).collect();

        f.render_widget(
            Paragraph::new(highlighted).wrap(Wrap { trim: false }).scroll((scroll_row, 0)),
            inner,
        );
    } else {
        let highlighted: Vec<Line<'static>> = highlight_code(
            app.selected_problem().map(|p| p.code.as_str()).unwrap_or("# Select a problem and press Enter"),
            &app.language,
        )
        .into_iter()
        .enumerate()
        .map(|(i, mut l)| {
            l.spans.insert(0, Span::styled(format!("{:>3} ", i + 1), Style::default().fg(Color::DarkGray)));
            l
        })
        .collect();

        f.render_widget(
            Paragraph::new(highlighted)
                .block(block)
                .wrap(Wrap { trim: false })
                .scroll((app.code_scroll, 0)),
            area,
        );
    }
}

fn draw_output_section(f: &mut Frame, area: Rect, app: &App, focused: bool) {
    if app.output_collapsed {
        f.render_widget(
            Paragraph::new(Line::from(vec![
                Span::styled(" [O] Output ", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
                Span::styled(" (collapsed — press o to expand)", Style::default().fg(Color::DarkGray)),
            ])).style(Style::default().bg(Color::DarkGray)),
            area,
        );
        return;
    }

    let output_style = if app.output.contains("Accepted") || app.output.contains("passed") {
        Style::default().fg(Color::Green)
    } else if app.output.contains("Error") || app.output.contains("Wrong") {
        Style::default().fg(Color::Red)
    } else {
        Style::default()
    };
    let border_style = if focused { Style::default().fg(Color::Cyan) } else { Style::default() };

    f.render_widget(
        Paragraph::new(app.output.as_str())
            .block(Block::default().borders(Borders::ALL).title(" [O] Output ").border_style(border_style))
            .style(output_style)
            .wrap(Wrap { trim: false })
            .scroll((app.output_scroll, 0)),
        area,
    );
}

fn draw_problem_status(f: &mut Frame, area: Rect, app: &App) {
    let status = if app.editing {
        " [Esc]save  [r]run  [s]submit".to_string()
    } else {
        format!(
            " [b]back  [e]edit  [C]cursor  [r]run  [s]submit  [l]lang:{}  [p]desc  [o]output  [Tab]focus  [d]disguise  [q]quit",
            app.language,
        )
    };
    f.render_widget(
        Paragraph::new(status).style(Style::default().bg(Color::DarkGray).fg(Color::White)),
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

fn handle_key_problem(app: &mut App, key: KeyCode) -> Action {
    match key {
        KeyCode::Char('q') => Action::Exit(ExitReason::Quit),
        KeyCode::Char('d') => Action::Exit(ExitReason::SwitchToDisguise),
        KeyCode::Char('e') => Action::EnterEditMode,
        KeyCode::Char('C') => Action::OpenCursor,
        KeyCode::Char('y') => Action::SyncCursor,

        KeyCode::Tab => {
            app.focused_panel = match app.focused_panel {
                Panel::Code => Panel::Output,
                Panel::Output => Panel::Description,
                Panel::Description => Panel::Code,
                Panel::List => Panel::Code,
            };
            Action::None
        }
        KeyCode::Char('b') => { app.page = Page::List; Action::None }
        KeyCode::Char('p') => { app.desc_collapsed = !app.desc_collapsed; Action::None }
        KeyCode::Char('o') => { app.output_collapsed = !app.output_collapsed; Action::None }
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
            match app.focused_panel {
                Panel::Description => app.desc_scroll = app.desc_scroll.saturating_add(1),
                Panel::Code => app.code_scroll = app.code_scroll.saturating_add(1),
                Panel::Output => app.output_scroll = app.output_scroll.saturating_add(1),
                Panel::List => app.desc_scroll = app.desc_scroll.saturating_add(1),
            }
            Action::None
        }
        KeyCode::Char('k') | KeyCode::Up => {
            match app.focused_panel {
                Panel::Description => app.desc_scroll = app.desc_scroll.saturating_sub(1),
                Panel::Code => app.code_scroll = app.code_scroll.saturating_sub(1),
                Panel::Output => app.output_scroll = app.output_scroll.saturating_sub(1),
                Panel::List => app.desc_scroll = app.desc_scroll.saturating_sub(1),
            }
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
    execute!(stdout, crossterm::event::EnableMouseCapture)?;
    execute!(stdout, crossterm::event::EnableBracketedPaste)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let mut list_state = ListState::default();
    list_state.select(Some(app.selected));

    let reason = event_loop(&mut terminal, app, &mut list_state)?;

    disable_raw_mode()?;
    execute!(terminal.backend_mut(), crossterm::event::DisableBracketedPaste)?;
    execute!(terminal.backend_mut(), crossterm::event::DisableMouseCapture)?;
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

        // Poll with short timeout when Cursor auto-sync is active
        let timeout = if app.cursor_file.is_some() {
            std::time::Duration::from_millis(500)
        } else {
            std::time::Duration::from_secs(3600)
        };

        if !event::poll(timeout)? {
            // Timeout: check if cursor file changed
            if let Some(ref path) = app.cursor_file.clone() {
                if let Ok(meta) = std::fs::metadata(path) {
                    if let Ok(mtime) = meta.modified() {
                        let changed = app.cursor_file_mtime.map(|prev| mtime > prev).unwrap_or(true);
                        if changed {
                            app.cursor_file_mtime = Some(mtime);
                            if let Ok(code) = std::fs::read_to_string(path) {
                                // Sync into problem and, if editing, into textarea too
                                if app.editing {
                                    let lines: Vec<String> = code.lines().map(str::to_string).collect();
                                    textarea = tui_textarea::TextArea::new(lines);
                                }
                                if let Some(p) = app.selected_problem_mut() {
                                    p.code = code;
                                }
                                app.code_scroll = 0;
                                app.output = "Auto-synced from Cursor.".to_string();
                            }
                        }
                    }
                }
            }
            continue;
        }

        match event::read()? {
            Event::Paste(text) => {
                if app.editing {
                    textarea.insert_str(&text);
                }
            }
            Event::Mouse(mouse) => {
                match mouse.kind {
                    crossterm::event::MouseEventKind::ScrollDown => match app.focused_panel {
                        Panel::Description => app.desc_scroll = app.desc_scroll.saturating_add(3),
                        Panel::Code => app.code_scroll = app.code_scroll.saturating_add(3),
                        Panel::Output => app.output_scroll = app.output_scroll.saturating_add(3),
                        Panel::List => {}
                    },
                    crossterm::event::MouseEventKind::ScrollUp => match app.focused_panel {
                        Panel::Description => app.desc_scroll = app.desc_scroll.saturating_sub(3),
                        Panel::Code => app.code_scroll = app.code_scroll.saturating_sub(3),
                        Panel::Output => app.output_scroll = app.output_scroll.saturating_sub(3),
                        Panel::List => {}
                    },
                    _ => {}
                }
            }
            Event::Key(key) => {
                if key.kind != KeyEventKind::Press {
                    continue;
                }

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
                        let lines: Vec<String> = app.selected_problem()
                            .map(|p| p.code.lines().map(str::to_string).collect())
                            .unwrap_or_default();
                        textarea = tui_textarea::TextArea::new(lines);
                        app.editing = true;
                    }

                    Action::OpenCursor => {
                        if let Some(p) = app.selected_problem() {
                            let path = temp_file_path(&p.slug, &app.language);
                            std::fs::write(&path, &p.code)?;
                            let mtime = std::fs::metadata(&path).ok().and_then(|m| m.modified().ok());
                            std::process::Command::new("cursor").arg(&path).spawn().ok();
                            app.cursor_file = Some(path.clone());
                            app.cursor_file_mtime = mtime;
                            app.output = format!("Opened in Cursor: {}\nAuto-syncing on save…", path);
                        }
                    }

                    Action::SyncCursor => {
                        if let Some(ref path) = app.cursor_file.clone() {
                            match std::fs::read_to_string(path) {
                                Ok(code) => {
                                    if app.editing {
                                        let lines: Vec<String> = code.lines().map(str::to_string).collect();
                                        textarea = tui_textarea::TextArea::new(lines);
                                    }
                                    if let Some(p) = app.selected_problem_mut() {
                                        p.code = code;
                                    }
                                    app.code_scroll = 0;
                                    app.output = "Synced from Cursor.".to_string();
                                }
                                Err(e) => { app.output = format!("Sync error: {e}"); }
                            }
                        } else {
                            app.output = "No Cursor session. Press [C] to open in Cursor first.".to_string();
                        }
                    }

                    Action::Exit(reason) => return Ok(reason),

                    Action::LoadProblem => {
                        if let Some(slug) = app.selected_problem().map(|p| p.slug.clone()) {
                            if let Some(client) = app.client.clone() {
                                app.output = "Loading…".to_string();
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
                                    Err(e) => { app.output = format!("Error: {e}"); }
                                }
                            } else {
                                app.output = "No API session — start with --session and --csrf".to_string();
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
                                app.output = "No code — press Enter on the problem to load it first.".to_string();
                                continue;
                            }
                            let lang = app.language.clone();
                            app.output = "Running tests…".to_string();
                            terminal.draw(|f| draw(f, app, list_state, &textarea))?;
                            match tokio::task::block_in_place(|| {
                                tokio::runtime::Handle::current().block_on(
                                    client.run_tests(&slug, &code, &lang, &test_cases, &qid)
                                )
                            }) {
                                Ok(r) => {
                                    let mut out = format!(
                                        "Status: {}\nPassed: {}/{}\nRuntime: {}ms  Memory: {}MB",
                                        r.status, r.passed, r.total,
                                        r.runtime_ms.map_or("?".into(), |v| v.to_string()),
                                        r.memory_mb.map_or("?".into(), |v| format!("{:.1}", v)),
                                    );
                                    if let Some(detail) = r.error_detail { out.push('\n'); out.push_str(&detail); }
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
                                app.output = "No code — press Enter on the problem to load it first.".to_string();
                                continue;
                            }
                            let lang = app.language.clone();
                            app.output = "Submitting…".to_string();
                            terminal.draw(|f| draw(f, app, list_state, &textarea))?;
                            match tokio::task::block_in_place(|| {
                                tokio::runtime::Handle::current().block_on(
                                    client.submit(&slug, &code, &lang, &qid)
                                )
                            }) {
                                Ok(r) => {
                                    if r.status == "Accepted" {
                                        if let Some(&i) = app.filtered.get(app.selected) {
                                            app.problems[i].solved = true;
                                            app.solved_count = app.problems.iter().filter(|p| p.solved).count();
                                        }
                                    }
                                    app.output = format!(
                                        "Status: {}\nPassed: {}/{}\nRuntime: {}ms  Memory: {}MB",
                                        r.status, r.passed, r.total,
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
            _ => {}
        }
    }
}

fn truncate(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        s.to_string()
    } else {
        let cut = s.char_indices().nth(max - 1).map(|(i, _)| i).unwrap_or(max);
        format!("{}…", &s[..cut])
    }
}
