use labradormail::prelude::*;

#[test]
fn root_window_accessors_return_expected_windows() -> std::io::Result<()> {
    let root = RootWindow::new_with_size((80, 24))?;
    let tree = root.tree();
    assert_eq!(tree.get(root.root_id()).window_type, WindowType::Root);
    assert_eq!(
        tree.get(root.help_bar_id()).window_type,
        WindowType::HelpBar
    );
    assert_eq!(
        tree.get(root.all_dialogs_id()).window_type,
        WindowType::AllDialogs
    );
    assert_eq!(
        tree.get(root.message_container_id()).window_type,
        WindowType::Container
    );
    Ok(())
}
