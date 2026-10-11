use super::*;
use crate::tui::theme::t;
use ratatui::{backend::TestBackend, buffer::Buffer as Screen, Terminal};
use std::time::Duration;

struct Ed {
    ed: CodeEditor,
    now: Instant,
}

impl Ed {
    fn new(text: &str, lang: Lang) -> Ed {
        Ed { ed: CodeEditor::new(text, lang), now: Instant::now() }
    }

    fn key(&mut self, code: KeyCode, mods: KeyModifiers) -> Effect {
        self.ed.handle_key(KeyEvent::new(code, mods), self.now)
    }

    fn press(&mut self, code: KeyCode) -> Effect {
        self.key(code, KeyModifiers::NONE)
    }

    fn ctrl(&mut self, c: char) -> Effect {
        self.key(KeyCode::Char(c), KeyModifiers::CONTROL)
    }

    fn type_str(&mut self, s: &str) {
        for c in s.chars() {
            self.press(KeyCode::Char(c));
        }
    }

    fn wait(&mut self, ms: u64) {
        self.now += Duration::from_millis(ms);
    }

    fn text(&self) -> &str {
        self.ed.text()
    }

    fn head(&self) -> usize {
        self.ed.cursor.sel.head
    }

    fn render(&self, w: u16, h: u16, show_cursor: bool) -> Screen {
        let mut term = Terminal::new(TestBackend::new(w, h)).unwrap();
        term.draw(|f| self.ed.render(f.area(), f.buffer_mut(), show_cursor)).unwrap();
        term.backend().buffer().clone()
    }
}

fn py(text: &str) -> Ed {
    Ed::new(text, Lang::Python3)
}

fn row(screen: &Screen, y: u16) -> String {
    (0..screen.area.width).map(|x| screen[(x, y)].symbol()).collect()
}

fn cells_with_bg(screen: &Screen, bg: ratatui::style::Color) -> Vec<(u16, u16, String)> {
    let mut found = Vec::new();
    for y in 0..screen.area.height {
        for x in 0..screen.area.width {
            if screen[(x, y)].bg == bg {
                found.push((x, y, screen[(x, y)].symbol().to_string()));
            }
        }
    }
    found
}

#[test]
fn typing_backspace_undo_redo_round_trip() {
    let mut e = py("");
    e.type_str("abc");
    e.wait(1000);
    e.press(KeyCode::Backspace);
    assert_eq!(e.text(), "ab");
    e.ctrl('z');
    assert_eq!(e.text(), "abc");
    e.ctrl('z');
    assert_eq!(e.text(), "");
    assert_eq!(e.head(), 0);
    e.ctrl('y');
    assert_eq!(e.text(), "abc");
    assert_eq!(e.head(), 3);
    e.key(KeyCode::Char('Z'), KeyModifiers::CONTROL | KeyModifiers::SHIFT);
    assert_eq!(e.text(), "ab");
}

#[test]
fn undo_groups_typing_within_300ms_and_splits_after_a_pause() {
    let mut e = py("");
    e.type_str("ab");
    e.wait(200);
    e.type_str("c");
    e.wait(400);
    e.type_str("d");
    e.ctrl('z');
    assert_eq!(e.text(), "abc");
    e.ctrl('z');
    assert_eq!(e.text(), "");
}

#[test]
fn caret_jump_and_newline_break_the_undo_group() {
    let mut e = py("");
    e.type_str("ab");
    e.press(KeyCode::Left);
    e.type_str("X");
    e.ctrl('z');
    assert_eq!(e.text(), "ab");
    e.press(KeyCode::End);
    e.press(KeyCode::Enter);
    e.type_str("c");
    e.ctrl('z');
    assert_eq!(e.text(), "ab\n");
}

#[test]
fn new_edit_after_undo_clears_redo() {
    let mut e = py("");
    e.type_str("a");
    e.ctrl('z');
    e.type_str("b");
    e.ctrl('y');
    assert_eq!(e.text(), "b");
}

#[test]
fn typing_replaces_the_selection_and_undo_restores_it() {
    let mut e = py("hello world");
    e.ctrl('a');
    e.type_str("x");
    assert_eq!(e.text(), "x");
    e.ctrl('z');
    assert_eq!(e.text(), "hello world");
    assert_eq!(e.ed.cursor.sel, Selection { anchor: 0, head: 11 });
}

