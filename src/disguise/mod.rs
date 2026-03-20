use crossterm::{
    event::{self, Event, KeyCode, KeyEventKind},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{
    backend::CrosstermBackend,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},

    widgets::{Block, Borders, Cell, Paragraph, Row, Table, TableState},
    Frame, Terminal,
};
use std::io;

use crate::tui::{App, Difficulty, Panel};

// ── Theme ────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq)]
enum DisguiseTheme {
    Budget,
    Meeting,
}

impl DisguiseTheme {
    fn toggle(&self) -> Self {
        match self {
            DisguiseTheme::Budget => DisguiseTheme::Meeting,
            DisguiseTheme::Meeting => DisguiseTheme::Budget,
        }
    }
}

// ── Theme-specific display helpers ───────────────────────────────────────────

fn budget_category(difficulty: &Difficulty) -> &'static str {
    match difficulty {
        Difficulty::Easy => "Operations",
        Difficulty::Medium => "Engineering",
        Difficulty::Hard => "Strategic",
        Difficulty::Unknown => "General",
    }
}

fn budget_amount(difficulty: &Difficulty) -> &'static str {
    match difficulty {
        Difficulty::Easy => "$1,200",
        Difficulty::Medium => "$8,500",
        Difficulty::Hard => "$45,000",
        Difficulty::Unknown => "$500",
    }
}

fn meeting_duration(difficulty: &Difficulty) -> &'static str {
    match difficulty {
        Difficulty::Easy => "5 min",
        Difficulty::Medium => "15 min",
        Difficulty::Hard => "30 min",
        Difficulty::Unknown => "5 min",
    }
}

// ── Rendering ─────────────────────────────────────────────────────────────────

fn draw(
    f: &mut Frame,
    app: &App,
    theme: &DisguiseTheme,
    table_state: &mut TableState,
) {
    let area = f.area();

    // Outer vertical split: title + content + status
    let outer = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1), // title bar
            Constraint::Min(0),    // main content
            Constraint::Length(1), // status bar
        ])
        .split(area);

    draw_title(f, outer[0], theme);
    draw_main(f, outer[1], app, theme, table_state);
    draw_status(f, outer[2], app, theme);
}

fn draw_title(f: &mut Frame, area: Rect, theme: &DisguiseTheme) {
    let text = match theme {
        DisguiseTheme::Budget => "Q4 2026 Budget Planning | Finance Department",
        DisguiseTheme::Meeting => "Weekly Sync - Engineering Team - Agenda",
    };
    let title = Paragraph::new(text).style(
        Style::default()
            .bg(Color::Blue)
            .fg(Color::White)
            .add_modifier(Modifier::BOLD),
    );
    f.render_widget(title, area);
}

fn draw_main(
    f: &mut Frame,
    area: Rect,
    app: &App,
    theme: &DisguiseTheme,
    table_state: &mut TableState,
) {
    // Horizontal split: list (left) | detail panels (right)
    let cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(45), Constraint::Percentage(55)])
        .split(area);

    draw_list(f, cols[0], app, theme, table_state);
    draw_detail(f, cols[1], app, theme);
}

