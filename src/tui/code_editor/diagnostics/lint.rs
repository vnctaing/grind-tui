use super::scan::is_ident_byte;
use super::{Diagnostic, Source};

pub fn empty_if(blank: &[u8]) -> Vec<Diagnostic> {
    let mut out = Vec::new();
    for at in words(blank, b"if") {
        let open = skip_space(blank, at + 2);
        if blank.get(open) != Some(&b'(') {
            continue;
        }
        let line_end = blank[open..].iter().position(|&c| c == b'\n').map_or(blank.len(), |p| open + p);
        let Some(close) = closing_paren(&blank[..line_end], open) else { continue };
        let semi = skip_space(blank, close + 1);
        if blank.get(semi) != Some(&b';') {
            continue;
        }
        let next = semi + 1 + blank[semi + 1..].iter().take_while(|c| c.is_ascii_whitespace()).count();
        if words(&blank[next..], b"else").next() != Some(0) {
            out.push(Diagnostic::warning(semi..semi + 1, Source::Lint, "Empty statement after 'if'"));
        }
    }
    out
}

fn closing_paren(code: &[u8], open: usize) -> Option<usize> {
    let mut depth = 0usize;
    for (i, &c) in code.iter().enumerate().skip(open) {
        match c {
            b'(' | b'[' | b'{' => depth += 1,
            b')' | b']' | b'}' => {
                depth = depth.checked_sub(1)?;
                if depth == 0 {
                    return (c == b')').then_some(i);
                }
            }
            _ => {}
        }
    }
    None
}

fn words<'a>(code: &'a [u8], word: &'a [u8]) -> impl Iterator<Item = usize> + 'a {
    code.windows(word.len()).enumerate().filter_map(move |(i, w)| {
        let before = i == 0 || !is_ident_byte(code[i - 1]);
        let after = code.get(i + word.len()).is_none_or(|&c| !is_ident_byte(c));
        (w == word && before && after).then_some(i)
    })
}

fn skip_space(code: &[u8], from: usize) -> usize {
    from + code[from.min(code.len())..].iter().take_while(|c| matches!(c, b' ' | b'\t')).count()
}
