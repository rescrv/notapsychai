use std::io::Result;
use std::io::Stdout;
use std::io::Write;

use crate::context::GuiContext;
use crate::global::GlobalFunctionEntry;
use crate::help_data::HelpData;
use crate::help_data::HelpItem;
use crate::reflow::window_reflow;
use crate::root_window::resize_screen;
use crate::root_window::RootWindow;
use crate::root_window::RootWindowConfig;
use crate::window::WindowId;
use crate::window::WindowType;
use crate::window::WindowWidget;
use crate::Bindings;

use super::commands::inbox_global_functions;
use super::state::InboxState;
use super::state::MailboxState;
use super::view::IndexBarWidget;
use super::view::IndexWidget;
use super::view::MessageWidget;
use super::view::PagerBarWidget;
use super::view::PagerWidget;
use super::view::SidebarWidget;
use agent_inbox_protocol::Mailbox;

/// Window handles.
pub struct Windows {
    #[allow(dead_code)]
    pub root: RootWindow,
    pub ctx: GuiContext,
    pub help_bar: WindowId,
    pub message_container: WindowId,
    pub global_functions: Vec<GlobalFunctionEntry>,
}

impl Windows {
    pub fn new(
        stdout: &mut Stdout,
        mailboxes: Vec<Mailbox>,
        config: RootWindowConfig,
        dialog_bindings: Bindings,
        global_bindings: Bindings,
    ) -> Result<Self> {
        Self::new_with_inbox_state(
            stdout,
            InboxState::new(mailboxes),
            config,
            dialog_bindings,
            global_bindings,
        )
    }

    pub fn new_with_states(
        stdout: &mut Stdout,
        mailbox_states: Vec<MailboxState>,
        config: RootWindowConfig,
        dialog_bindings: Bindings,
        global_bindings: Bindings,
    ) -> Result<Self> {
        Self::new_with_inbox_state(
            stdout,
            InboxState::new_with_states(mailbox_states),
            config,
            dialog_bindings,
            global_bindings,
        )
    }

    fn new_with_inbox_state(
        stdout: &mut Stdout,
        inbox_state: InboxState,
        config: RootWindowConfig,
        dialog_bindings: Bindings,
        global_bindings: Bindings,
    ) -> Result<Self> {
        let mut ctx = GuiContext::new();
        let help_data = HelpData::from_items(vec![HelpItem::new("?", "Press ? for help")]);
        let mut root = RootWindow::new_with_config(stdout, config, Some(help_data))?;
        ctx.register_root_window(root.root_id());

        let main_layout = root.main_layout;
        {
            let dlg_win = if let Some(WindowWidget::InboxState(state)) =
                root.tree_mut().get_mut(main_layout).widget_mut()
            {
                Some(state.as_mut())
            } else {
                None
            };
            if let Some(state) = dlg_win {
                *state = inbox_state;
            } else {
                root.tree_mut()
                    .get_mut(main_layout)
                    .set_widget(WindowWidget::InboxState(Box::new(inbox_state)));
            }
            // Store bindings for the help dialog to use
            let dlg = root.tree_mut().get_mut(main_layout);
            dlg.bindings.extend(&dialog_bindings);
            dlg.bindings.extend(&global_bindings);
        }

        let global_functions = inbox_global_functions();

        // Initial reflow.
        let root_id = root.root_id();
        window_reflow(root.tree_mut(), root_id);

        let help_bar = root.help_bar_id();
        let message_container = root.message_container_id();

        let sidebar = root.sidebar;
        let index_window = root.index_window;
        let index_bar = root.index_bar;
        let pager_window = root.pager_window;
        let pager_bar = root.pager_bar;

        // Attach widgets to windows.
        root.tree_mut()
            .get_mut(sidebar)
            .set_widget(WindowWidget::Sidebar(SidebarWidget));
        root.tree_mut()
            .get_mut(index_window)
            .set_widget(WindowWidget::Index(IndexWidget));
        root.tree_mut()
            .get_mut(index_bar)
            .set_widget(WindowWidget::IndexBar(IndexBarWidget));
        root.tree_mut()
            .get_mut(pager_window)
            .set_widget(WindowWidget::Pager(PagerWidget));
        root.tree_mut()
            .get_mut(pager_bar)
            .set_widget(WindowWidget::PagerBar(PagerBarWidget));
        root.tree_mut()
            .get_mut(message_container)
            .set_widget(WindowWidget::Message(MessageWidget));

        Ok(Self {
            root,
            ctx,
            help_bar,
            message_container,
            global_functions,
        })
    }

    pub fn handle_resize(&mut self) -> Result<()> {
        resize_screen(&mut self.ctx, &mut self.root)
    }

    /// Returns a reference to the inbox state stored in the dialog.
    pub fn state(&self) -> Option<&InboxState> {
        let win = self.root.main_layout;
        if let Some(WindowWidget::InboxState(state)) = self.root.tree().get(win).widget_ref() {
            Some(state.as_ref())
        } else {
            None
        }
    }

