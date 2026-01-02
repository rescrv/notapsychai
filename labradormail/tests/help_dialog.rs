use gui::HelpData;
use gui::HelpDialog;
use gui::HelpItem;
use gui::WindowType;

#[test]
fn help_dialog_window_tree() {
    let data = HelpData::from_items(vec![HelpItem::new("q", "Quit")]);
    let dialog = HelpDialog::new(&data);
    let win = dialog.window();
    let borrowed = win.borrow();
    assert_eq!(borrowed.window_type, WindowType::DlgHelp);
    assert_eq!(borrowed.children.len(), 2);
}
