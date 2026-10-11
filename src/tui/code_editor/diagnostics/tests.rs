use super::*;
use crate::catalog::{Body, Catalog};

/// `(line, col, message)` per diagnostic, one-based, col in chars.
fn found(lang: Lang, src: &str) -> Vec<(usize, usize, String)> {
    Checker::new(lang)
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

fn expect(lang: Lang, src: &str, want: &[(usize, usize, &str)]) {
    let want: Vec<_> = want.iter().map(|&(l, c, m)| (l, c, m.to_string())).collect();
    assert_eq!(found(lang, src), want, "{lang:?} {src:?}");
}

#[test]
fn broken_snippets_point_at_the_mistake_in_every_language() {
    let assign = "Assignment in condition: use '==' to compare";
    let cases: &[(Lang, &str, &[(usize, usize, &str)])] = &[
        (Lang::Python3, "def f(x):\n    if x > 1\n        return x\n", &[(2, 13, "Missing ':'")]),
        (Lang::Python3, "def f(x):\n    return (x + 1\n", &[(2, 12, "Unclosed '('")]),
        (Lang::Python3, "def f(x):\n    s = 'abc\n    return s\n", &[(2, 9, "Unterminated string")]),
        (Lang::Python3, "def f(x):\n    return x +\n", &[(2, 14, "Expected an expression after '+'")]),
        (Lang::Python3, "def f(x):\n    while x = 0:\n        x -= 1\n", &[(2, 13, assign)]),
        (Lang::Python3, "def f(x):\n    print x\n", &[(2, 5, "print is a function in Python 3: print(...)")]),
        (Lang::Rust, "fn f(x: i32) -> i32 {\n    let y = x + 1\n    y\n}\n", &[(2, 18, "Missing ';'")]),
        (Lang::Rust, "fn f(x: i32) -> i32 {\n    g(x;\n}\n", &[(2, 8, "Missing ')'")]),
        (Lang::Rust, "fn f(x: i32) -> i32 {\n    if x = 1 { return 2; }\n    x\n}\n", &[(2, 10, assign)]),
        (Lang::Javascript, "function f(x) {\n  return [1, 2;\n}\n", &[(2, 15, "Missing ']'")]),
        (Lang::Javascript, "function f(x) {\n  const y = x +;\n  return y\n}\n", &[(2, 15, "Expected an expression after '+'")]),
        (Lang::Javascript, "function f(x) {\n  return `a${x`;\n}\n", &[(2, 12, "Unclosed '${'")]),
        (Lang::Typescript, "function f(x: number): number {\n  const s = \"abc;\n  return x\n}\n", &[(2, 13, "Unterminated string")]),
        (Lang::Typescript, "function f(x: number): number {\n  return x)\n}\n", &[(2, 11, "Unexpected ')'")]),
        (Lang::Java, "class Solution {\n  int f(int x) {\n    return x + 1\n  }\n}\n", &[(3, 17, "Missing ';'")]),
        (Lang::Java, "class Solution {\n  int f(int x) {\n    int y = ;\n    return y;\n  }\n}\n", &[(3, 11, "Expected an expression after '='")]),
        (Lang::Cpp, "class Solution {\npublic:\n  int f(int x) { return x + 1 }\n};\n", &[(3, 30, "Missing ';'")]),
        (Lang::Cpp, "int f(int n) {\n  for (int i = 0; i < n; i++ {\n  }\n  return 0;\n}\n", &[(2, 7, "Unclosed '('")]),
        (Lang::Go, "package main\nfunc f(x int) int {\n  return x +\n}\n", &[(3, 12, "Expected an expression after '+'")]),
        (Lang::Go, "func f(x int) int {\n  return (x\n}\n", &[(2, 12, "Missing ')'")]),
        (Lang::Go, "func f(x int) int {\n  return x]\n}\n", &[(2, 11, "Unexpected ']'")]),
        (Lang::C, "int f(int x) {\n  return (x + 1;\n}\n", &[(2, 16, "Missing ')'")]),
        (Lang::C, "int f(int x) {\n  int y = x\n  return y;\n}\n", &[(2, 12, "Missing ';'")]),
        (Lang::C, "int f(int x) {\n  /* note\n  return x;\n}\n", &[(2, 3, "Unterminated comment")]),
    ];
    for (lang, src, want) in cases {
        expect(*lang, src, want);
    }
}

#[test]
fn tricky_but_valid_code_has_no_diagnostics() {
    let cases: &[(Lang, &str)] = &[
        (Lang::Python3, "def f(s):\n    t = '''(\n['''\n    u = f\"{s}(\" + r'\\d' + \"#\"  # )\n    return {'a': [1, (2, 3)]}[t]\n"),
        (Lang::Python3, "def f(x):\n    if x == 1 and g(k=2) >= 0 or x != 3:\n        return lambda y=1: y\n    while (n := x):\n        x -= 1\n    else:\n        pass\n"),
        (Lang::Python3, "def f(x):\n    if (x and\n            x > 1):\n        return [\n            i for i in x\n        ]\n    return \\\n        x\n"),
        (Lang::Rust, "fn f<'a>(s: &'a str) -> char {\n    let c = '}';\n    let r = r#\"(\")\"#;\n    let p = r\"\\\";\n    let v = ['é','('];\n    if let Some(x) = s.chars().next() { return x; }\n    while s.len() == 0 {}\n    /* a /* nested */ ( */\n    c\n}\n"),
        (Lang::Javascript, "function f(x) {\n  const re = /[(]\\//g;\n  const t = `a${ {b: 1}.b }(`;\n  return x.replace(re, '') / 2 / 1;\n}\n"),
        (Lang::Typescript, "function f<T>(x: T[]): T | undefined {\n  // (\n  return x.length > 0 ? x[0] : undefined;\n}\n"),
        (Lang::Java, "class S {\n  String f() {\n    char c = '(';\n    String s = \"\"\"\n      {\n      \"\"\";\n    return s + c;\n  }\n}\n"),
        (Lang::Cpp, "#include <vector>\nint f() {\n  int n = 1'000'000;\n  const char* s = R\"x(\")x\";\n  char c = '{';\n  return n + s[0] + c;\n}\n"),
        (Lang::Go, "func f(s string) rune {\n  r := `(\n{`\n  c := '}'\n  return c + rune(len(r))\n}\n"),
        (Lang::C, "int f(char *s) {\n  if (s[0] == '(') {\n    return 1; // }\n  }\n  return 0;\n}\n"),
    ];
    for (lang, src) in cases {
        expect(*lang, src, &[]);
    }
}

#[test]
fn every_free_starter_is_clean_in_every_language() {
    let catalog = Catalog::embedded();
    let mut checked = 0;
    for lang in Lang::ALL {
        let mut checker = Checker::new(lang);
        for p in catalog.problems() {
            if matches!(p.body, Body::Free { .. }) {
                assert_eq!(checker.check(p.starter(lang)), vec![], "{lang:?} {}", p.slug);
                checked += 1;
            }
        }
    }
    assert!(checked > 1000, "{checked}");
}

#[test]
fn warnings_flag_likely_bugs_without_blocking() {
    for lang in [Lang::Java, Lang::Cpp, Lang::C, Lang::Javascript, Lang::Typescript] {
        let src = "int f(int x) {\n  if (x > 1);\n  return x;\n}\n";
        let d = Checker::new(lang).check(src);
        let semi = src.find(");").unwrap() + 1;
        assert!(
            d.iter().any(|d| d.range == (semi..semi + 1) && d.severity == Severity::Warning),
            "{lang:?} {d:?}"
        );
    }
    let mixed = Checker::new(Lang::Python3).check("def f():\n \treturn 1\n");
    assert_eq!(mixed.len(), 1);
    assert_eq!((mixed[0].range.clone(), mixed[0].severity), (9..11, Severity::Warning));
    assert!(Checker::new(Lang::Python3).check("def f():\n\treturn 1\n").is_empty());
}

#[test]
fn a_missing_closer_replaces_the_unclosed_report_and_cascades_are_dropped() {
    let d = Checker::new(Lang::C).check("int f(int x) {\n  return (x + 1;\n}\n");
    assert_eq!(d.len(), 1, "{d:?}");
    assert_eq!((d[0].range.clone(), d[0].source), (30..30, Source::Syntax));
    let d = Checker::new(Lang::Python3).check("def f(x):\n    if x = 1:\n        pass\n");
    assert_eq!(d.len(), 1, "{d:?}");
    assert_eq!(d[0].source, Source::Lint);
}

/// xorshift64, so failures replay from the printed seed.
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
    "(", ")", "[", "]", "{", "}", "\"", "'", "`", "${", "/*", "*/", "//", "#", "\n", "\t", " ", "\\", "r#\"",
    "R\"x(", "'''", "\"\"\"", "/", "=", ";", ":", "if ", "x", "print ", "1'0", "é", "漢字", "🦀", "\r",
];

