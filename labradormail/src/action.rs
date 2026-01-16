//! UI action catalog for key bindings and dispatch.

/// Actions understood by the UI layer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Action {
    Repaint,
    Abort,
    Help,
    Quit,
    EnterCommand,
    NextEntry,
    PrevEntry,
    NextLine,
    PrevLine,
    NextPage,
    PrevPage,
    HalfDown,
    HalfUp,
    FirstEntry,
    LastEntry,
    TopPage,
    BottomPage,
    Mail,
    Search,
    SidebarNext,
    SidebarPrev,
    Agent,
    YankMessage,
}

impl Action {
    /// Human-readable action description used by help UI.
    pub fn description(self) -> &'static str {
        match self {
            Action::Repaint => "repaint required",
            Action::Abort => "abort",
            Action::Help => "this screen",
            Action::Quit => "save changes to mailbox and quit",
            Action::EnterCommand => "enter a command",
            Action::NextEntry => "move to the next entry",
            Action::PrevEntry => "move to the previous entry",
            Action::NextLine => "scroll down one line",
            Action::PrevLine => "scroll up one line",
            Action::NextPage => "move to the next page",
            Action::PrevPage => "move to the previous page",
            Action::HalfDown => "scroll down 1/2 page",
            Action::HalfUp => "scroll up 1/2 page",
            Action::FirstEntry => "move to the first entry",
            Action::LastEntry => "move to the last entry",
            Action::TopPage => "move to the top of the page",
            Action::BottomPage => "move to the bottom of the page",
            Action::Mail => "compose a new mail message",
            Action::Search => "search for a regular expression",
            Action::SidebarNext => "move the highlight to next mailbox",
            Action::SidebarPrev => "move the highlight to previous mailbox",
            Action::Agent => "open AI agent chat dialog",
            Action::YankMessage => "yank current message into agent context",
        }
    }
}
