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
use serde::{Deserialize, Serialize};
use std::io;

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

enum Action {
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

// ── File helpers ──────────────────────────────────────────────────────────────

fn ext_for_lang(lang: &str) -> &'static str {
    match lang {
        "python3" | "python" => "py", "rust" => "rs", "javascript" => "js",
        "typescript" => "ts", "java" => "java", "cpp" => "cpp", "go" => "go", "c" => "c", _ => "txt",
    }
}

fn code_file_path(slug: &str, lang: &str) -> String {
    let dir = dirs::home_dir().unwrap_or_else(|| std::path::PathBuf::from("/tmp")).join(".grind-tui");
    std::fs::create_dir_all(&dir).ok();
    dir.join(format!("{}.{}", slug, ext_for_lang(lang))).to_string_lossy().to_string()
}

// ── Syntax highlighting ───────────────────────────────────────────────────────

fn keywords_for_lang(lang: &str) -> &'static [&'static str] {
    match lang {
        "python3" | "python" => &["def","class","return","if","elif","else","for","while","in","and","or","not","import","from","pass","break","continue","lambda","with","as","try","except","finally","raise","True","False","None","self"],
        "rust" => &["fn","let","mut","pub","use","struct","enum","impl","trait","return","if","else","for","while","match","mod","crate","super","self","Self","type","where","async","await","move","ref","in","loop","break","continue","const","static"],
        "javascript" | "typescript" => &["function","const","let","var","return","if","else","for","while","class","import","export","from","async","await","new","this","typeof","instanceof","null","undefined","true","false"],
        _ => &[],
    }
}

fn highlight_code(code: &str, lang: &str) -> Vec<Line<'static>> {
    let keywords = keywords_for_lang(lang);
    code.lines().map(|line| {
        let comment_start = match lang {
            "python3" | "python" => line.find('#'),
            "rust" | "javascript" | "typescript" | "cpp" | "java" | "go" => line.find("//"),
            _ => None,
        };
        if let Some(pos) = comment_start {
            let (code_part, comment_part) = line.split_at(pos);
            let mut spans = tokenize_spans(code_part, keywords);
            spans.push(Span::styled(comment_part.to_string(), Style::default().fg(Color::DarkGray).add_modifier(Modifier::ITALIC)));
            Line::from(spans)
        } else {
            Line::from(tokenize_spans(line, keywords))
        }
    }).collect()
}

fn tokenize_spans(line: &str, keywords: &[&str]) -> Vec<Span<'static>> {
    let mut spans = Vec::new();
    let mut current = String::new();
    for ch in line.chars() {
        if ch.is_alphanumeric() || ch == '_' {
            current.push(ch);
        } else {
            if !current.is_empty() {
                spans.push(word_span(current.clone(), keywords));
                current.clear();
            }
            spans.push(Span::raw(ch.to_string()));
        }
    }
    if !current.is_empty() { spans.push(word_span(current, keywords)); }
    spans
}

fn word_span(word: String, keywords: &[&str]) -> Span<'static> {
    if keywords.contains(&word.as_str()) {
        Span::styled(word, Style::default().fg(Color::Magenta).add_modifier(Modifier::BOLD))
    } else if word.chars().next().map(|c| c.is_numeric()).unwrap_or(false) {
        Span::styled(word, Style::default().fg(Color::Cyan))
    } else {
        Span::raw(word)
    }
}

// ── Drawing ───────────────────────────────────────────────────────────────────

fn draw(f: &mut Frame, app: &App, list_state: &mut ListState) {
    match app.page {
        Page::List => draw_list_page(f, app, list_state),
        Page::Problem => draw_problem_page(f, app),
    }
}

fn draw_list_page(f: &mut Frame, app: &App, list_state: &mut ListState) {
    let area = f.area();
    let chunks = Layout::default().direction(Direction::Vertical)
        .constraints([Constraint::Min(0), Constraint::Length(1)]).split(area);
    draw_list_full(f, chunks[0], app, list_state);
    draw_list_status(f, chunks[1], app);
}

