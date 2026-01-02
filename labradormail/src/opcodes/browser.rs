use super::OpString;

pub const OPCODES: &[OpString] = &[
    OpString {
        name: "OP_BROWSER_GOTO_FOLDER",
        description: "swap the current folder position with $folder if it exists",
    },
    OpString {
        name: "OP_BROWSER_NEW_FILE",
        description: "select a new file in this directory",
    },
    OpString {
        name: "OP_BROWSER_SUBSCRIBE",
        description: "subscribe to current mbox (IMAP/NNTP only)",
    },
    OpString {
        name: "OP_BROWSER_TELL",
        description: "display the currently selected file's name",
    },
    OpString {
        name: "OP_BROWSER_TOGGLE_LSUB",
        description: "toggle view all/subscribed mailboxes (IMAP only)",
    },
    OpString {
        name: "OP_BROWSER_UNSUBSCRIBE",
        description: "unsubscribe from current mbox (IMAP/NNTP only)",
    },
    OpString {
        name: "OP_BROWSER_VIEW_FILE",
        description: "view file",
    },
];