fn draw_list(
    f: &mut Frame,
    area: Rect,
    app: &App,
    theme: &DisguiseTheme,
    table_state: &mut TableState,
) {
    let (col1, col2, col3, col4) = match theme {
        DisguiseTheme::Budget => ("  #  ", "  Vendor / Description          ", "  Category    ", "   Amount   "),
        DisguiseTheme::Meeting => ("  #  ", "  Agenda Item                   ", "  Owner       ", "  Duration  "),
    };
    let status_header = "  Status  ";

    let header_cols = [col1, col2, col3, col4, status_header];
    let header_cells = header_cols.iter().map(|h| {
        Cell::from(*h).style(
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD | Modifier::UNDERLINED),
        )
    });
    let header = Row::new(header_cells).height(1);

    let rows: Vec<Row> = app
        .problems
        .iter()
        .map(|p| {
            let id_str = format!(" {:>4} ", p.id);

            let (col3_val, col4_val) = match theme {
                DisguiseTheme::Budget => (
                    budget_category(&p.difficulty).to_string(),
                    budget_amount(&p.difficulty).to_string(),
                ),
                DisguiseTheme::Meeting => (
                    // Owner column — use difficulty label repurposed
                    match p.difficulty {
                        Difficulty::Easy => "Team",
                        Difficulty::Medium => "Lead",
                        Difficulty::Hard => "Director",
                        Difficulty::Unknown => "TBD",
                    }
                    .to_string(),
                    meeting_duration(&p.difficulty).to_string(),
                ),
            };

            let (status_text, status_style) = if p.solved {
                match theme {
                    DisguiseTheme::Budget => (
                        "✓ PAID   ",
                        Style::default().fg(Color::Green).add_modifier(Modifier::BOLD),
                    ),
                    DisguiseTheme::Meeting => (
                        "✅ Done  ",
                        Style::default().fg(Color::Green),
                    ),
                }
            } else {
                match theme {
                    DisguiseTheme::Budget => (
                        "PENDING  ",
                        Style::default().fg(Color::Yellow),
                    ),
                    DisguiseTheme::Meeting => (
                        "🔲 Todo  ",
                        Style::default().fg(Color::Gray),
                    ),
                }
            };

            Row::new(vec![
                Cell::from(id_str),
                Cell::from(format!(" {:<30} ", p.title)),
                Cell::from(format!(" {:<12} ", col3_val)),
                Cell::from(format!(" {:>10} ", col4_val)),
                Cell::from(status_text).style(status_style),
            ])
            .height(1)
        })
        .collect();

    let block_title = match theme {
        DisguiseTheme::Budget => "Expense Notes / Justification:",
        DisguiseTheme::Meeting => "Agenda Items",
    };

    // Widen the list block title only in budget mode for visual plausibility
    let list_block_title = match theme {
        DisguiseTheme::Budget => " Line Items ",
        DisguiseTheme::Meeting => " Agenda ",
    };

    let table = Table::new(
        rows,
        [
            Constraint::Length(6),
            Constraint::Min(32),
            Constraint::Length(14),
            Constraint::Length(12),
            Constraint::Length(10),
        ],
    )
    .header(header)
    .block(
        Block::default()
            .borders(Borders::ALL)
            .title(list_block_title),
    )
    .row_highlight_style(
        Style::default()
            .bg(Color::DarkGray)
            .add_modifier(Modifier::BOLD),
    );

    f.render_stateful_widget(table, area, table_state);
    let _ = block_title; // used in draw_detail
}

fn draw_detail(f: &mut Frame, area: Rect, app: &App, theme: &DisguiseTheme) {
    // Vertical split: description | code | output
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage(35),
            Constraint::Percentage(40),
            Constraint::Percentage(25),
        ])
        .split(area);

    let selected_problem = app.problems.get(app.selected);

    // Description panel
    let desc_title = match theme {
        DisguiseTheme::Budget => " Expense Notes / Justification: ",
        DisguiseTheme::Meeting => " Agenda Item Details ",
    };
    let desc_text = selected_problem
        .map(|p| p.description.as_str())
        .unwrap_or("");
    let desc_active = app.active_panel == Panel::Description;
    let desc_block = Block::default()
        .borders(Borders::ALL)
        .title(desc_title)
        .border_style(if desc_active {
            Style::default().fg(Color::Cyan)
        } else {
            Style::default()
        });
    let desc_para = Paragraph::new(desc_text)
        .block(desc_block)
        .wrap(ratatui::widgets::Wrap { trim: false });
    f.render_widget(desc_para, rows[0]);

    // Code panel
    let code_title = match theme {
        DisguiseTheme::Budget => " Supporting Documentation: ",
        DisguiseTheme::Meeting => " Action Items: ",
    };
    let code_text = selected_problem
        .map(|p| p.code.as_str())
        .unwrap_or("");
    let code_active = app.active_panel == Panel::Code;
    let code_block = Block::default()
        .borders(Borders::ALL)
        .title(code_title)
        .border_style(if code_active {
            Style::default().fg(Color::Cyan)
        } else {
            Style::default()
        });
    let code_para = Paragraph::new(code_text)
        .block(code_block)
        .wrap(ratatui::widgets::Wrap { trim: false });
    f.render_widget(code_para, rows[1]);

    // Output panel
    let output_title = match theme {
        DisguiseTheme::Budget => " Approval Comments: ",
        DisguiseTheme::Meeting => " Notes: ",
    };
    let output_active = app.active_panel == Panel::Output;
    let output_block = Block::default()
        .borders(Borders::ALL)
        .title(output_title)
        .border_style(if output_active {
            Style::default().fg(Color::Cyan)
        } else {
            Style::default()
        });
    let output_para = Paragraph::new(app.output.as_str())
        .block(output_block)
        .wrap(ratatui::widgets::Wrap { trim: false });
    f.render_widget(output_para, rows[2]);
}