#[test]
fn shift_selection_then_backspace_deletes_it() {
    let mut e = py("hello");
    e.key(KeyCode::Right, KeyModifiers::SHIFT);
    e.key(KeyCode::Right, KeyModifiers::SHIFT);
    e.press(KeyCode::Backspace);
    assert_eq!(e.text(), "llo");
}

#[test]
fn enter_copies_indent_and_indents_after_an_opener() {
    let mut e = py("    x = 1");
    e.press(KeyCode::End);
    e.press(KeyCode::Enter);
    assert_eq!(e.text(), "    x = 1\n    ");

    let mut e = py("def f():");
    e.press(KeyCode::End);
    e.press(KeyCode::Enter);
    assert_eq!(e.text(), "def f():\n    ");
    assert_eq!(e.head(), e.text().len());
}

#[test]
fn enter_between_braces_opens_an_indented_block() {
    let mut e = Ed::new("fn f() ", Lang::Rust);
    e.press(KeyCode::End);
    e.type_str("{");
    assert_eq!(e.text(), "fn f() {}");
    e.press(KeyCode::Enter);
    assert_eq!(e.text(), "fn f() {\n    \n}");
    assert_eq!(e.head(), "fn f() {\n    ".len());
}

#[test]
fn go_indents_with_tabs() {
    let mut e = Ed::new("func f() {}", Lang::Go);
    e.press(KeyCode::End);
    e.press(KeyCode::Left);
    e.press(KeyCode::Enter);
    assert_eq!(e.text(), "func f() {\n\t\n}");
    e.press(KeyCode::Tab);
    assert_eq!(e.text(), "func f() {\n\t\t\n}");
}

#[test]
fn brackets_and_quotes_auto_close_and_step_over() {
    let mut e = py("");
    e.type_str("f(");
    assert_eq!((e.text(), e.head()), ("f()", 2));
    e.type_str("\"a");
    assert_eq!(e.text(), "f(\"a\")");
    e.type_str("\")");
    assert_eq!((e.text(), e.head()), ("f(\"a\")", 6));
}

#[test]
fn apostrophe_after_a_letter_does_not_pair() {
    let mut e = py("");
    e.type_str("don't");
    assert_eq!(e.text(), "don't");
}

#[test]
fn rust_lifetimes_do_not_pair() {
    let mut e = Ed::new("", Lang::Rust);
    e.type_str("&'a");
    assert_eq!(e.text(), "&'a");
}

#[test]
fn opener_before_a_word_does_not_pair() {
    let mut e = py("x");
    e.type_str("(");
    assert_eq!(e.text(), "(x");
}

#[test]
fn backspace_inside_an_empty_pair_removes_both() {
    let mut e = py("");
    e.type_str("[");
    e.press(KeyCode::Backspace);
    assert_eq!(e.text(), "");
}

#[test]
fn backspace_in_leading_spaces_removes_to_the_previous_tab_stop() {
    let mut e = py("      x");
    e.press(KeyCode::Right);
    for _ in 0..5 {
        e.press(KeyCode::Right);
    }
    e.press(KeyCode::Backspace);
    assert_eq!(e.text(), "    x");
    e.press(KeyCode::Backspace);
    assert_eq!(e.text(), "x");
}

#[test]
fn backspace_and_delete_remove_whole_chars() {
    let mut e = py("aé漢");
    e.press(KeyCode::End);
    e.press(KeyCode::Backspace);
    assert_eq!(e.text(), "aé");
    e.press(KeyCode::Home);
    e.press(KeyCode::Right);
    e.press(KeyCode::Delete);
    assert_eq!(e.text(), "a");
}

#[test]
fn ctrl_and_alt_arrows_jump_words() {
    let mut e = py("foo.bar baz");
    e.key(KeyCode::Right, KeyModifiers::CONTROL);
    assert_eq!(e.head(), 3);
    e.key(KeyCode::Right, KeyModifiers::ALT);
    assert_eq!(e.head(), 4);
    e.key(KeyCode::Char('f'), KeyModifiers::ALT);
    assert_eq!(e.head(), 7);
    e.key(KeyCode::Left, KeyModifiers::CONTROL | KeyModifiers::SHIFT);
    assert_eq!(e.ed.cursor.sel, Selection { anchor: 7, head: 4 });
}

