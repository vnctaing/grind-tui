use super::*;
use crate::catalog::{Body, Catalog};

/// `(line, col, message)` per diagnostic, one-based, col in chars.
fn found(src: &str) -> Vec<(usize, usize, String)> {
    Checker::new()
        .check(src)
        .into_iter()
        .map(|d| {
            let before = &src[..d.range.start];
            let line = before.matches('\n').count() + 1;
            let col = before.rsplit('\n').next().unwrap().chars().count() + 1;
            (line, col, d.message)
        })
        .collect()
}

fn expect(src: &str, want: &[(usize, usize, &str)]) {
    let want: Vec<_> = want.iter().map(|&(l, c, m)| (l, c, m.to_string())).collect();
    assert_eq!(found(src), want, "{src:?}");
}

#[test]
fn broken_snippets_point_at_the_mistake() {
    let cases: &[(&str, &[(usize, usize, &str)])] = &[
        ("function f(x) {\n  return [1, 2;\n}\n", &[(2, 15, "Missing ']'")]),
        ("function f(x) {\n  return (x + 1;\n}\n", &[(2, 16, "Missing ')'")]),
        ("function f(x) {\n  return (x + 1\n", &[(1, 15, "Unclosed '{'"), (2, 10, "Unclosed '('")]),
        ("function f(x) {\n  const y = x +;\n  return y\n}\n", &[(2, 15, "Expected an expression after '+'")]),
        ("function f(x) {\n  return x +\n}\n", &[(2, 12, "Expected an expression after '+'")]),
        ("function f(x) {\n  return `a${x`;\n}\n", &[(2, 12, "Unclosed '${'")]),
        ("function f(x) {\n  return `a${x}\n}\n", &[(2, 10, "Unterminated template string")]),
        ("function f(x) {\n  const s = \"abc;\n  return x\n}\n", &[(2, 13, "Unterminated string")]),
        ("function f(x) {\n  const s = 'it\\'s;\n  return s\n}\n", &[(2, 13, "Unterminated string")]),
        ("function f(x) {\n  return x)\n}\n", &[(2, 11, "Unexpected ')'")]),
        ("function f(x) {\n  return x]\n}\n", &[(2, 11, "Unexpected ']'")]),
        ("function f(x) {\n  /* note\n  return x;\n}\n", &[(2, 3, "Unterminated comment")]),
        ("function f(n) {\n  for (let i = 0; i < n; i++ {\n  }\n  return 0;\n}\n", &[(2, 7, "Unclosed '('")]),
    ];
    for (src, want) in cases {
        expect(src, want);
    }
}

#[test]
fn tricky_but_valid_code_has_no_diagnostics() {
    let cases: &[&str] = &[
        "function f(x) {\n  const re = /[(]\\//g;\n  const t = `a${ {b: 1}.b }(`;\n  return x.replace(re, '') / 2 / 1;\n}\n",
        "function f(ok, s) {\n  if (ok) /[(]/.test(s) && go();\n  while (ok) /[{]/.test(s);\n}\n",
        "function f(x, y) {\n  if (x) {}\n  /[(]/.test(y);\n  return (x) / (y) / 2;\n}\n",
        "function f(x) {\n  if (x) ; else g();\n}\n",
        "function f(s) {\n  // (\n  /* { [ */\n  const a = \"(\\\"\" + ')' + `\n{${s}`;\n  const b = 'a\\\nb';\n  return a + b;\n}\n",
        "var twoSum = function(nums, target) {\n  const seen = new Map();\n  for (let i = 0; i < nums.length; i++) {\n    if (seen.has(target - nums[i])) return [seen.get(target - nums[i]), i];\n    seen.set(nums[i], i);\n  }\n};\n",
    ];
    for src in cases {
        expect(src, &[]);
    }
}

#[test]
fn every_free_starter_is_clean() {
    let catalog = Catalog::embedded();
    let mut checker = Checker::new();
    let mut checked = 0;
    for p in catalog.problems() {
        if matches!(p.body, Body::Free { .. }) {
            assert_eq!(checker.check(p.starter()), vec![], "{}", p.slug);
            checked += 1;
        }
    }
    assert_eq!(checked, 160);
}

#[test]
fn an_empty_if_body_is_a_warning() {
    let src = "function f(x) {\n  if (x > 1);\n  return x;\n}\n";
    let d = Checker::new().check(src);
    let semi = src.find(");").unwrap() + 1;
    assert_eq!(d.len(), 1, "{d:?}");
    assert_eq!((d[0].range.clone(), d[0].severity, d[0].source), (semi..semi + 1, Severity::Warning, Source::Lint));
}

#[test]
fn a_missing_closer_replaces_the_unclosed_report_and_cascades_are_dropped() {
    let d = Checker::new().check("function f(x) {\n  return (x + 1;\n}\n");
    assert_eq!(d.len(), 1, "{d:?}");
    assert_eq!((d[0].range.clone(), d[0].source), (31..31, Source::Syntax));
}

struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }

    fn below(&mut self, n: usize) -> usize {
        (self.next() % n as u64) as usize
    }
}

const PIECES: &[&str] = &[
    "(", ")", "[", "]", "{", "}", "\"", "'", "`", "${", "/*", "*/", "//", "#", "\n", "\t", " ", "\\", "/", "=",
    ";", ":", "if ", "x", "return ", "é", "漢字", "🦀", "\r",
];

#[test]
fn random_edits_never_panic_and_ranges_stay_on_char_boundaries() {
    let catalog = Catalog::embedded();
    let problem = catalog.problems().iter().find(|p| p.slug == "two-sum").unwrap();
    let mut checker = Checker::new();
    for seed in 0..8u64 {
        let mut rng = Rng(0x9e37_79b9_7f4a_7c15 ^ (seed + 1));
        let mut text = problem.starter().to_string();
        for step in 0..400 {
            let boundaries: Vec<usize> = (0..=text.len()).filter(|&i| text.is_char_boundary(i)).collect();
            let a = boundaries[rng.below(boundaries.len())];
            if rng.below(3) == 0 && !text.is_empty() {
                let b = boundaries[rng.below(boundaries.len())];
                text.replace_range(a.min(b)..a.max(b), "");
            } else {
                text.insert_str(a, PIECES[rng.below(PIECES.len())]);
            }
            if text.len() > 4000 {
                text.truncate(boundaries.iter().rev().find(|&&b| b <= 2000).copied().unwrap_or(0));
            }
            let found = checker.check(&text);
            assert!(
                found.windows(2).all(|w| (w[0].range.start, &w[0].message) != (w[1].range.start, &w[1].message)),
                "seed {seed} step {step}: duplicate in {found:?}"
            );
            for d in found {
                let r = &d.range;
                assert!(
                    r.start <= r.end && r.end <= text.len() && text.is_char_boundary(r.start) && text.is_char_boundary(r.end),
                    "seed {seed} step {step}: {d:?} in {text:?}"
                );
            }
        }
    }
}

#[test]
fn findings_past_the_cap_are_dropped_in_position_order() {
    let line = "x = 'a\n";
    let src = line.repeat(60);
    let d = Checker::new().check(&src);
    assert_eq!(d.len(), MAX_DIAGNOSTICS);
    assert!(d.iter().all(|d| d.message == "Unterminated string"), "{d:?}");
    assert_eq!(d.last().unwrap().range.start, line.len() * (MAX_DIAGNOSTICS - 1) + 4);
}