fn draw_status(f: &mut Frame, area: Rect, app: &App, theme: &DisguiseTheme) {
    let total = app.problems.len();
    let solved = app.solved_count;

    let text = match theme {
        DisguiseTheme::Budget => format!(
            " Q4 Budget: $2.4M Total │ Approved: {} │ Pending: {} │ [TAB] switch panel │ [B] meeting mode │ [D] classic mode",
            solved,
            total.saturating_sub(solved),
        ),
        DisguiseTheme::Meeting => format!(
            " Next meeting: Thursday 3pm │ Items: {} │ Completed: {} │ [B] budget mode │ [D] classic mode",
            total, solved,
        ),
    };

    let status = Paragraph::new(text).style(
        Style::default()
            .bg(Color::DarkGray)
            .fg(Color::White),
    );
    f.render_widget(status, area);
}

// ── Event handling ────────────────────────────────────────────────────────────

fn handle_key(app: &mut App, theme: &mut DisguiseTheme, key: KeyCode) -> Option<crate::tui::ExitReason> {
    match key {
        // Switch back to classic mode
        KeyCode::Char('d') => return Some(crate::tui::ExitReason::SwitchToClassic),

        // Toggle between budget and meeting themes
        KeyCode::Char('b') => {
            *theme = theme.toggle();
        }

        // Navigation in list panel
        KeyCode::Char('j') | KeyCode::Down => {
            if app.active_panel == Panel::List && !app.problems.is_empty() {
                app.selected = (app.selected + 1).min(app.problems.len() - 1);
            }
        }
        KeyCode::Char('k') | KeyCode::Up => {
            if app.active_panel == Panel::List && app.selected > 0 {
                app.selected -= 1;
            }
        }

        // Switch panels with Tab
        KeyCode::Tab => {
            app.active_panel = match app.active_panel {
                Panel::List => Panel::Description,
                Panel::Description => Panel::Code,
                Panel::Code => Panel::Output,
                Panel::Output => Panel::List,
            };
        }

        // Quit
        KeyCode::Char('q') => return Some(crate::tui::ExitReason::Quit),

        _ => {}
    }
    None
}

// ── Public entry point ────────────────────────────────────────────────────────

pub async fn run(app: &mut App) -> Result<crate::tui::ExitReason, Box<dyn std::error::Error>> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let mut theme = DisguiseTheme::Budget;
    let mut table_state = TableState::default();
    table_state.select(Some(app.selected));

    let result = event_loop(&mut terminal, app, &mut theme, &mut table_state);

    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;

    result
}

fn event_loop(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    app: &mut App,
    theme: &mut DisguiseTheme,
    table_state: &mut TableState,
) -> Result<crate::tui::ExitReason, Box<dyn std::error::Error>> {
    loop {
        // Sync table selection with app state
        table_state.select(Some(app.selected));

        terminal.draw(|f| draw(f, app, theme, table_state))?;

        if let Event::Key(key) = event::read()? {
            if key.kind != KeyEventKind::Press {
                continue;
            }
            if let Some(reason) = handle_key(app, theme, key.code) {
                return Ok(reason);
            }
        }
    }
}