    /// Returns a mutable reference to the inbox state stored in the dialog.
    pub fn state_mut(&mut self) -> Option<&mut InboxState> {
        let win = self.root.main_layout;
        if let Some(WindowWidget::InboxState(state)) =
            self.root.tree_mut().get_mut(win).widget_mut()
        {
            Some(state.as_mut())
        } else {
            None
        }
    }
}

/// Redraw only dirty windows.
pub fn redraw(stdout: &mut Stdout, windows: &mut Windows) -> Result<()> {
    let help_bar = windows.help_bar;
    let sidebar = windows.root.sidebar;
    let index_window = windows.root.index_window;
    let index_bar = windows.root.index_bar;
    let pager_window = windows.root.pager_window;
    let pager_bar = windows.root.pager_bar;
    let message_container = windows.message_container;

    let help_bar_dirty = windows
        .state()
        .is_some_and(|state| state.dirty.is_help_bar_dirty());
    if help_bar_dirty {
        windows
            .root
            .tree_mut()
            .get_mut(help_bar)
            .mark_recalc_repaint();
        windows
            .root
            .tree_mut()
            .redraw(help_bar, &mut windows.ctx, stdout)?;
        if let Some(state) = windows.state_mut() {
            state.dirty.set_help_bar_dirty(false);
        }
    }

    let sidebar_dirty = windows
        .state()
        .is_some_and(|state| state.dirty.is_sidebar_dirty());
    if sidebar_dirty {
        windows.root.tree_mut().get_mut(sidebar).mark_repaint();
        windows
            .root
            .tree_mut()
            .redraw(sidebar, &mut windows.ctx, stdout)?;
        if let Some(state) = windows.state_mut() {
            state.dirty.set_sidebar_dirty(false);
        }
    }

    let index_dirty = windows
        .state()
        .is_some_and(|state| state.dirty.is_index_dirty());
    if index_dirty {
        let viewport_height: usize = windows
            .root
            .tree()
            .get(index_window)
            .state
            .rect
            .size
            .rows
            .try_into()
            .unwrap_or(0);
        if let Some(state) = windows.state_mut() {
            state.update_scroll_offset(viewport_height);
        }
        windows.root.tree_mut().get_mut(index_window).mark_repaint();
        windows
            .root
            .tree_mut()
            .redraw(index_window, &mut windows.ctx, stdout)?;
        if let Some(state) = windows.state_mut() {
            state.dirty.set_index_dirty(false);
        }
    }

    let index_bar_dirty = windows
        .state()
        .is_some_and(|state| state.dirty.is_index_bar_dirty());
    if index_bar_dirty {
        windows.root.tree_mut().get_mut(index_bar).mark_repaint();
        windows
            .root
            .tree_mut()
            .redraw(index_bar, &mut windows.ctx, stdout)?;
        if let Some(state) = windows.state_mut() {
            state.dirty.set_index_bar_dirty(false);
        }
    }

    let pager_dirty = windows
        .state()
        .is_some_and(|state| state.dirty.is_pager_dirty());
    if pager_dirty {
        windows.root.tree_mut().get_mut(pager_window).mark_repaint();
        windows
            .root
            .tree_mut()
            .redraw(pager_window, &mut windows.ctx, stdout)?;
        if let Some(state) = windows.state_mut() {
            state.dirty.set_pager_dirty(false);
        }
    }

    let pager_bar_dirty = windows
        .state()
        .is_some_and(|state| state.dirty.is_pager_bar_dirty());
    if pager_bar_dirty {
        windows.root.tree_mut().get_mut(pager_bar).mark_repaint();
        windows
            .root
            .tree_mut()
            .redraw(pager_bar, &mut windows.ctx, stdout)?;
        if let Some(state) = windows.state_mut() {
            state.dirty.set_pager_bar_dirty(false);
        }
    }

    let message_dirty = windows
        .state()
        .is_some_and(|state| state.dirty.is_message_dirty());
    if message_dirty {
        windows
            .root
            .tree_mut()
            .get_mut(message_container)
            .mark_repaint();
        windows
            .root
            .tree_mut()
            .redraw(message_container, &mut windows.ctx, stdout)?;
        if let Some(state) = windows.state_mut() {
            state.dirty.set_message_dirty(false);
        }
    }

    if let Some(top) = windows.root.dialog_top() {
        let win = windows.root.tree().get(top);
        let window_type = win.window_type;
        if matches!(window_type, WindowType::DlgHelp | WindowType::DlgAgent) {
            windows
                .root
                .tree_mut()
                .redraw(top, &mut windows.ctx, stdout)?;
        }
    }

    stdout.flush()?;
    Ok(())
}

/// Returns the current top dialog window ID, if any.
pub fn current_dialog_top(windows: &Windows) -> Option<WindowId> {
    windows.root.dialog_top()
}

pub fn is_primary_dialog_active(windows: &Windows) -> bool {
    let top = windows.root.dialog_top();
    match top {
        Some(top) => top == windows.root.main_layout,
        None => true,
    }
}
