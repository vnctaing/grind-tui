mod app;
mod editor;
mod highlight;
mod list_view;
mod markdown;
mod problem_view;
mod theme;

pub use app::{App, Difficulty, Problem};

use app::{handle_key_list, handle_key_problem, Action, ExitReason, Page, Panel};
use crossterm::{
    event::{self, Event, KeyCode, KeyEventKind},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use editor::code_file_path;
use list_view::draw_list_page;
use problem_view::draw_problem_page;
use ratatui::{backend::CrosstermBackend, widgets::ListState, Frame, Terminal};
use std::io;

// ── Drawing ───────────────────────────────────────────────────────────────────

fn draw(f: &mut Frame, app: &App, list_state: &mut ListState) {
    match app.page {
        Page::List => draw_list_page(f, app, list_state),
        Page::Problem => draw_problem_page(f, app),
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
