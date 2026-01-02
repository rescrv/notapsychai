//! Common layout helpers.

use std::cell::RefCell;
use std::rc::Rc;

use crate::dialog::Dialog;
use crate::help_data::HelpData;
use crate::window::MuttWindow;
use crate::window::WindowOrientation;
use crate::window::WindowSize;
use crate::window::WindowType;

/// Configuration for the index/pager layout.
#[derive(Debug, Clone, Copy)]
pub struct IndexPagerLayoutConfig {
    /// Width of the sidebar in columns.
    pub sidebar_width: i16,
    /// Height of status bars in rows.
    pub status_height: i16,
}

impl Default for IndexPagerLayoutConfig {
    fn default() -> Self {
        Self {
            sidebar_width: 20,
            status_height: 1,
        }
    }
}

/// A standard index/pager layout with sidebar and status bars.
pub struct IndexPagerLayout {
    /// Main dialog window.
    pub dialog: Rc<RefCell<MuttWindow>>,
    /// Sidebar window.
    pub sidebar: Rc<RefCell<MuttWindow>>,
    /// Index (message list) window.
    pub index_window: Rc<RefCell<MuttWindow>>,
    /// Status bar for the index window.
    pub index_bar: Rc<RefCell<MuttWindow>>,
    /// Pager (message view) window.
    pub pager_window: Rc<RefCell<MuttWindow>>,
    /// Status bar for the pager window.
    pub pager_bar: Rc<RefCell<MuttWindow>>,
}

impl IndexPagerLayout {
    /// Creates a new index/pager layout with default configuration.
    ///
    /// The help data is stored on the dialog window for the help bar to display.
    pub fn new(window_type: WindowType, help_data: Option<Rc<HelpData>>) -> Self {
        Self::new_with_config(window_type, help_data, IndexPagerLayoutConfig::default())
    }

    /// Creates a new index/pager layout with explicit configuration.
    ///
    /// The help data is stored on the dialog window for the help bar to display.
    pub fn new_with_config(
        window_type: WindowType,
        help_data: Option<Rc<HelpData>>,
        config: IndexPagerLayoutConfig,
    ) -> Self {
        let dialog = Dialog::new(window_type);
        {
            let mut borrowed = dialog.window().borrow_mut();
            borrowed.help_data = help_data;
        }

        let sidebar = MuttWindow::new(
            WindowType::Sidebar,
            WindowOrientation::Vertical,
            WindowSize::Fixed,
            config.sidebar_width,
            0,
        );

        let layout_row = MuttWindow::new(
            WindowType::Container,
            WindowOrientation::Horizontal,
            WindowSize::Maximise,
            0,
            0,
        );

        let main_panel = MuttWindow::new(
            WindowType::Container,
            WindowOrientation::Vertical,
            WindowSize::Maximise,
            0,
            0,
        );

        let index_panel = MuttWindow::new(
            WindowType::Container,
            WindowOrientation::Vertical,
            WindowSize::Maximise,
            0,
            0,
        );
        let index_window = MuttWindow::new(
            WindowType::Index,
            WindowOrientation::Vertical,
            WindowSize::Maximise,
            0,
            0,
        );
        let index_bar = MuttWindow::new(
            WindowType::StatusBar,
            WindowOrientation::Vertical,
            WindowSize::Fixed,
            0,
            config.status_height,
        );

        let pager_panel = MuttWindow::new(
            WindowType::Container,
            WindowOrientation::Vertical,
            WindowSize::Maximise,
            0,
            0,
        );
        let pager_window = MuttWindow::new(
            WindowType::Pager,
            WindowOrientation::Vertical,
            WindowSize::Maximise,
            0,
            0,
        );
        let pager_bar = MuttWindow::new(
            WindowType::StatusBar,
            WindowOrientation::Vertical,
            WindowSize::Fixed,
            0,
            config.status_height,
        );

        MuttWindow::add_child(&index_panel, Rc::clone(&index_window));
        MuttWindow::add_child(&index_panel, Rc::clone(&index_bar));
        MuttWindow::add_child(&pager_panel, Rc::clone(&pager_window));
        MuttWindow::add_child(&pager_panel, Rc::clone(&pager_bar));
        MuttWindow::add_child(&main_panel, Rc::clone(&index_panel));
        MuttWindow::add_child(&main_panel, Rc::clone(&pager_panel));
        MuttWindow::add_child(&layout_row, Rc::clone(&sidebar));
        MuttWindow::add_child(&layout_row, Rc::clone(&main_panel));
        MuttWindow::add_child(dialog.window(), Rc::clone(&layout_row));

        MuttWindow::set_focus(dialog.window());

        Self {
            dialog: Rc::clone(dialog.window()),
            sidebar,
            index_window,
            index_bar,
            pager_window,
            pager_bar,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn index_pager_layout_create() {
        let layout = IndexPagerLayout::new(WindowType::DlgIndex, None);

        assert_eq!(layout.dialog.borrow().window_type, WindowType::DlgIndex);
        assert_eq!(layout.sidebar.borrow().window_type, WindowType::Sidebar);
        assert_eq!(layout.index_window.borrow().window_type, WindowType::Index);
        assert_eq!(layout.index_bar.borrow().window_type, WindowType::StatusBar);
        assert_eq!(layout.pager_window.borrow().window_type, WindowType::Pager);
        assert_eq!(layout.pager_bar.borrow().window_type, WindowType::StatusBar);
    }
}