#[test]
fn tab_and_shift_tab_indent_every_selected_line_as_one_step() {
    let mut e = py("a\nb\nc");
    e.key(KeyCode::Down, KeyModifiers::SHIFT);
    e.key(KeyCode::Right, KeyModifiers::SHIFT);
    e.press(KeyCode::Tab);
    assert_eq!(e.text(), "    a\n    b\nc");
    assert_eq!(e.ed.cursor.sel, Selection { anchor: 4, head: 11 });
    e.press(KeyCode::BackTab);
    assert_eq!(e.text(), "a\nb\nc");
    e.press(KeyCode::Tab);
    e.ctrl('z');
    assert_eq!(e.text(), "a\nb\nc");
}

#[test]
fn tab_with_no_selection_pads_to_the_next_stop() {
    let mut e = py("ab");
    e.press(KeyCode::End);
    e.press(KeyCode::Tab);
    assert_eq!(e.text(), "ab  ");
}

#[test]
fn copy_paste_and_cut_use_the_internal_register() {
    let mut e = py("one two");
    e.key(KeyCode::Right, KeyModifiers::CONTROL | KeyModifiers::SHIFT);
    assert_eq!(e.ctrl('c'), Effect::Copy("one".into()));
    e.press(KeyCode::End);
    e.type_str(" ");
    e.ctrl('v');
    assert_eq!(e.text(), "one two one");
    e.ctrl('a');
    assert_eq!(e.ctrl('x'), Effect::Copy("one two one".into()));
    assert_eq!(e.text(), "");
}

#[test]
fn cut_with_no_selection_takes_the_line_and_paste_puts_it_above() {
    let mut e = py("a\nb\nc");
    e.press(KeyCode::Down);
    assert_eq!(e.ctrl('x'), Effect::Copy("b\n".into()));
    assert_eq!(e.text(), "a\nc");
    e.ctrl('v');
    assert_eq!(e.text(), "a\nb\nc");
}

#[test]
fn bracketed_paste_inserts_text_with_unix_newlines() {
    let mut e = py("");
    e.ed.paste("x\r\ny", e.now);
    assert_eq!(e.text(), "x\ny");
    assert_eq!(e.head(), 3);
}

#[test]
fn ctrl_s_asks_the_caller_to_save() {
    let mut e = py("");
    assert_eq!(e.ctrl('s'), Effect::Save);
}

#[test]
fn dirty_tracks_the_saved_text_through_undo() {
    let mut e = py("x");
    e.type_str("a");
    assert!(e.ed.is_dirty());
    e.ctrl('z');
    assert!(!e.ed.is_dirty());
    e.type_str("b");
    e.ed.mark_saved();
    assert!(!e.ed.is_dirty());
}

#[test]
fn renders_selection_and_cursor_cells() {
    let mut e = py("hello");
    e.key(KeyCode::Right, KeyModifiers::SHIFT);
    e.key(KeyCode::Right, KeyModifiers::SHIFT);
    let screen = e.render(20, 3, true);
    assert_eq!(row(&screen, 0), "1 │ hello           ");
    let selected: Vec<_> = cells_with_bg(&screen, t::BORDER);
    assert_eq!(selected, vec![(4, 0, "h".into()), (5, 0, "e".into())]);
    assert_eq!(cells_with_bg(&screen, t::CYAN), vec![(6, 0, "l".into())]);

    let hidden = e.render(20, 3, false);
    assert!(cells_with_bg(&hidden, t::CYAN).is_empty());
}

#[test]
fn cursor_at_line_end_and_tabs_render_as_cells() {
    let mut e = Ed::new("\tx", Lang::Go);
    e.press(KeyCode::End);
    let screen = e.render(20, 2, true);
    assert_eq!(row(&screen, 0), "1 │     x           ");
    assert_eq!(cells_with_bg(&screen, t::CYAN), vec![(9, 0, " ".into())]);
}

#[test]
fn syntax_colors_come_from_the_highlighter() {
    let e = py("def f(): return 42");
    let screen = e.render(30, 1, false);
    assert_eq!(screen[(4, 0)].fg, t::PURPLE);
    assert_eq!(screen[(20, 0)].fg, t::YELLOW);
}

