mod api;
mod disguise;
mod state;
mod tui;

use clap::{Parser, Subcommand};
use state::{AppMode, AppState};
use tui::{App, Difficulty, ExitReason, Problem};

#[derive(Parser)]
#[command(name = "leet", about = "Metal Gear Soleetcode — LeetCode in your terminal")]
struct Cli {
    #[command(subcommand)]
    command: Option<Cmd>,
    #[arg(short, long, help = "Start in disguise mode")]
    disguise: bool,
    #[arg(long, env = "LEETCODE_SESSION", help = "LeetCode session token")]
    session: Option<String>,
    #[arg(long, env = "LEETCODE_CSRF", help = "CSRF token")]
    csrf: Option<String>,
    #[arg(long, help = "Filter: easy|medium|hard")]
    filter: Option<String>,
}

#[derive(Subcommand)]
enum Cmd {
    /// Interactive setup: grab your LeetCode session from Chrome and save it
    Setup,
}

fn run_setup() -> Result<(), Box<dyn std::error::Error>> {
    println!();
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
    println!("  Metal Gear Soleetcode — Setup");
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
    println!();
    println!("Step 1 — Open Chrome, go to leetcode.com and log in.");
    println!("         Then open DevTools (Cmd+Option+I) → Console tab.");
    println!();
    println!("         Paste this snippet and hit Enter:");
    println!();
    println!("  ┌─────────────────────────────────────────────────────┐");
    println!("  │ const c = Object.fromEntries(document.cookie        │");
    println!("  │   .split('; ').map(x => x.split(/=(.*)/s).slice(0,2)│");
    println!("  │   .map(([k,v]) => [k,v]));                          │");
    println!("  │ console.log(c['LEETCODE_SESSION']);                  │");
    println!("  │ console.log(c['csrftoken']);                         │");
    println!("  └─────────────────────────────────────────────────────┘");
    println!();
    println!("Step 2 — Paste the two values below.");
    println!();

    print!("  LEETCODE_SESSION: ");
    std::io::Write::flush(&mut std::io::stdout())?;
    let mut session_token = String::new();
    std::io::stdin().read_line(&mut session_token)?;
    let session_token = session_token.trim().to_string();

    print!("  csrftoken:        ");
    std::io::Write::flush(&mut std::io::stdout())?;
    let mut csrf_token = String::new();
    std::io::stdin().read_line(&mut csrf_token)?;
    let csrf_token = csrf_token.trim().to_string();

    if session_token.is_empty() || csrf_token.is_empty() {
        eprintln!("\n  ✗ Empty value — setup aborted.");
        return Ok(());
    }

    let session = api::Session { session_token, csrf_token };
    api::save_session(&session);

    let path = state::config_dir().join("session.json");
    println!();
    println!("  ✓ Saved to {}", path.display());
    println!();
    println!("  Run `leet` to start.");
    println!();
    Ok(())
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();

    if let Some(Cmd::Setup) = cli.command {
        return run_setup();
    }
    let mut app_state = AppState::load();

    let mut mode = if cli.disguise { AppMode::Disguise } else { AppMode::Classic };

    // Build API client if session is available
    let session = cli.session.zip(cli.csrf).map(|(s, c)| api::Session {
        session_token: s,
        csrf_token: c,
    }).or_else(api::load_session);

    // Build TUI App — optionally load problems via API
    let mut app = App::new();
    app.language = app_state.language.clone();

    if let Some(ref sess) = session {
        let client = std::sync::Arc::new(api::LeetCodeClient::new(sess.clone()));
        app.client = Some(client.clone());
        match client.get_all_problems().await {
            Ok(problems) => {
                let filter = cli.filter.as_deref().map(str::to_lowercase);
                app.problems = problems
                    .into_iter()
                    .filter(|p| {
                        filter.as_deref().map_or(true, |f| p.difficulty.to_lowercase() == f)
                    })
                    .map(|p| Problem {
                        id: p.id,
                        title: p.title,
                        slug: p.slug.clone(),
                        difficulty: Difficulty::from_str(&p.difficulty),
                        solved: app_state.solved.contains(&p.id),
                        description: String::new(), // loaded on demand
                        code: String::new(),
                        starter_code: std::collections::HashMap::new(),
                        test_cases: String::new(),
                    })
                    .collect();
                app.solved_count = app_state.solved.len();
            }
            Err(e) => {
                eprintln!("Warning: could not load problems: {e}");
                eprintln!("Set LEETCODE_SESSION and LEETCODE_CSRF to authenticate.");
            }
        }
    } else {
        // Demo problems so the TUI is usable without credentials
        app.problems = vec![
            Problem {
                id: 1,
                title: "Two Sum".into(),
                slug: "two-sum".into(),
                difficulty: Difficulty::Easy,
                solved: false,
                description: "Given an array of integers nums and an integer target, return indices of the two numbers such that they add up to target.\n\nExample:\nInput: nums = [2,7,11,15], target = 9\nOutput: [0,1]".into(),
                code: "def twoSum(self, nums, target):\n    seen = {}\n    for i, n in enumerate(nums):\n        if target - n in seen:\n            return [seen[target-n], i]\n        seen[n] = i".into(),
                starter_code: std::collections::HashMap::new(),
                test_cases: String::new(),
            },
            Problem {
                id: 2,
                title: "Add Two Numbers".into(),
                slug: "add-two-numbers".into(),
                difficulty: Difficulty::Medium,
                solved: false,
                description: "You are given two non-empty linked lists representing two non-negative integers.".into(),
                code: String::new(),
                starter_code: std::collections::HashMap::new(),
                test_cases: String::new(),
            },
            Problem {
                id: 3,
                title: "Longest Substring Without Repeating Characters".into(),
                slug: "longest-substring-without-repeating-characters".into(),
                difficulty: Difficulty::Medium,
                solved: false,
                description: "Given a string s, find the length of the longest substring without repeating characters.".into(),
                code: String::new(),
                starter_code: std::collections::HashMap::new(),
                test_cases: String::new(),
            },
            Problem {
                id: 4,
                title: "Median of Two Sorted Arrays".into(),
                slug: "median-of-two-sorted-arrays".into(),
                difficulty: Difficulty::Hard,
                solved: false,
                description: "Given two sorted arrays nums1 and nums2 of size m and n respectively, return the median of the two sorted arrays.".into(),
                code: String::new(),
                starter_code: std::collections::HashMap::new(),
                test_cases: String::new(),
            },
        ];
        app.output = "No session found. Set --session and --csrf to load real problems.".to_string();
    }

    app.rebuild_filtered();

    // Main mode loop — switch between classic and disguise
    loop {
        match mode {
            AppMode::Classic => {
                match tui::run(&mut app).await? {
                    ExitReason::Quit => break,
                    ExitReason::SwitchToDisguise => {
                        mode = AppMode::Disguise;
                        continue;
                    }
                    ExitReason::SwitchToClassic => continue,
                }
            }
            AppMode::Disguise => {
                match disguise::run(&mut app).await? {
                    ExitReason::Quit => break,
                    ExitReason::SwitchToClassic => {
                        mode = AppMode::Classic;
                        continue;
                    }
                    _ => break,
                }
            }
        }
    }

    // Persist state
    app_state.language = app.language.clone();
    app_state.solved = app
        .problems
        .iter()
        .filter(|p| p.solved)
        .map(|p| p.id)
        .collect();
    app_state.save();

    Ok(())
}
