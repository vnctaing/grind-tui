mod app;
mod editor;
mod highlight;
mod list_view;
mod markdown;
mod problem_view;
mod theme;

pub use app::{App, ListFilter};
pub use editor::load_editors;

use app::{handle_key, Action, Page};
use crossterm::{
    event::{self, Event, KeyEventKind, MouseEventKind},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use editor::EditorDef;
use list_view::draw_list_page;
use problem_view::draw_problem_page;
use ratatui::{backend::CrosstermBackend, style::Style, Frame, Terminal};
use theme::t;
use std::io;
use std::time::Duration;

// ── Drawing ───────────────────────────────────────────────────────────────────

fn draw(f: &mut Frame, app: &mut App) {
    let area = f.area();
    f.buffer_mut().set_style(area, Style::default().fg(t::FG).bg(t::BG));
    match &app.page {
        Page::List => draw_list_page(f, app),
        Page::Problem(view) => draw_problem_page(f, app, view),
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

pub fn run(app: &mut App) -> io::Result<()> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    execute!(stdout, crossterm::event::EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;
    let result = event_loop(&mut terminal, app);
    disable_raw_mode()?;
    execute!(terminal.backend_mut(), crossterm::event::DisableMouseCapture)?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;
    result
}

fn event_loop(terminal: &mut Terminal<CrosstermBackend<io::Stdout>>, app: &mut App) -> io::Result<()> {
    loop {
        terminal.draw(|f| draw(f, app))?;

        let timeout = match app.page {
            Page::Problem(_) => Duration::from_millis(500),
            Page::List => Duration::from_secs(3600),
        };

        if !event::poll(timeout)? {
            if let Page::Problem(view) = &mut app.page {
                view.poll_disk();
            }
            continue;
        }

        match event::read()? {
            Event::Mouse(mouse) => {
                if let Page::Problem(view) = &mut app.page {
                    match mouse.kind {
                        MouseEventKind::ScrollDown => view.scroll_by(3),
                        MouseEventKind::ScrollUp => view.scroll_by(-3),
                        _ => {}
                    }
                }
            }
            Event::Key(key) => {
                if key.kind != KeyEventKind::Press { continue; }
                match handle_key(app, key.code) {
                    Action::None => {}
                    Action::Quit => return Ok(()),
                    Action::LaunchEditor(ed) => launch_editor(terminal, app, &ed)?,
                    Action::CopyCommand => copy_command(app),
                    Action::SyncDetached => {
                        if let Page::Problem(view) = &mut app.page {
                            view.message = match view.reload() {
                                Ok(()) => "Synced.".to_string(),
                                Err(e) => format!("Sync error: {e}"),
                            };
                        }
                    }
                }
            }
            _ => {}
        }
    }
}

fn launch_editor(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    app: &mut App,
    ed: &EditorDef,
) -> io::Result<()> {
    let Page::Problem(view) = &mut app.page else { return Ok(()) };
    if ed.detach {
        std::process::Command::new(&ed.command).arg(&view.path).spawn().ok();
        view.detached = Some(ed.name.clone());
        view.message = format!("Opened in {}, auto-syncing on save.\nFile: {}", ed.name, view.path.display());
    } else {
        suspend_tui(terminal)?;
        std::process::Command::new(&ed.command).arg(&view.path).status().ok();
        resume_tui(terminal)?;
        view.message = match view.reload() {
            Ok(()) => format!("Reloaded from {}.", ed.name),
            Err(e) => format!("Reload failed: {e}"),
        };
    }
    Ok(())
}

fn system_copy(text: &str) -> io::Result<()> {
    let mut child = std::process::Command::new("pbcopy")
        .stdin(std::process::Stdio::piped())
        .spawn()?;
    if let Some(stdin) = child.stdin.as_mut() {
        use std::io::Write;
        stdin.write_all(text.as_bytes())?;
    }
    child.wait().map(drop)
}

fn copy_command(app: &mut App) {
    let Page::Problem(view) = &mut app.page else { return };
    let cmd = format!("nvim {}", view.path.display());
    view.message = match system_copy(&cmd) {
        Ok(_) => format!("Copied: {}", cmd),
        Err(_) => format!("Copy failed. Command: {}", cmd),
    };
}
