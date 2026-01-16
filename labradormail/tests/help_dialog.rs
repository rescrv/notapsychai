use labradormail::prelude::*;

#[test]
fn help_dialog_window_tree() {
    let mut tree = WindowTree::new();
    let data = HelpData::from_items(vec![HelpItem::new("q", "Quit")]);
    let dialog = HelpDialog::new(&mut tree, &data);
    let win = tree.get(dialog.window_id());
    assert_eq!(win.window_type, WindowType::DlgHelp);
    assert_eq!(win.children.len(), 2);
}
