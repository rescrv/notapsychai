use labradormail::prelude::*;

#[test]
fn agent_dialog_can_be_opened_via_op_agent() {
    let mut root_win = RootWindow::new_with_size((80, 24)).unwrap();
    let all_dialogs = root_win.all_dialogs_id();

    // Initially just main_layout in the stack
    let initial_count = root_win.tree().get(all_dialogs).children.len();
    assert_eq!(initial_count, 1);

    // Create and push an agent dialog
    let dialog = AgentDialog::new(root_win.tree_mut());
    root_win
        .tree_mut()
        .stack_push(all_dialogs, dialog.window_id());
    root_win.tree_mut().set_focus(dialog.window_id());

    // Now should have 2 dialogs in stack
    let new_count = root_win.tree().get(all_dialogs).children.len();
    assert_eq!(new_count, 2);

    // Top should be agent dialog
    let top = root_win.tree().stack_top(all_dialogs).unwrap();
    assert_eq!(root_win.tree().get(top).window_type, WindowType::DlgAgent);
}

#[test]
fn dialog_stack_three_deep_lifecycle() {
    let mut tree = WindowTree::new();
    let all_dialogs = tree.add_window(
        WindowType::AllDialogs,
        WindowOrientation::Vertical,
        WindowSize::Maximise,
        0,
        0,
    );

    let dialog1 = tree.add_dialog(WindowType::DlgIndex);
    let dialog2 = tree.add_dialog(WindowType::DlgHelp);
    let dialog3 = tree.add_dialog(WindowType::DlgIndex);

    tree.stack_push(all_dialogs, dialog1);
    tree.stack_push(all_dialogs, dialog2);
    tree.stack_push(all_dialogs, dialog3);

    assert_eq!(tree.get(all_dialogs).children.len(), 3);
    assert!(tree.get(dialog3).state.visible);
    assert!(!tree.get(dialog1).state.visible);

    let top = tree.stack_top(all_dialogs).unwrap();
    assert!(tree.same_window(top, dialog3));

    let popped = tree.stack_pop(all_dialogs);
    assert_eq!(popped, Some(dialog3));
    assert!(tree.get(dialog2).state.visible);
    assert!(!tree.get(dialog3).state.visible);
}

#[test]
fn focus_change_updates_help_bar_via_root() {
    let mut root_win = RootWindow::new_with_size((80, 24)).unwrap();
    let help_bar = root_win.help_bar_id();

    root_win.tree_mut().get_mut(help_bar).clear_actions();

    let dialog = root_win.tree_mut().add_dialog(WindowType::DlgHelp);
    root_win.dialog_push(dialog);
    root_win.tree_mut().set_focus(dialog);

    let actions = root_win.tree().get(help_bar).action_flags();
    assert!(actions.contains(WindowActionFlags::RECALC));
    assert!(actions.contains(WindowActionFlags::REPAINT));
}

#[test]
fn reflow_after_visibility_toggle() {
    let mut tree = WindowTree::new();
    let root = tree.add_window(
        WindowType::Root,
        WindowOrientation::Vertical,
        WindowSize::Fixed,
        10,
        10,
    );
    let child1 = tree.add_window(
        WindowType::Container,
        WindowOrientation::Vertical,
        WindowSize::Maximise,
        0,
        0,
    );
    let child2 = tree.add_window(
        WindowType::Container,
        WindowOrientation::Vertical,
        WindowSize::Maximise,
        0,
        0,
    );

    tree.add_child(root, child1);
    tree.add_child(root, child2);

    window_reflow(&mut tree, root);
    assert_eq!(tree.get(child1).state.rect.size.rows, 5);

    tree.set_visible(child2, false);
    window_reflow(&mut tree, root);
    assert_eq!(tree.get(child1).state.rect.size.rows, 10);
}

#[test]
fn window_tree_traversal_after_remove() {
    let mut tree = WindowTree::new();
    let root = tree.add_window(
        WindowType::Root,
        WindowOrientation::Vertical,
        WindowSize::Fixed,
        80,
        24,
    );
    let message = tree.add_window(
        WindowType::Message,
        WindowOrientation::Vertical,
        WindowSize::Fixed,
        80,
        1,
    );
    let container = tree.add_window(
        WindowType::Container,
        WindowOrientation::Vertical,
        WindowSize::Maximise,
        0,
        0,
    );

    tree.add_child(root, container);
    tree.add_child(container, message);

    assert!(tree.find_child(root, WindowType::Message).is_some());

    tree.remove_child(container, message);
    assert!(tree.find_child(root, WindowType::Message).is_none());
}

#[test]
fn resize_propagates_through_nested_windows() {
    let mut root_win = RootWindow::new_with_size((80, 24)).unwrap();
    let all_dialogs = root_win.all_dialogs_id();

    let dialog = root_win.tree_mut().add_dialog(WindowType::DlgIndex);
    let content = root_win.tree_mut().add_window(
        WindowType::Container,
        WindowOrientation::Vertical,
        WindowSize::Maximise,
        0,
        0,
    );
    root_win.tree_mut().add_child(dialog, content);
    root_win.dialog_push(dialog);

    root_win.set_size(100, 30);

    let tree = root_win.tree();
    let all_dialogs_rows = tree.get(all_dialogs).state.rect.size.rows;
    let dialog_rows = tree.get(dialog).state.rect.size.rows;
    let content_rows = tree.get(content).state.rect.size.rows;

    assert_eq!(all_dialogs_rows, dialog_rows);
    assert_eq!(dialog_rows, content_rows);
}

#[test]
fn color_config_change_affects_multiple_widgets() {
    let mut ctx = GuiContext::new();
    ctx.simple_color_set(
        ColorId::Status,
        AttrColor::new(
            Some(crossterm::style::Color::White),
            Some(crossterm::style::Color::Blue),
            &[crossterm::style::Attribute::Bold],
        ),
    );
    ctx.simple_color_set(
        ColorId::Normal,
        AttrColor::new(Some(crossterm::style::Color::Grey), None, &[]),
    );

    let mut tree = WindowTree::new();
    let root = tree.add_window(
        WindowType::Root,
        WindowOrientation::Vertical,
        WindowSize::Fixed,
        20,
        4,
    );
    let help_bar = HelpBar::new(&mut tree);
    let status_bar = StatusBar::new(&mut tree);

    let help_data = HelpData::from_items(vec![HelpItem::new("q", "Quit")]);
    let dialog = tree.add_dialog(WindowType::DlgIndex);
    tree.get_mut(dialog).help_data = Some(help_data);

    tree.add_child(root, help_bar.window_id());
    tree.add_child(root, dialog);
    tree.add_child(root, status_bar.window_id());

    tree.set_focus(dialog);
    window_reflow(&mut tree, root);

    assert!(ctx.simple_color_get(ColorId::Status).is_set);
    assert!(ctx.simple_color_get(ColorId::Normal).is_set);

    tree.get_mut(help_bar.window_id()).mark_repaint();
    tree.get_mut(status_bar.window_id()).mark_repaint();
}
