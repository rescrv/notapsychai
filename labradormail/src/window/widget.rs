//! Typed widget storage for windows.

use std::io::Result;
use std::io::Write;

use crate::agent_dialog::AgentDialogContentData;
use crate::context::GuiContext;
use crate::help_bar::HelpBarWindowData;
use crate::help_dialog::HelpDialogContentData;
use crate::inbox::InboxState;
use crate::inbox::IndexBarWidget;
use crate::inbox::IndexWidget;
use crate::inbox::MessageWidget;
use crate::inbox::PagerBarWidget;
use crate::inbox::PagerWidget;
use crate::inbox::SidebarWidget;
use crate::message_window::MsgWinWindowData;
use crate::sbar::SBarPrivateData;

use super::CursorBehavior;
use super::HasObserverId;
use super::RenderMode;
use super::WindowId;
use super::WindowTree;

/// Widgets that can be attached to windows.
#[derive(Debug)]
pub enum WindowWidget {
    HelpBar(HelpBarWindowData),
    StatusBar(SBarPrivateData),
    MessageWindow(MsgWinWindowData),
    HelpDialog(HelpDialogContentData),
    AgentDialog(Box<AgentDialogContentData>),
    InboxState(Box<InboxState>),
    Sidebar(SidebarWidget),
    Index(IndexWidget),
    Pager(PagerWidget),
    IndexBar(IndexBarWidget),
    PagerBar(PagerBarWidget),
    Message(MessageWidget),
}

impl WindowWidget {
    /// Updates widget state, if applicable.
    pub fn update(&mut self, tree: &mut WindowTree, win: WindowId) {
        match self {
            WindowWidget::HelpBar(data) => data.update(tree, win),
            WindowWidget::StatusBar(data) => data.update(tree, win),
            WindowWidget::MessageWindow(data) => data.update(tree, win),
            WindowWidget::HelpDialog(data) => data.update(tree, win),
            WindowWidget::AgentDialog(data) => data.update(tree, win),
            WindowWidget::InboxState(_) => {}
            WindowWidget::Sidebar(data) => data.update(tree, win),
            WindowWidget::Index(data) => data.update(tree, win),
            WindowWidget::Pager(data) => data.update(tree, win),
            WindowWidget::IndexBar(data) => data.update(tree, win),
            WindowWidget::PagerBar(data) => data.update(tree, win),
            WindowWidget::Message(data) => data.update(tree, win),
        }
    }

    /// Renders the widget.
    pub fn render(
        &mut self,
        tree: &mut WindowTree,
        win: WindowId,
        ctx: &mut GuiContext,
        out: &mut dyn Write,
        mode: RenderMode,
    ) -> Result<CursorBehavior> {
        match self {
            WindowWidget::HelpBar(data) => data.render(tree, win, ctx, out, mode),
            WindowWidget::StatusBar(data) => data.render(tree, win, ctx, out, mode),
            WindowWidget::MessageWindow(data) => data.render(tree, win, ctx, out, mode),
            WindowWidget::HelpDialog(data) => data.render(tree, win, ctx, out, mode),
            WindowWidget::AgentDialog(data) => data.render(tree, win, ctx, out, mode),
            WindowWidget::InboxState(_) => Ok(CursorBehavior::Hidden),
            WindowWidget::Sidebar(data) => data.render(tree, win, ctx, out, mode),
            WindowWidget::Index(data) => data.render(tree, win, ctx, out, mode),
            WindowWidget::Pager(data) => data.render(tree, win, ctx, out, mode),
            WindowWidget::IndexBar(data) => data.render(tree, win, ctx, out, mode),
            WindowWidget::PagerBar(data) => data.render(tree, win, ctx, out, mode),
            WindowWidget::Message(data) => data.render(tree, win, ctx, out, mode),
        }
    }

    /// Takes a stored observer ID for widgets that track one.
    pub fn take_observer_id(&mut self) -> Option<u64> {
        match self {
            WindowWidget::HelpBar(data) => data.take_observer_id(),
            WindowWidget::StatusBar(data) => data.take_observer_id(),
            WindowWidget::MessageWindow(data) => data.take_observer_id(),
            _ => None,
        }
    }
}
