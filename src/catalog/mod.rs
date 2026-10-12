use std::collections::{HashMap, HashSet};

use serde::{Deserialize, Serialize};

pub const CATALOG_JSON: &str = include_str!("../../data/catalog.json");

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ProblemId(pub u32); // LeetCode frontend id; same numbers state.json already stores

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
pub enum Difficulty {
    Easy,
    Medium,
    Hard,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum Body {
    Free {
        description: String,
        starter: String,
    },
    Premium,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Problem {
    pub id: ProblemId,
    pub slug: String,
    pub title: String,
    pub difficulty: Difficulty,
    pub examples: String,
    pub body: Body,
}

impl Problem {
    pub fn starter(&self) -> &str {
        match &self.body {
            Body::Free { starter, .. } => starter,
            Body::Premium => "",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ListId {
    Grind75,
    NeetCode150,
}

pub struct Catalog {
    problems: Vec<Problem>,
    grind75: Vec<usize>,
    neetcode150: Vec<usize>,
}

#[derive(Deserialize)]
struct RawCatalog {
    lists: RawLists,
    problems: Vec<Problem>,
}

#[derive(Deserialize)]
struct RawLists {
    grind75: Vec<String>,
    neetcode150: Vec<String>,
}

impl Catalog {
    pub fn embedded() -> Catalog {
        Catalog::parse(CATALOG_JSON)
            .unwrap_or_else(|e| panic!("embedded catalog is invalid: {e}"))
    }

    pub fn parse(json: &str) -> Result<Catalog, String> {
        let raw: RawCatalog = serde_json::from_str(json).map_err(|e| e.to_string())?;

        let mut by_slug: HashMap<&str, usize> = HashMap::new();
        let mut ids = HashSet::new();
        for (i, problem) in raw.problems.iter().enumerate() {
            if !ids.insert(problem.id) {
                return Err(format!("duplicate problem id {}", problem.id.0));
            }
            if by_slug.insert(problem.slug.as_str(), i).is_some() {
                return Err(format!("duplicate problem slug {}", problem.slug));
            }
        }

        let resolve = |name: &str, slugs: &[String]| -> Result<Vec<usize>, String> {
            let mut seen = HashSet::new();
            slugs
                .iter()
                .map(|slug| {
                    if !seen.insert(slug.as_str()) {
                        return Err(format!("{name} lists {slug} twice"));
                    }
                    by_slug
                        .get(slug.as_str())
                        .copied()
                        .ok_or_else(|| format!("{name} lists unknown problem {slug}"))
                })
                .collect()
        };
        let grind75 = resolve("grind75", &raw.lists.grind75)?;
        let neetcode150 = resolve("neetcode150", &raw.lists.neetcode150)?;

        let mut listed = vec![false; raw.problems.len()];
        for &i in grind75.iter().chain(&neetcode150) {
            listed[i] = true;
        }
        if let Some(i) = listed.iter().position(|&l| !l) {
            return Err(format!("problem {} is in no list", raw.problems[i].slug));
        }

        Ok(Catalog {
            problems: raw.problems,
            grind75,
            neetcode150,
        })
    }

    pub fn problems(&self) -> &[Problem] {
        &self.problems
    }

    pub fn list(&self, id: ListId) -> &[usize] {
        match id {
            ListId::Grind75 => &self.grind75,
            ListId::NeetCode150 => &self.neetcode150,
        }
    }

    pub fn in_list(&self, idx: usize, id: ListId) -> bool {
        self.list(id).contains(&idx)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn index_of(catalog: &Catalog, slug: &str) -> usize {
        catalog
            .problems()
            .iter()
            .position(|p| p.slug == slug)
            .unwrap_or_else(|| panic!("no problem {slug}"))
    }

    fn embedded_json() -> serde_json::Value {
        serde_json::from_str(CATALOG_JSON).unwrap()
    }

    fn parse_value(value: &serde_json::Value) -> Result<Catalog, String> {
        Catalog::parse(&value.to_string())
    }

    #[test]
    fn list_lengths_and_union() {
        let c = Catalog::embedded();
        assert_eq!(c.list(ListId::Grind75).len(), 75);
        assert_eq!(c.list(ListId::NeetCode150).len(), 150);
        assert_eq!(c.problems().len(), 167);
        let union: HashSet<usize> = c
            .list(ListId::Grind75)
            .iter()
            .chain(c.list(ListId::NeetCode150))
            .copied()
            .collect();
        assert_eq!(union.len(), 167);
    }

    #[test]
    fn membership_of_known_problems() {
        let c = Catalog::embedded();
        let two_sum = index_of(&c, "two-sum");
        assert!(c.in_list(two_sum, ListId::Grind75));
        assert!(c.in_list(two_sum, ListId::NeetCode150));
        let matrix = index_of(&c, "01-matrix");
        assert!(c.in_list(matrix, ListId::Grind75));
        assert!(!c.in_list(matrix, ListId::NeetCode150));
    }

    #[test]
    fn premium_problem_has_no_starter() {
        let c = Catalog::embedded();
        let meeting = &c.problems()[index_of(&c, "meeting-rooms")];
        assert!(matches!(meeting.body, Body::Premium));
        assert_eq!(meeting.starter(), "");
    }

    #[test]
    fn every_free_problem_has_a_javascript_starter() {
        let c = Catalog::embedded();
        for p in c.problems() {
            if matches!(p.body, Body::Free { .. }) {
                assert!(!p.starter().is_empty(), "{}", p.slug);
            }
        }
        let two_sum = &c.problems()[index_of(&c, "two-sum")];
        assert!(two_sum.starter().contains("var twoSum = function(nums, target)"), "{}", two_sum.starter());
    }

    #[test]
    fn descriptions_are_plain_markdown_lite() {
        let c = Catalog::embedded();
        for p in c.problems() {
            if let Body::Free { description, .. } = &p.body {
                assert!(!description.contains("&#"), "{}", p.slug);
                assert!(!description.contains("&nbsp;"), "{}", p.slug);
                assert!(!description.contains("<strong"), "{}", p.slug);
                assert!(!description.contains("<code"), "{}", p.slug);
                assert!(!description.contains("<p>"), "{}", p.slug);
            }
        }
    }

    #[test]
    fn parse_rejects_list_slug_missing_from_problems() {
        let mut v = embedded_json();
        v["lists"]["grind75"]
            .as_array_mut()
            .unwrap()
            .push("no-such-problem".into());
        assert!(parse_value(&v).is_err());
    }

    #[test]
    fn parse_rejects_problem_in_no_list() {
        let mut v = embedded_json();
        for name in ["grind75", "neetcode150"] {
            v["lists"][name]
                .as_array_mut()
                .unwrap()
                .retain(|s| s != "two-sum");
        }
        let err = parse_value(&v).err().unwrap();
        assert!(err.contains("two-sum"), "{err}");
    }
}
