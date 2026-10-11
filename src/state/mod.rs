use std::collections::HashSet;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::catalog::{Lang, ProblemId};

#[derive(Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct AppState {
    pub solved: HashSet<ProblemId>,
    pub last_problem_id: Option<ProblemId>,
    pub language: Lang,
}

#[cfg(not(test))]
pub fn config_dir() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("/tmp"))
        .join(".grind-tui")
}

/// Tests never see the real home: each test thread gets its own directory,
/// removed when the thread ends.
#[cfg(test)]
pub fn config_dir() -> PathBuf {
    test_home::HOME.with(|home| home.0.clone())
}

#[cfg(test)]
mod test_home {
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicUsize, Ordering};

    static NEXT: AtomicUsize = AtomicUsize::new(0);

    pub struct TestHome(pub PathBuf);

    impl Drop for TestHome {
        fn drop(&mut self) {
            std::fs::remove_dir_all(&self.0).ok();
        }
    }

    thread_local! {
        pub static HOME: TestHome = TestHome(std::env::temp_dir().join(format!(
            "grind-tui-home-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed),
        )));
    }
}

impl AppState {
    /// A missing file is a fresh start; an unreadable one is an error so the
    /// caller never overwrites the user's solved set with an empty one.
    pub fn load(dir: &Path) -> Result<Self, String> {
        let path = dir.join("state.json");
        match fs::read_to_string(&path) {
            Ok(contents) => serde_json::from_str(&contents)
                .map_err(|e| format!("{} is not valid: {e}", path.display())),
            Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(Self::default()),
            Err(e) => Err(format!("cannot read {}: {e}", path.display())),
        }
    }

    pub fn save(&self, dir: &Path) -> io::Result<()> {
        fs::create_dir_all(dir)?;
        let contents = serde_json::to_string_pretty(self)?;
        fs::write(dir.join("state.json"), contents)
    }

    pub fn toggle_solved(&mut self, id: ProblemId) {
        if !self.solved.remove(&id) {
            self.solved.insert(id);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("grind-tui-test-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn legacy_file_parses_and_keeps_ids_outside_the_catalog() {
        let dir = temp_dir("legacy");
        fs::write(
            dir.join("state.json"),
            r#"{"solved":[1,349],"bookmarks":[],"last_problem_id":142,"notes":{},"language":"javascript"}"#,
        )
        .unwrap();

        let mut state = AppState::load(&dir).unwrap();
        assert_eq!(state.language, Lang::Javascript);
        assert_eq!(state.last_problem_id, Some(ProblemId(142)));

        state.toggle_solved(ProblemId(20));
        state.save(&dir).unwrap();

        let reloaded = AppState::load(&dir).unwrap();
        let expected: HashSet<ProblemId> = [1, 349, 20].into_iter().map(ProblemId).collect();
        assert_eq!(reloaded.solved, expected);
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn tests_get_a_private_config_dir_outside_the_real_home() {
        let real = dirs::home_dir().unwrap().join(".grind-tui");
        let dir = config_dir();
        assert!(!dir.starts_with(&real), "{} is the real config dir", dir.display());
        assert!(dir.starts_with(std::env::temp_dir()));
        let other = std::thread::spawn(config_dir).join().unwrap();
        assert_ne!(dir, other);
    }

    #[test]
    fn missing_fields_default_instead_of_failing() {
        let state: AppState = serde_json::from_str(r#"{"solved":[349]}"#).unwrap();
        assert!(state.solved.contains(&ProblemId(349)));
        assert_eq!(state.language, Lang::Python3);
    }

    #[test]
    fn missing_file_is_default_and_corrupt_file_is_an_error() {
        let dir = temp_dir("corrupt");
        assert_eq!(AppState::load(&dir).unwrap(), AppState::default());
        fs::write(dir.join("state.json"), "{not json").unwrap();
        assert!(AppState::load(&dir).is_err());
        fs::remove_dir_all(&dir).unwrap();
    }
}
