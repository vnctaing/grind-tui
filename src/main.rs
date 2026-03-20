mod api;
mod disguise;
mod state;
mod tui;

use clap::Parser;
use state::{AppMode, AppState};
use tui::{App, Difficulty, ExitReason, Problem};

#[derive(Parser)]
#[command(name = "leetcode-tui", about = "LeetCode in your terminal")]
struct Cli {
    #[arg(short, long, help = "Start in disguise mode")]
    disguise: bool,
    #[arg(long, env = "LEETCODE_SESSION", help = "LeetCode session token")]
    session: Option<String>,
    #[arg(long, env = "LEETCODE_CSRF", help = "CSRF token")]
    csrf: Option<String>,
    #[arg(long, help = "Filter: easy|medium|hard")]
    filter: Option<String>,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();
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
