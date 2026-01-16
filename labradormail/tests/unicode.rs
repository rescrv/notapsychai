use labradormail::prelude::*;

#[test]
fn wstr_trunc_respects_utf8_boundaries() {
    let s = "éx";
    let trunc = truncate_str_width(s, 1, 10);
    assert_eq!(trunc.bytes, 0);
    assert_eq!(trunc.width, 0);

    let trunc = truncate_str_width(s, 2, 1);
    assert_eq!(trunc.bytes, 2);
    assert_eq!(trunc.width, 1);
}

#[test]
fn zero_width_characters_have_zero_width() {
    assert_eq!(char_width('\u{200D}'), 0); // ZWJ
    assert_eq!(char_width('\u{FE0F}'), 0); // variation selector-16
}

#[test]
fn all_zero_width_types() {
    let chars = ['\u{200B}', '\u{200C}', '\u{200D}', '\u{2060}', '\u{FEFF}'];
    for ch in chars {
        assert_eq!(char_width(ch), 0);
    }
}

#[test]
fn expand_tabs_with_wide_characters() {
    let s = "好\tX";
    let out = expand_tabs(s, s.len(), 4).unwrap();
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
    let mut tree = WindowTree::new();
    let msg = MessageWindow::new(&mut tree, true);
    msg.set_text(&mut tree, &ctx, "hi好", ColorId::Message);

    let mut data = msg.data(&tree).unwrap();
    let rows = data.calc_rows(3);
    assert!(rows > 1);
}

#[test]
fn message_window_wrap_exact_boundary() {
    let ctx = GuiContext::new();
    let mut tree = WindowTree::new();
    let msg = MessageWindow::new(&mut tree, true);
    msg.set_text(&mut tree, &ctx, "abcd", ColorId::Message);

    let mut data = msg.data(&tree).unwrap();
    let rows = data.calc_rows(4);
    assert_eq!(rows, 1);
}

#[test]
fn message_window_embedded_newlines() {
    let ctx = GuiContext::new();
    let mut tree = WindowTree::new();
    let msg = MessageWindow::new(&mut tree, true);
    msg.set_text(&mut tree, &ctx, "a\nbbbb", ColorId::Message);

    let mut data = msg.data(&tree).unwrap();
    let rows = data.calc_rows(2);
    assert!(rows >= 2);
}
