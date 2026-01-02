use super::OpString;

pub const OPCODES: &[OpString] = &[
    OpString {
        name: "OP_SIDEBAR_FIRST",
        description: "move the highlight to the first mailbox",
    },
    OpString {
        name: "OP_SIDEBAR_LAST",
        description: "move the highlight to the last mailbox",
    },
    OpString {
        name: "OP_SIDEBAR_NEXT",
        description: "move the highlight to next mailbox",
    },
    OpString {
        name: "OP_SIDEBAR_NEXT_NEW",
        description: "move the highlight to next mailbox with new mail",
    },
    OpString {
        name: "OP_SIDEBAR_OPEN",
        description: "open highlighted mailbox",
    },
    OpString {
        name: "OP_SIDEBAR_PAGE_DOWN",
        description: "scroll the sidebar down 1 page",
    },
    OpString {
        name: "OP_SIDEBAR_PAGE_UP",
        description: "scroll the sidebar up 1 page",
    },
    OpString {
        name: "OP_SIDEBAR_PREV",
        description: "move the highlight to previous mailbox",
    },
    OpString {
        name: "OP_SIDEBAR_PREV_NEW",
        description: "move the highlight to previous mailbox with new mail",
    },
    OpString {
        name: "OP_SIDEBAR_TOGGLE_VIRTUAL",
        description: "toggle between mailboxes and virtual mailboxes",
    },
    OpString {
        name: "OP_SIDEBAR_TOGGLE_VISIBLE",
        description: "make the sidebar (in)visible",
    },
];
