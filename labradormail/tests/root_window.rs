use labradormail::RootWindow;
use labradormail::WindowType;

#[test]
fn root_window_accessors_return_expected_windows() {
    let root = RootWindow::new_with_size((80, 24)).unwrap();
    assert_eq!(root.root().borrow().window_type, WindowType::Root);
    assert_eq!(root.help_bar().borrow().window_type, WindowType::HelpBar);
    assert_eq!(
        root.all_dialogs().borrow().window_type,
        WindowType::AllDialogs
    );
    assert_eq!(
        root.message_container().borrow().window_type,
        WindowType::Container
    );
}
