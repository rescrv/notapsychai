use super::OpString;

pub const OPCODES: &[OpString] = &[
    OpString {
        name: "OP_EDITOR_BACKSPACE",
        description: "delete the char in front of the cursor",
    },
    OpString {
        name: "OP_EDITOR_BACKWARD_CHAR",
        description: "move the cursor one character to the left",
    },
    OpString {
        name: "OP_EDITOR_BACKWARD_WORD",
        description: "move the cursor to the beginning of the word",
    },
    OpString {
        name: "OP_EDITOR_BOL",
        description: "jump to the beginning of the line",
    },
    OpString {
        name: "OP_EDITOR_CAPITALIZE_WORD",
        description: "capitalize the word",
    },
    OpString {
        name: "OP_EDITOR_COMPLETE",
        description: "complete filename or alias",
    },
    OpString {
        name: "OP_EDITOR_COMPLETE_QUERY",
        description: "complete address with query",
    },
    OpString {
        name: "OP_EDITOR_DELETE_CHAR",
        description: "delete the char under the cursor",
    },
    OpString {
        name: "OP_EDITOR_DOWNCASE_WORD",
        description: "convert the word to lower case",
    },
    OpString {
        name: "OP_EDITOR_EOL",
        description: "jump to the end of the line",
    },
    OpString {
        name: "OP_EDITOR_FORWARD_CHAR",
        description: "move the cursor one character to the right",
    },
    OpString {
        name: "OP_EDITOR_FORWARD_WORD",
        description: "move the cursor to the end of the word",
    },
    OpString {
        name: "OP_EDITOR_HISTORY_DOWN",
        description: "scroll down through the history list",
    },
    OpString {
        name: "OP_EDITOR_HISTORY_SEARCH",
        description: "search through the history list",
    },
    OpString {
        name: "OP_EDITOR_HISTORY_UP",
        description: "scroll up through the history list",
    },
    OpString {
        name: "OP_EDITOR_KILL_EOL",
        description: "delete chars from cursor to end of line",
    },
    OpString {
        name: "OP_EDITOR_KILL_EOW",
        description: "delete chars from the cursor to the end of the word",
    },
    OpString {
        name: "OP_EDITOR_KILL_LINE",
        description: "delete chars from cursor to beginning the line",
    },
    OpString {
        name: "OP_EDITOR_KILL_WHOLE_LINE",
        description: "delete all chars on the line",
    },
    OpString {
        name: "OP_EDITOR_KILL_WORD",
        description: "delete the word in front of the cursor",
    },
    OpString {
        name: "OP_EDITOR_MAILBOX_CYCLE",
        description: "cycle among incoming mailboxes",
    },
    OpString {
        name: "OP_EDITOR_QUOTE_CHAR",
        description: "quote the next typed key",
    },
    OpString {
        name: "OP_EDITOR_TRANSPOSE_CHARS",
        description: "transpose character under cursor with previous",
    },
    OpString {
        name: "OP_EDITOR_UPCASE_WORD",
        description: "convert the word to upper case",
    },
];
