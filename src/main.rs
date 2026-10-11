mod catalog;
mod state;
mod tui;

use catalog::{Catalog, Difficulty, ListId};
use clap::{Parser, ValueEnum};
use state::AppState;
use tui::{App, ListFilter};

#[derive(Parser)]
#[command(name = "grind", version, about = "grind-tui: Grind 75 and NeetCode 150 in your terminal, offline")]
struct Cli {
    #[arg(long, value_enum, default_value_t = ListArg::All, help = "List to show on launch")]
    list: ListArg,
    #[arg(long, value_enum, help = "Only show problems of this difficulty")]
    difficulty: Option<DifficultyArg>,
}

#[derive(Clone, Copy, ValueEnum)]
enum ListArg { All, Grind75, Neetcode150 }

#[derive(Clone, Copy, ValueEnum)]
enum DifficultyArg { Easy, Medium, Hard }

impl From<ListArg> for ListFilter {
    fn from(arg: ListArg) -> Self {
        match arg {
            ListArg::All => ListFilter::All,
            ListArg::Grind75 => ListFilter::Only(ListId::Grind75),
            ListArg::Neetcode150 => ListFilter::Only(ListId::NeetCode150),
        }
    }
}

impl From<DifficultyArg> for Difficulty {
    fn from(arg: DifficultyArg) -> Self {
        match arg {
            DifficultyArg::Easy => Difficulty::Easy,
            DifficultyArg::Medium => Difficulty::Medium,
            DifficultyArg::Hard => Difficulty::Hard,
        }
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();
    let dir = state::config_dir();
    let progress = AppState::load(&dir)?;

    let mut app = App::new(
        Catalog::embedded(),
        progress,
        tui::load_editors(),
        cli.list.into(),
        cli.difficulty.map(Difficulty::from),
    );
    tui::run(&mut app)?;

    if let Some(idx) = app.selected() {
        app.progress.last_problem_id = Some(app.catalog.problems()[idx].id);
    }
    app.progress.save(&dir)?;
    Ok(())
}