#[test]
fn vertical_scroll_follows_the_cursor() {
    let text: String = (1..=50).map(|i| format!("line{i}\n")).collect();
    let mut e = py(&text);
    e.render(20, 5, true);
    for _ in 0..20 {
        e.press(KeyCode::Down);
    }
    let screen = e.render(20, 5, true);
    assert_eq!(row(&screen, 4).trim_end(), "21 │ line21");
    assert_eq!(cells_with_bg(&screen, t::CYAN), vec![(5, 4, "l".into())]);
    e.press(KeyCode::PageUp);
    let screen = e.render(20, 5, true);
    assert_eq!(row(&screen, 0).trim_end(), "17 │ line17");
}

#[test]
fn horizontal_scroll_keeps_the_cursor_visible() {
    let long = format!("{}END", "x".repeat(40));
    let mut e = py(&long);
    e.render(20, 2, true);
    e.press(KeyCode::End);
    let screen = e.render(20, 2, true);
    assert_eq!(row(&screen, 0), "1 │ xxxxxxxxxxxxEND ");
    assert_eq!(cells_with_bg(&screen, t::CYAN), vec![(19, 0, " ".into())]);
    e.press(KeyCode::Home);
    let screen = e.render(20, 2, true);
    assert!(row(&screen, 0).starts_with("1 │ xxx"));
    assert_eq!(cells_with_bg(&screen, t::CYAN), vec![(4, 0, "x".into())]);
}

#[test]
fn scroll_by_moves_the_view_without_moving_the_cursor() {
    let text: String = (1..=30).map(|i| format!("{i}\n")).collect();
    let mut e = py(&text);
    e.ed.scroll_by(10);
    let screen = e.render(12, 3, true);
    assert_eq!(row(&screen, 0).trim_end(), "11 │ 11");
    assert_eq!(e.head(), 0);
    e.ed.scroll_by(-100);
    assert_eq!(row(&e.render(12, 3, true), 0).trim_end(), " 1 │ 1");
}

#[test]
fn tiny_areas_never_panic() {
    let mut e = py("漢字\tx\n\n   é");
    e.press(KeyCode::Down);
    e.press(KeyCode::Down);
    e.press(KeyCode::End);
    for w in 0..=12 {
        for h in 0..=14 {
            e.render(w, h, true);
            e.press(KeyCode::Left);
            e.press(KeyCode::Up);
            e.press(KeyCode::PageDown);
        }
    }
}

#[test]
fn copy_and_cut_on_a_blank_line_do_nothing() {
    let mut e = py("a\n\n    \nb");
    e.press(KeyCode::Down);
    assert_eq!(e.ctrl('c'), Effect::None);
    assert_eq!(e.ctrl('x'), Effect::None);
    e.press(KeyCode::Down);
    assert_eq!(e.ctrl('x'), Effect::None);
    assert_eq!(e.text(), "a\n\n    \nb");

    let mut e = py("a");
    e.press(KeyCode::End);
    e.press(KeyCode::Enter);
    assert_eq!(e.ctrl('x'), Effect::None);
    e.ctrl('z');
    assert_eq!(e.text(), "a");
}

#[test]
fn ctrl_alt_chords_type_text_and_alt_letters_insert_except_word_jumps() {
    let mut e = py("");
    e.key(KeyCode::Char('@'), KeyModifiers::CONTROL | KeyModifiers::ALT);
    assert_eq!(e.key(KeyCode::Char('s'), KeyModifiers::CONTROL | KeyModifiers::ALT), Effect::None);
    e.key(KeyCode::Char('x'), KeyModifiers::ALT);
    assert_eq!(e.text(), "@sx");
    e.key(KeyCode::Char('b'), KeyModifiers::ALT);
    assert_eq!((e.text(), e.head()), ("@sx", 1));
}

#[test]
fn tab_over_lines_leaves_empty_lines_empty() {
    let mut e = py("a\n\nb");
    e.ctrl('a');
    e.press(KeyCode::Tab);
    assert_eq!(e.text(), "    a\n\n    b");
    e.press(KeyCode::BackTab);
    assert_eq!(e.text(), "a\n\nb");
}

#[test]
fn contents_keep_the_files_crlf_line_endings() {
    let mut e = py("a\r\nb");
    e.press(KeyCode::Down);
    e.press(KeyCode::End);
    e.press(KeyCode::Enter);
    e.type_str("c");
    assert_eq!(e.text(), "a\nb\nc");
    assert_eq!(e.ed.contents(), "a\r\nb\r\nc");
}