fn draw_list_full(f: &mut Frame, area: Rect, app: &App, list_state: &mut ListState) {
    let rows = Layout::default().direction(Direction::Vertical)
        .constraints([Constraint::Length(3), Constraint::Min(0)]).split(area);

    let search_text = if app.searching { format!("/{}_", app.search_query) }
        else if !app.search_query.is_empty() { format!("/{}", app.search_query) }
        else { " /  (press / to search)".to_string() };
    f.render_widget(
        Paragraph::new(search_text)
            .style(if app.searching { Style::default().fg(Color::Yellow) } else { Style::default().fg(Color::DarkGray) })
            .block(Block::default().borders(Borders::ALL).title(" Search ")),
        rows[0],
    );

    let items: Vec<ListItem> = app.filtered.iter().map(|&i| {
        let p = &app.problems[i];
        ListItem::new(Line::from(vec![
            Span::styled(format!("{:>4}", p.id), Style::default().fg(Color::DarkGray)),
            Span::raw("  "),
            Span::styled(if p.solved { "✓" } else { " " }, Style::default().fg(Color::Green)),
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
        rows[1], list_state,
    );
}

fn draw_list_status(f: &mut Frame, area: Rect, app: &App) {
    f.render_widget(
        Paragraph::new(format!(
            " [/]search  [Enter]open  [q]quit  │  Problems: {}  Solved: {}",
            app.filtered.len(), app.solved_count,
        )).style(Style::default().bg(Color::DarkGray).fg(Color::White)),
        area,
    );
}

fn draw_problem_page(f: &mut Frame, app: &App) {
    let area = f.area();
    let outer = Layout::default().direction(Direction::Vertical)
        .constraints([Constraint::Length(1), Constraint::Min(0), Constraint::Length(1)]).split(area);
    draw_problem_title(f, outer[0], app);
    draw_problem_sections(f, outer[1], app);
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

fn draw_problem_sections(f: &mut Frame, area: Rect, app: &App) {
    let desc_width = if app.desc_collapsed { Constraint::Length(0) } else { Constraint::Percentage(40) };
    let cols = Layout::default().direction(Direction::Horizontal)
        .constraints([desc_width, Constraint::Min(0)]).split(area);

    if !app.desc_collapsed {
        draw_desc_section(f, cols[0], app, app.focused_panel == Panel::Description);
    }

    // Editor bar is always visible at the top of the right column
    let editor_bar_height = 3u16;
    let output_height = if app.output_collapsed { Constraint::Length(1) } else { Constraint::Percentage(25) };

    if app.code_collapsed {
        let right_rows = Layout::default().direction(Direction::Vertical)
            .constraints([Constraint::Length(editor_bar_height), Constraint::Length(1), Constraint::Min(0)])
            .split(cols[1]);
        draw_editor_bar(f, right_rows[0], app);
        f.render_widget(
            Paragraph::new(Span::styled(
                " [c] ▸ Code preview (hidden) ",
                Style::default().fg(Color::DarkGray),
            )),
            right_rows[1],
        );
        draw_output_section(f, right_rows[2], app, app.focused_panel == Panel::Output);
    } else {
        let right_rows = Layout::default().direction(Direction::Vertical)
            .constraints([Constraint::Length(editor_bar_height), Constraint::Min(0), output_height])
            .split(cols[1]);
        draw_editor_bar(f, right_rows[0], app);
        draw_code_section(f, right_rows[1], app, app.focused_panel == Panel::Code);
        draw_output_section(f, right_rows[2], app, app.focused_panel == Panel::Output);
    }
}

fn draw_desc_section(f: &mut Frame, area: Rect, app: &App, focused: bool) {
    let content = app.selected_problem().map(|p| p.description.as_str())
        .unwrap_or("Select a problem and press Enter to load it.");
    let title = if focused { " Description [focused — p to collapse] " } else { " [p] Description " };
    let border_style = if focused { Style::default().fg(Color::Cyan) } else { Style::default() };
    f.render_widget(
        Paragraph::new(content)
            .block(Block::default().borders(Borders::ALL).title(title).border_style(border_style))
            .wrap(Wrap { trim: false })
            .scroll((app.desc_scroll, 0)),
        area,
    );
}

fn draw_editor_bar(f: &mut Frame, area: Rect, app: &App) {
    let file_path = app.selected_problem()
        .map(|p| code_file_path(&p.slug, &app.language))
        .unwrap_or_default();

    let line1: Line = if let Some(ref name) = app.detached_editor_name {
        Line::from(vec![
            Span::styled(" ⟳ ", Style::default().fg(Color::Green).add_modifier(Modifier::BOLD)),
            Span::styled(format!("{} ", name), Style::default().fg(Color::White).add_modifier(Modifier::BOLD)),
            Span::styled("auto-syncing on save  ", Style::default().fg(Color::DarkGray)),
            Span::styled("[y]", Style::default().fg(Color::Yellow)),
            Span::styled("force sync", Style::default().fg(Color::DarkGray)),
        ])
    } else {
        let mut spans = vec![Span::styled(" ✏  Start coding: ", Style::default().fg(Color::DarkGray))];
        for ed in &app.editors {
            spans.push(Span::styled(format!("[{}]", ed.key), Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)));
            spans.push(Span::styled(format!("{} ", ed.name), Style::default().fg(Color::White)));
        }
        Line::from(spans)
    };

    let line2: Line = if file_path.is_empty() {
        Line::from("")
    } else {
        let cmd = app.detached_editor_name.as_deref()
            .or_else(|| app.editors.first().map(|e| e.command.as_str()))
            .unwrap_or("vim");
        Line::from(vec![
            Span::styled(" 💡 New tab: ", Style::default().fg(Color::DarkGray)),
            Span::styled(format!("{} {}", cmd, file_path), Style::default().fg(Color::DarkGray).add_modifier(Modifier::ITALIC)),
            Span::styled("  [Y]", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
            Span::styled("yank", Style::default().fg(Color::DarkGray)),
        ])
    };

    f.render_widget(
        Paragraph::new(vec![line1, line2])
            .block(Block::default().borders(Borders::BOTTOM).border_style(Style::default().fg(Color::DarkGray))),
        area,
    );
}

fn draw_code_section(f: &mut Frame, area: Rect, app: &App, _focused: bool) {
    // Code panel is always read-only — never interactive
    let (title, border_style) = if let Some(ref name) = app.detached_editor_name {
        (
            format!(" ⟳ Live from {} — read-only  [c]hide ", name),
            Style::default().fg(Color::DarkGray),
        )
    } else {
        (
            format!(" Code preview — read-only  [c]hide "),
            Style::default().fg(Color::DarkGray),
        )
    };
    let block = Block::default().borders(Borders::ALL).title(title).border_style(border_style);
    let inner = block.inner(area);
    f.render_widget(block, area);

    let highlighted: Vec<Line<'static>> = highlight_code(
        app.selected_problem().map(|p| p.code.as_str()).unwrap_or(""),
        &app.language,
    ).into_iter().enumerate().map(|(i, mut l)| {
        l.spans.insert(0, Span::styled(format!("{:>3} ", i + 1), Style::default().fg(Color::DarkGray)));
        l
    }).collect();

    f.render_widget(
        Paragraph::new(highlighted)
            .wrap(Wrap { trim: false })
            .scroll((app.code_scroll, 0)),
        inner,
    );
}

fn diff_line(got: &str, expected: &str, label: &str, base_color: Color) -> Line<'static> {
    let parse = |s: &str| -> Option<Vec<String>> {
        let t = s.trim();
        if t.starts_with('[') && t.ends_with(']') {
            Some(t[1..t.len()-1].split(',').map(|x| x.trim().to_string()).collect())
        } else {
            None
        }
    };

    let dim = Style::default().fg(Color::DarkGray);
    let highlight = Style::default().fg(base_color).add_modifier(Modifier::BOLD);
    let normal = Style::default().fg(base_color);

    let other = if label == "Got:     " { expected } else { got };

    if let (Some(items), Some(other_items)) = (parse(s_for(label, got, expected)), parse(other)) {
        let mut spans = vec![Span::styled(format!("{} [", label), normal)];
        for (i, item) in items.iter().enumerate() {
            let matches = other_items.get(i).map(|o| o == item).unwrap_or(false);
            spans.push(Span::styled(item.clone(), if matches { dim } else { highlight }));
            if i + 1 < items.len() {
                spans.push(Span::styled(",", normal));
            }
        }
        // extra items not in other
        for i in items.len()..other_items.len() {
            let _ = i; // missing items — already shown as dim absence
        }
        spans.push(Span::styled("]", normal));
        Line::from(spans)
    } else {
        Line::from(Span::styled(format!("{} {}", label, s_for(label, got, expected)), normal))
    }
}

fn s_for<'a>(label: &str, got: &'a str, expected: &'a str) -> &'a str {
    if label.starts_with("Got") { got } else { expected }
}

fn draw_output_section(f: &mut Frame, area: Rect, app: &App, focused: bool) {
    if app.output_collapsed {
        f.render_widget(
            Paragraph::new(Span::styled(" [o] Output ▸ (expand) ", Style::default().fg(Color::DarkGray))),
            area,
        );
        return;
    }
    let border_style = if focused { Style::default().fg(Color::Cyan) } else { Style::default() };
    let title = if focused { " Output [focused — o to collapse] " } else { " [o] Output " };

    let raw: Vec<&str> = app.output.lines().collect();
    let mut lines: Vec<Line> = Vec::new();
    let mut i = 0;
    while i < raw.len() {
        let line = raw[i];
        // Detect Got/Expected pair and do element-level diff
        if line.starts_with("Got:") && i + 1 < raw.len() && raw[i+1].starts_with("Expected:") {
            let got_val = line.trim_start_matches("Got:").trim();
            let exp_val = raw[i+1].trim_start_matches("Expected:").trim();
            lines.push(diff_line(got_val, exp_val, "Got:     ", Color::Red));
            lines.push(diff_line(got_val, exp_val, "Expected:", Color::Green));
            i += 2;
            continue;
        }
        let style = if line.starts_with("✓") || line.starts_with("Status: Accepted") {
            Style::default().fg(Color::Green)
        } else if line.starts_with("✗") || line.starts_with("Status: Wrong") || line.starts_with("Status: Runtime") || line.starts_with("Status: Time") {
            Style::default().fg(Color::Red)
        } else if line.starts_with("Input:") {
            Style::default().fg(Color::Yellow)
        } else if line.starts_with("Status:") {
            Style::default().fg(Color::White).add_modifier(Modifier::BOLD)
        } else {
            Style::default()
        };
        lines.push(Line::from(Span::styled(line.to_string(), style)));
        i += 1;
    }

    f.render_widget(
        Paragraph::new(lines)
            .block(Block::default().borders(Borders::ALL).title(title).border_style(border_style))
            .wrap(Wrap { trim: false }).scroll((app.output_scroll, 0)),
        area,
    );
}

fn draw_problem_status(f: &mut Frame, area: Rect, app: &App) {
    let editor_keys: String = app.editors.iter()
        .map(|ed| format!("[{}]{} ", ed.key, ed.name))
        .collect();
    let code_label = if app.code_collapsed { "[c]show code" } else { "[c]hide code" };
    let status = format!(
        " [b]back  {}[r]run  [s]submit  [l]lang:{}  [p]desc  [o]output  {}  [Tab]p↔o  [q]quit",
        editor_keys, app.language, code_label,
    );
    f.render_widget(
        Paragraph::new(status).style(Style::default().bg(Color::DarkGray).fg(Color::White)),
        area,
    );
}

// ── Key handling ──────────────────────────────────────────────────────────────

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

// ── TUI suspend / resume ──────────────────────────────────────────────────────

fn suspend_tui(terminal: &mut Terminal<CrosstermBackend<io::Stdout>>) -> io::Result<()> {
    disable_raw_mode()?;
    execute!(terminal.backend_mut(), crossterm::event::DisableMouseCapture)?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()
}

fn resume_tui(terminal: &mut Terminal<CrosstermBackend<io::Stdout>>) -> io::Result<()> {
    execute!(terminal.backend_mut(), EnterAlternateScreen)?;
    execute!(terminal.backend_mut(), crossterm::event::EnableMouseCapture)?;
    enable_raw_mode()?;
    terminal.clear()
}

// ── Public entry point ────────────────────────────────────────────────────────

pub async fn run(app: &mut App) -> Result<ExitReason, Box<dyn std::error::Error>> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    execute!(stdout, crossterm::event::EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;
    let mut list_state = ListState::default();
    list_state.select(Some(app.selected));
    let reason = event_loop(&mut terminal, app, &mut list_state)?;
    disable_raw_mode()?;
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
    loop {
        list_state.select(Some(app.selected));
        terminal.draw(|f| draw(f, app, list_state))?;

        let timeout = if app.detached_file.is_some() {
            std::time::Duration::from_millis(500)
        } else {
            std::time::Duration::from_secs(3600)
        };

        if !event::poll(timeout)? {
            // Auto-sync detached editor
            if let Some(ref path) = app.detached_file.clone() {
                if let Ok(meta) = std::fs::metadata(path) {
                    if let Ok(mtime) = meta.modified() {
                        let changed = app.detached_file_mtime.map(|prev| mtime > prev).unwrap_or(true);
                        if changed {
                            app.detached_file_mtime = Some(mtime);
                            if let Ok(code) = std::fs::read_to_string(path) {
                                if let Some(p) = app.selected_problem_mut() { p.code = code; }
                                app.code_scroll = 0;
                                let name = app.detached_editor_name.clone().unwrap_or_else(|| "editor".into());
                                app.output = format!("Auto-synced from {}.", name);
                            }
                        }
                    }
                }
            }
            continue;
        }

        match event::read()? {
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
                if key.kind != KeyEventKind::Press { continue; }

                // Editor key handling (needs terminal access for suspend/resume)
                if app.page == Page::Problem {
                    if let KeyCode::Char(c) = key.code {
                        if let Some(ed) = app.editors.iter().find(|e| e.key == c.to_string()).cloned() {
                            const ALLOWED_EDITORS: &[&str] = &[
                                "vim", "nvim", "nano", "hx", "helix",
                                "code", "cursor", "emacs", "micro", "vi",
                            ];
                            let binary = std::path::Path::new(&ed.command)
                                .file_name()
                                .and_then(|n| n.to_str())
                                .unwrap_or("");
                            if !ALLOWED_EDITORS.contains(&binary) {
                                app.output = format!("Blocked: '{}' is not a trusted editor. Edit ~/.grind-tui/editors.json to use a known editor.", ed.command);
                                continue;
                            }
                            if let Some(p) = app.selected_problem() {
                                let path = code_file_path(&p.slug, &app.language);
                                std::fs::write(&path, &p.code)?;

                                if ed.detach {
                                    let mtime = std::fs::metadata(&path).ok().and_then(|m| m.modified().ok());
                                    std::process::Command::new(&ed.command).arg(&path).spawn().ok();
                                    app.detached_file = Some(path.clone());
                                    app.detached_file_mtime = mtime;
                                    app.detached_editor_name = Some(ed.name.clone());
                                    app.output = format!("Opened in {} — auto-syncing on save.\nFile: {}", ed.name, path);
                                } else {
                                    suspend_tui(terminal)?;
                                    std::process::Command::new(&ed.command).arg(&path).status().ok();
                                    resume_tui(terminal)?;
                                    if let Ok(code) = std::fs::read_to_string(&path) {
                                        if let Some(p) = app.selected_problem_mut() { p.code = code; }
                                        app.code_scroll = 0;
                                    }
                                }
                            }
                            continue;
                        }
                    }
                }

                let action = match app.page {
                    Page::List => handle_key_list(app, key.code),
                    Page::Problem => handle_key_problem(app, key.code),
                };

                match action {
                    Action::None => {}
                    Action::Exit(reason) => return Ok(reason),

                    Action::CopyCommand => {
                        if let Some(p) = app.selected_problem() {
                            let path = code_file_path(&p.slug, &app.language);
                            let cmd = format!("nvim {}", path);
                            let copied = std::process::Command::new("pbcopy")
                                .stdin(std::process::Stdio::piped())
                                .spawn()
                                .and_then(|mut child| {
                                    use std::io::Write;
                                    child.stdin.as_mut().unwrap().write_all(cmd.as_bytes())?;
                                    child.wait()
                                });
                            app.output = match copied {
                                Ok(_) => format!("Copied: {}", cmd),
                                Err(_) => format!("Copy failed. Command: {}", cmd),
                            };
                        }
                    }

                    Action::SyncDetached => {
                        if let Some(ref path) = app.detached_file.clone() {
                            match std::fs::read_to_string(path) {
                                Ok(code) => {
                                    if let Some(p) = app.selected_problem_mut() { p.code = code; }
                                    app.code_scroll = 0;
                                    app.output = "Synced.".to_string();
                                }
                                Err(e) => { app.output = format!("Sync error: {e}"); }
                            }
                        } else {
                            app.output = "No detached editor open.".to_string();
                        }
                    }

                    Action::LoadProblem => {
                        if let Some(slug) = app.selected_problem().map(|p| p.slug.clone()) {
                            if let Some(client) = app.client.clone() {
                                app.output = "Loading…".to_string();
                                terminal.draw(|f| draw(f, app, list_state))?;
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
                                                .cloned().unwrap_or_default();
                                        }
                                        app.desc_scroll = 0;
                                        app.page = Page::Problem;
                                        if let Some(p) = app.selected_problem_mut() {
                                            let path = code_file_path(&p.slug, &lang);
                                            if let Ok(saved) = std::fs::read_to_string(&path) {
                                                if !saved.trim().is_empty() {
                                                    p.code = saved;
                                                }
                                            } else {
                                                std::fs::write(&path, &p.code).ok();
                                            }
                                            let mtime = std::fs::metadata(&path).ok().and_then(|m| m.modified().ok());
                                            app.detached_file = Some(path);
                                            app.detached_file_mtime = mtime;
                                        }
                                        app.output = "Problem loaded.".to_string();
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
                                app.output = "No code — load the problem first.".to_string();
                                continue;
                            }
                            let lang = app.language.clone();
                            app.output = "Running tests…".to_string();
                            terminal.draw(|f| draw(f, app, list_state))?;
                            match tokio::task::block_in_place(|| {
                                tokio::runtime::Handle::current().block_on(client.run_tests(&slug, &code, &lang, &test_cases, &qid))
                            }) {
                                Ok(r) => {
                                    let mut out = format!(
                                        "Status: {}\nPassed: {}/{}\nRuntime: {}ms  Memory: {}MB",
                                        r.status, r.passed, r.total,
                                        r.runtime_ms.map_or("?".into(), |v| v.to_string()),
                                        r.memory_mb.map_or("?".into(), |v| format!("{:.1}", v)),
                                    );
                                    if let Some(d) = r.error_detail { out.push('\n'); out.push_str(&d); }
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
                                app.output = "No code — load the problem first.".to_string();
                                continue;
                            }
                            let lang = app.language.clone();
                            app.output = "Submitting…".to_string();
                            terminal.draw(|f| draw(f, app, list_state))?;
                            match tokio::task::block_in_place(|| {
                                tokio::runtime::Handle::current().block_on(client.submit(&slug, &code, &lang, &qid))
                            }) {
                                Ok(r) => {
                                    if r.status == "Accepted" {
                                        if let Some(&i) = app.filtered.get(app.selected) {
                                            app.problems[i].solved = true;
                                            app.solved_count = app.problems.iter().filter(|p| p.solved).count();
                                        }
                                    }
                                    let mut out = format!(
                                        "Status: {}\nPassed: {}/{}\nRuntime: {}ms  Memory: {}MB",
                                        r.status, r.passed, r.total,
                                        r.runtime_ms.map_or("?".into(), |v| v.to_string()),
                                        r.memory_mb.map_or("?".into(), |v| format!("{:.1}", v)),
                                    );
                                    if let Some(d) = r.error_detail { out.push('\n'); out.push_str(&d); }
                                    app.output = out;
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
    if s.chars().count() <= max { s.to_string() }
    else {
        let cut = s.char_indices().nth(max - 1).map(|(i, _)| i).unwrap_or(max);
        format!("{}…", &s[..cut])
    }
}
