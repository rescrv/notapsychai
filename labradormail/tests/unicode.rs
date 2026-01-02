use gui::mutt_char_width;
use gui::mutt_str_expand_tabs;
use gui::mutt_strnwidth;
use gui::mutt_strwidth;
use gui::mutt_wstr_trunc;
use gui::AttrColor;
use gui::ColorId;
use gui::GuiContext;
use gui::MessageWindow;
use gui::MwChar;

#[test]
fn strnwidth_truncates_mid_utf8() {
    let s = "é"; // two bytes
    assert_eq!(mutt_strnwidth(s, 1), 0);
    assert_eq!(mutt_strnwidth(s, 2), 1);
}

#[test]
fn wstr_trunc_respects_utf8_boundaries() {
    let s = "éx";
    let mut width = 0usize;
    let used = mutt_wstr_trunc(s, 1, 10, Some(&mut width));
    assert_eq!(used, 0);
    assert_eq!(width, 0);

    let used = mutt_wstr_trunc(s, 2, 1, Some(&mut width));
    assert_eq!(used, 2);
    assert_eq!(width, 1);
}

#[test]
fn zero_width_characters_have_zero_width() {
    assert_eq!(mutt_char_width('\u{200D}'), 0); // ZWJ
    assert_eq!(mutt_char_width('\u{FE0F}'), 0); // variation selector-16
}

#[test]
fn combining_mark_width() {
    let s = "a\u{0301}"; // a + combining acute
    assert_eq!(mutt_strwidth(s), 1);
}

#[test]
fn right_to_left_text_width() {
    let s = "مرحبا";
    assert_eq!(mutt_strwidth(s), 5);
}

#[test]
fn emoji_skin_tone_modifier_width() {
    let s = "👍🏻";
    assert_eq!(mutt_strwidth(s), 2);
}

#[test]
fn zwj_sequence_width() {
    let s = "👨‍👩‍👧‍👦";
    assert_eq!(mutt_strwidth(s), 2);
}

#[test]
fn variation_selector_does_not_increase_width() {
    let base = "☺";
    let with_vs = "☺\u{FE0F}";
    assert_eq!(mutt_strwidth(with_vs), mutt_strwidth(base));
}

#[test]
fn zero_width_joiner_in_string() {
    let s = "a\u{200D}b";
    assert_eq!(mutt_strwidth(s), 2);
}

#[test]
fn all_zero_width_types() {
    let chars = ['\u{200B}', '\u{200C}', '\u{200D}', '\u{2060}', '\u{FEFF}'];
    for ch in chars {
        assert_eq!(mutt_char_width(ch), 0);
    }
}

#[test]
fn expand_tabs_with_wide_characters() {
    let s = "好\tX";
    let out = mutt_str_expand_tabs(s, s.len(), 4).unwrap();
    assert_eq!(out, "好  X");
}

#[test]
fn mwchar_measure_variation_selector() {
    let ch = '\u{FE0F}';
    let color = AttrColor::unset();
    let measured = MwChar::measure(ch, color);
    assert_eq!(measured.width, 0);
}

#[test]
fn message_window_calc_rows_with_cjk_wrap() {
    let ctx = GuiContext::new();
    let msg = MessageWindow::new(true);
    msg.set_text(&ctx, "hi好", ColorId::Message);

    let mut data = msg.data().unwrap();
    let rows = data.calc_rows(3);
    assert!(rows > 1);
}

#[test]
fn message_window_wrap_exact_boundary() {
    let ctx = GuiContext::new();
    let msg = MessageWindow::new(true);
    msg.set_text(&ctx, "abcd", ColorId::Message);

    let mut data = msg.data().unwrap();
    let rows = data.calc_rows(4);
    assert_eq!(rows, 1);
}

#[test]
fn message_window_embedded_newlines() {
    let ctx = GuiContext::new();
    let msg = MessageWindow::new(true);
    msg.set_text(&ctx, "a\nbbbb", ColorId::Message);

    let mut data = msg.data().unwrap();
    let rows = data.calc_rows(2);
    assert!(rows >= 2);
}
