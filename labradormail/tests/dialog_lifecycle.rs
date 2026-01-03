use labradormail::simple_color_apply_config;
use labradormail::window_reflow;
use labradormail::AllDialogsWindow;
use labradormail::ColorConfigEntry;
use labradormail::ColorId;
use labradormail::Dialog;
use labradormail::GuiContext;
use labradormail::HelpBar;
use labradormail::HelpData;
use labradormail::HelpItem;
use labradormail::MuttWindow;
use labradormail::RootWindow;
use labradormail::StatusBar;
use labradormail::WindowActionFlags;
use labradormail::WindowOrientation;
use labradormail::WindowSize;
use labradormail::WindowType;

#[test]
fn dialog_stack_three_deep_lifecycle() {
    let all_dialogs = AllDialogsWindow::new();

    let dialog1 = Dialog::new(WindowType::DlgIndex);
    let dialog2 = Dialog::new(WindowType::DlgHelp);
    let dialog3 = Dialog::new(WindowType::DlgCompose);

    all_dialogs.push(dialog1.window().clone());
    all_dialogs.push(dialog2.window().clone());
    all_dialogs.push(dialog3.window().clone());

    assert_eq!(all_dialogs.len(), 3);
    assert!(dialog3.window().borrow().state.visible);
    assert!(!dialog1.window().borrow().state.visible);

    let top = all_dialogs.top().unwrap();
    assert!(MuttWindow::same_window(&top, dialog3.window()));

    let popped = all_dialogs.pop();
    assert!(popped.is_some());
    assert!(dialog2.window().borrow().state.visible);
    assert!(!dialog3.window().borrow().state.visible);
}

#[test]
fn focus_change_updates_help_bar_via_root() {
    let root_win = RootWindow::new_with_size((80, 24)).unwrap();
    let all_dialogs = root_win.all_dialogs();
    let help_bar = root_win.help_bar();

    help_bar.borrow_mut().actions = WindowActionFlags::NONE;

    let dialog = Dialog::new(WindowType::DlgHelp);
    MuttWindow::add_child(all_dialogs, dialog.window().clone());
    MuttWindow::set_focus(dialog.window());

    let actions = help_bar.borrow().actions;
    assert!(actions.contains(WindowActionFlags::RECALC));
    assert!(actions.contains(WindowActionFlags::REPAINT));
}

#[test]
fn reflow_after_visibility_toggle() {
    let root = MuttWindow::new(
        WindowType::Root,
        WindowOrientation::Vertical,
        WindowSize::Fixed,
        10,
        10,
    );
    root.borrow_mut().state.cols = 10;
    root.borrow_mut().state.rows = 10;

    let child1 = MuttWindow::new(
        WindowType::Container,
        WindowOrientation::Vertical,
        WindowSize::Maximise,
        0,
        0,
    );
    let child2 = MuttWindow::new(
        WindowType::Container,
        WindowOrientation::Vertical,
        WindowSize::Maximise,
        0,
        0,
    );

    MuttWindow::add_child(&root, child1.clone());
    MuttWindow::add_child(&root, child2.clone());

    window_reflow(&root);
    assert_eq!(child1.borrow().state.rows, 5);

    child2.borrow_mut().state.visible = false;
    window_reflow(&root);
    assert_eq!(child1.borrow().state.rows, 10);
}

#[test]
fn window_tree_traversal_after_remove() {
    let root = MuttWindow::new(
        WindowType::Root,
        WindowOrientation::Vertical,
        WindowSize::Fixed,
        80,
        24,
    );
    let message = MuttWindow::new(
        WindowType::Message,
        WindowOrientation::Vertical,
        WindowSize::Fixed,
        80,
        1,
    );
    let container = MuttWindow::new(
        WindowType::Container,
        WindowOrientation::Vertical,
        WindowSize::Maximise,
        0,
        0,
    );

    MuttWindow::add_child(&root, container.clone());
    MuttWindow::add_child(&container, message.clone());

    assert!(MuttWindow::find_child(&root, WindowType::Message).is_some());

    container.borrow_mut().remove_child(&message);
    assert!(MuttWindow::find_child(&root, WindowType::Message).is_none());
}

#[test]
fn resize_propagates_through_nested_windows() {
    let mut root_win = RootWindow::new_with_size((80, 24)).unwrap();
    let all_dialogs = root_win.all_dialogs().clone();

    let dialog = Dialog::new(WindowType::DlgIndex);
    let content = MuttWindow::new(
        WindowType::Container,
        WindowOrientation::Vertical,
        WindowSize::Maximise,
        0,
        0,
    );
    MuttWindow::add_child(dialog.window(), content.clone());
    MuttWindow::add_child(&all_dialogs, dialog.window().clone());

    root_win.set_size(100, 30);

    let all_dialogs_rows = all_dialogs.borrow().state.rows;
    let dialog_rows = dialog.window().borrow().state.rows;
    let content_rows = content.borrow().state.rows;

    assert_eq!(all_dialogs_rows, dialog_rows);
    assert_eq!(dialog_rows, content_rows);
}

#[test]
fn color_config_change_affects_multiple_widgets() {
    let mut ctx = GuiContext::new();
    simple_color_apply_config(
        &mut ctx,
        &[
            ColorConfigEntry {
                cid: ColorId::Status,
                fg: Some(crossterm::style::Color::White),
                bg: Some(crossterm::style::Color::Blue),
                attrs: vec![crossterm::style::Attribute::Bold],
            },
            ColorConfigEntry {
                cid: ColorId::Normal,
                fg: Some(crossterm::style::Color::Grey),
                bg: None,
                attrs: Vec::new(),
            },
        ],
    );

    let root = MuttWindow::new(
        WindowType::Root,
        WindowOrientation::Vertical,
        WindowSize::Fixed,
        20,
        4,
    );
    root.borrow_mut().state.cols = 20;
    root.borrow_mut().state.rows = 4;

    let help_bar = HelpBar::new();
    let status_bar = StatusBar::new();

    let help_data = HelpData::from_items(vec![HelpItem::new("q", "Quit")]);
    let dialog = Dialog::new(WindowType::DlgIndex);
    dialog.window().borrow_mut().help_data = Some(std::rc::Rc::new(help_data));

    MuttWindow::add_child(&root, help_bar.window().clone());
    MuttWindow::add_child(&root, dialog.window().clone());
    MuttWindow::add_child(&root, status_bar.window().clone());

    MuttWindow::set_focus(dialog.window());
    window_reflow(&root);

    assert!(ctx.simple_color_get(ColorId::Status).is_set);
    assert!(ctx.simple_color_get(ColorId::Normal).is_set);

    help_bar.window().borrow_mut().actions |= WindowActionFlags::REPAINT;
    status_bar.window().borrow_mut().actions |= WindowActionFlags::REPAINT;
}