#[test]
fn random_edits_never_panic_and_ranges_stay_on_char_boundaries() {
    let catalog = Catalog::embedded();
    let problem = catalog.problems().iter().find(|p| p.slug == "two-sum").unwrap();
    for (seed, lang) in Lang::ALL.into_iter().enumerate() {
        let mut rng = Rng(0x9e37_79b9_7f4a_7c15 ^ (seed as u64 + 1));
        let mut checker = Checker::new(lang);
        let mut text = problem.starter(lang).to_string();
        for step in 0..400 {
            let boundaries: Vec<usize> = (0..=text.len()).filter(|&i| text.is_char_boundary(i)).collect();
            let a = boundaries[rng.below(boundaries.len())];
            if rng.below(3) == 0 && text.len() > 0 {
                let b = boundaries[rng.below(boundaries.len())];
                text.replace_range(a.min(b)..a.max(b), "");
            } else {
                text.insert_str(a, PIECES[rng.below(PIECES.len())]);
            }
            if text.len() > 4000 {
                text.truncate(boundaries.iter().rev().find(|&&b| b <= 2000).copied().unwrap_or(0));
            }
            for d in checker.check(&text) {
                let r = &d.range;
                assert!(
                    r.start <= r.end && r.end <= text.len() && text.is_char_boundary(r.start) && text.is_char_boundary(r.end),
                    "{lang:?} seed {seed} step {step}: {d:?} in {text:?}"
                );
            }
        }
    }
}
