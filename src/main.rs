mod api;
mod catalog;
mod state;
mod tui;

use clap::{Parser, Subcommand};
use state::AppState;
use tui::{App, Difficulty, Problem};

#[derive(Parser)]
#[command(name = "grind", about = "grind-tui — LeetCode in your terminal")]
struct Cli {
    #[command(subcommand)]
    command: Option<Cmd>,
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
    /// Remove saved session credentials
    Logout,
}

async fn run_setup() -> Result<(), Box<dyn std::error::Error>> {
    let r   = "\x1b[0m";
    let b   = "\x1b[1m";
    let dim = "\x1b[2m";
    let cyn = "\x1b[96m";
    let mag = "\x1b[95m";
    let yel = "\x1b[93m";
    let grn = "\x1b[92m";
    let red = "\x1b[91m";

    // Center the 50-char-wide block in the terminal
    let content_width = 50usize;
    let cols = crossterm::terminal::size().map(|(w, _)| w as usize).unwrap_or(80);
    let p = " ".repeat(cols.saturating_sub(content_width) / 2);
    let s2 = "  ";    // +2 relative indent
    let s5 = "     "; // +5 relative indent (sub-items)
    // Center title/subtitle within the 50-char block
    let title_pad = " ".repeat((content_width - 29) / 2); // "░▒▓  G R I N D  —  T U I  ▓▒░" = 29
    let sub_pad   = " ".repeat((content_width - 27) / 2); // "leetcode · in your terminal"   = 27

    println!();
    println!("{p}{mag}▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓{r}");
    println!();
    println!("{p}{title_pad}{mag}░▒▓{r}  {b}{cyn}G R I N D  —  T U I{r}  {mag}▓▒░{r}");
    println!("{p}{sub_pad}{dim}leetcode · in your terminal{r}");
    println!();
    println!("{p}{mag}▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓{r}");
    println!();

    if let Some(existing) = api::load_session() {
        let preview = |s: &str| if s.len() > 20 { format!("{}…", &s[..20]) } else { s.to_string() };
        println!("{p}{grn}✓{r} Session already configured:");
        println!("{p}{s2}{dim}LEETCODE_SESSION:{r} {}", preview(&existing.session_token));
        println!("{p}{s2}{dim}csrftoken:       {r} {}", preview(&existing.csrf_token));
        println!();
        print!("{p}{yel}Overwrite?{r} [y/N]: ");
        std::io::Write::flush(&mut std::io::stdout())?;
        let mut answer = String::new();
        std::io::stdin().read_line(&mut answer)?;
        if !answer.trim().eq_ignore_ascii_case("y") {
            println!("{p}{dim}Kept existing session.{r}  Run {b}{cyn}grind{r} to start.");
            println!();
            return Ok(());
        }
        println!();
    }

    println!("{p}{b}{yel}①{r}  Log in to LeetCode in Chrome.");
    println!();
    println!("{p}{b}{yel}②{r}  Open DevTools {dim}→{r} {cyn}Cmd+Option+I{r}");
    println!();
    println!("{p}{b}{yel}③{r}  Navigate to:");
    println!("{p}{s5}{cyn}Application{r} {dim}→{r} {cyn}Cookies{r} {dim}→{r} {cyn}https://leetcode.com{r}");
    println!();
    println!("{p}{b}{yel}④{r}  Find and copy these two cookie values:");
    println!("{p}{s5}{b}{cyn}LEETCODE_SESSION{r}");
    println!("{p}{s5}{b}{cyn}csrftoken{r}");
    println!();
    println!("{p}{dim}────────────────────────────────────────{r}");
    println!();

    print!("{p}{yel}LEETCODE_SESSION{r}  › ");
    std::io::Write::flush(&mut std::io::stdout())?;
    let mut session_token = String::new();
    std::io::stdin().read_line(&mut session_token)?;
    let session_token = session_token.trim().to_string();

    print!("{p}{yel}csrftoken        {r} › ");
    std::io::Write::flush(&mut std::io::stdout())?;
    let mut csrf_token = String::new();
    std::io::stdin().read_line(&mut csrf_token)?;
    let csrf_token = csrf_token.trim().to_string();

    if session_token.is_empty() || csrf_token.is_empty() {
        eprintln!("\n{p}{red}✗{r} Empty value — setup aborted.");
        return Ok(());
    }

    let session = api::Session { session_token, csrf_token };
    api::save_session(&session);

    println!();
    print!("{p}Verifying session{dim}…{r}  ");
    std::io::Write::flush(&mut std::io::stdout())?;
    let client = api::LeetCodeClient::new(session);
    match client.verify_session().await {
        Ok(username) => {
            println!("{grn}✓{r}  signed in as {b}{cyn}{username}{r}");
        }
        Err(_) => {
            println!("{red}✗{r}  tokens invalid or expired.");
            println!("{p}{s5}Run {b}{cyn}grind setup{r} again to re-enter your tokens.");
        }
    }

    let path = state::config_dir().join("session.json");
    println!();
    println!("{p}{dim}Saved to {}{r}", path.display());
    println!();
    println!("{p}Run {b}{cyn}grind{r} to start grinding. {mag}▓▒░{r}");
    println!();
    Ok(())
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();

    if let Some(Cmd::Setup) = cli.command {
        return run_setup().await;
    }
    if let Some(Cmd::Logout) = cli.command {
        let path = state::config_dir().join("session.json");
        if path.exists() {
            std::fs::remove_file(&path)?;
            println!("  Logged out. Session removed.");
        } else {
            println!("  No session found.");
        }
        return Ok(());
    }
    let mut app_state = AppState::load();

    // Build API client if session is available
    let session = cli.session.zip(cli.csrf).map(|(s, c)| api::Session {
        session_token: s,
        csrf_token: c,
    }).or_else(api::load_session);

    // No session found — run interactive setup before launching the TUI
    let session = if session.is_none() {
        run_setup().await?;
        api::load_session()
    } else {
        session
    };

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

    if let Some(last_id) = app_state.last_problem_id {
        if let Some(idx) = app.filtered.iter().position(|&i| app.problems[i].id == last_id) {
            app.selected = idx;
        }
    }

    tui::run(&mut app).await?;

    // Persist state
    app_state.language = app.language.clone();
    app_state.solved = app.problems.iter().filter(|p| p.solved).map(|p| p.id).collect();
    app_state.last_problem_id = app.selected_problem().map(|p| p.id);
    app_state.save();

    Ok(())
}
