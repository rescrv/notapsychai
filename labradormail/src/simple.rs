//! Simple dialog helper.
//!
//! Provides a convenient way to create dialogs with a content area and status bar.

use std::cell::RefCell;
use std::rc::Rc;

use crate::dialog::Dialog;
use crate::help_data::HelpData;
use crate::sbar::StatusBar;
use crate::window::window_status_on_top;
use crate::window::MuttWindow;
use crate::window::WindowOrientation;
use crate::window::WindowSize;
use crate::window::WindowType;

/// Result of creating a simple dialog.
pub struct SimpleDialogWindows {
    /// Main dialog window.
    pub dlg: Rc<RefCell<MuttWindow>>,
    /// Status bar window.
    pub sbar: Rc<RefCell<MuttWindow>>,
}

/// Configuration options for simple dialogs.
#[derive(Debug, Clone, Copy, Default)]
pub struct SimpleDialogConfig {
    /// Whether the status bar should appear at the top.
    pub status_on_top: bool,
}

/// Simple dialog builder.
///
/// Creates a dialog with a content area and status bar.
pub struct SimpleDialog;

impl SimpleDialog {
    /// Creates a new simple dialog.
    ///
    /// The dialog contains:
    /// - A content container that takes most of the space
    /// - A status bar at the bottom
    ///
    /// The help data is stored on the dialog window for the help bar to display.
    pub fn create(window_type: WindowType, help_data: Option<Rc<HelpData>>) -> SimpleDialogWindows {
        Self::create_with_config(window_type, help_data, SimpleDialogConfig::default())
    }

    /// Creates a new simple dialog with explicit configuration.
    ///
    /// The help data is stored on the dialog window for the help bar to display.
    pub fn create_with_config(
        window_type: WindowType,
        help_data: Option<Rc<HelpData>>,
        config: SimpleDialogConfig,
    ) -> SimpleDialogWindows {
        let dialog = Dialog::new(window_type);
        {
            let mut borrowed = dialog.window().borrow_mut();
            borrowed.help_data = help_data;
        }

        // Create a container for content
        let content = MuttWindow::new(
            WindowType::Container,
            WindowOrientation::Vertical,
            WindowSize::Maximise,
            0,
            0,
        );

        // Create the status bar
        let sbar = StatusBar::new();

        // Add children to dialog
        if config.status_on_top {
            MuttWindow::add_child(dialog.window(), Rc::clone(sbar.window()));
            MuttWindow::add_child(dialog.window(), Rc::clone(&content));
        } else {
            MuttWindow::add_child(dialog.window(), Rc::clone(&content));
            MuttWindow::add_child(dialog.window(), Rc::clone(sbar.window()));
        }

        SimpleDialogWindows {
            dlg: Rc::clone(dialog.window()),
            sbar: Rc::clone(sbar.window()),
        }
    }

    /// Creates a simple dialog with a custom content window.
    ///
    /// The help data is stored on the dialog window for the help bar to display.
    pub fn with_content(
        window_type: WindowType,
        help_data: Option<Rc<HelpData>>,
        content: Rc<RefCell<MuttWindow>>,
    ) -> SimpleDialogWindows {
        Self::with_content_and_config(
            window_type,
            help_data,
            content,
            SimpleDialogConfig::default(),
        )
    }

    /// Creates a simple dialog with a custom content window and configuration.
    ///
    /// The help data is stored on the dialog window for the help bar to display.
    pub fn with_content_and_config(
        window_type: WindowType,
        help_data: Option<Rc<HelpData>>,
        content: Rc<RefCell<MuttWindow>>,
        config: SimpleDialogConfig,
    ) -> SimpleDialogWindows {
        let dialog = Dialog::new(window_type);
        {
            let mut borrowed = dialog.window().borrow_mut();
            borrowed.help_data = help_data;
        }

        // Create the status bar
        let sbar = StatusBar::new();

        // Add children to dialog
        if config.status_on_top {
            MuttWindow::add_child(dialog.window(), Rc::clone(sbar.window()));
            MuttWindow::add_child(dialog.window(), content);
        } else {
            MuttWindow::add_child(dialog.window(), content);
            MuttWindow::add_child(dialog.window(), Rc::clone(sbar.window()));
        }

        SimpleDialogWindows {
            dlg: Rc::clone(dialog.window()),
            sbar: Rc::clone(sbar.window()),
        }
    }

    /// Applies configuration changes to an existing dialog.
    pub fn apply_config(dialog: &Rc<RefCell<MuttWindow>>, config: SimpleDialogConfig) -> bool {
        window_status_on_top(dialog, config.status_on_top)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::window_reflow;

    #[test]
    fn simple_dialog_create() {
        let simple = SimpleDialog::create(WindowType::DlgIndex, None);

        assert_eq!(simple.dlg.borrow().window_type, WindowType::DlgIndex);
        assert_eq!(simple.dlg.borrow().children.len(), 2);

        // Status bar should be the second child
        assert_eq!(simple.sbar.borrow().window_type, WindowType::StatusBar);
    }

    #[test]
    fn simple_dialog_layout() {
        let simple = SimpleDialog::create(WindowType::DlgIndex, None);

        // Set up sizes for testing
        simple.dlg.borrow_mut().state.cols = 80;
        simple.dlg.borrow_mut().state.rows = 24;

        window_reflow(&simple.dlg);

        // Status bar should be 1 row at the bottom
        assert_eq!(simple.sbar.borrow().state.rows, 1);
        assert_eq!(simple.sbar.borrow().state.row_offset, 23);

        // Content should take remaining space
        let content = &simple.dlg.borrow().children[0];
        assert_eq!(content.borrow().state.rows, 23);
        assert_eq!(content.borrow().state.row_offset, 0);
    }

    #[test]
    fn simple_dialog_layout_status_on_top() {
        let simple = SimpleDialog::create_with_config(
            WindowType::DlgIndex,
            None,
            SimpleDialogConfig {
                status_on_top: true,
            },
        );

        simple.dlg.borrow_mut().state.cols = 80;
        simple.dlg.borrow_mut().state.rows = 24;

        window_reflow(&simple.dlg);

        // Status bar should be 1 row at the top
        assert_eq!(simple.sbar.borrow().state.rows, 1);
        assert_eq!(simple.sbar.borrow().state.row_offset, 0);

        // Content should take remaining space below
        let content = &simple.dlg.borrow().children[1];
        assert_eq!(content.borrow().state.rows, 23);
        assert_eq!(content.borrow().state.row_offset, 1);
    }

    #[test]
    fn simple_dialog_with_content() {
        let content = MuttWindow::new(
            WindowType::Index,
            WindowOrientation::Vertical,
            WindowSize::Maximise,
            0,
            0,
        );

        let simple = SimpleDialog::with_content(WindowType::DlgIndex, None, Rc::clone(&content));

        assert_eq!(simple.dlg.borrow().children.len(), 2);

        // First child should be our content
        let first_child = &simple.dlg.borrow().children[0];
        assert!(MuttWindow::same_window(first_child, &content));
    }

    #[test]
    fn simple_dialog_apply_config_status_on_top() {
        let simple = SimpleDialog::create(WindowType::DlgIndex, None);
        assert!(SimpleDialog::apply_config(
            &simple.dlg,
            SimpleDialogConfig {
                status_on_top: true,
            }
        ));

        let first_child = &simple.dlg.borrow().children[0];
        assert_eq!(first_child.borrow().window_type, WindowType::StatusBar);
    }
}
