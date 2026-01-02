use super::OpString;

pub const OPCODES: &[OpString] = &[
    OpString {
        name: "OP_ENVELOPE_EDIT_BCC",
        description: "edit the BCC list",
    },
    OpString {
        name: "OP_ENVELOPE_EDIT_CC",
        description: "edit the CC list",
    },
    OpString {
        name: "OP_ENVELOPE_EDIT_FCC",
        description: "enter a file to save a copy of this message in",
    },
    OpString {
        name: "OP_ENVELOPE_EDIT_FOLLOWUP_TO",
        description: "edit the Followup-To field",
    },
    OpString {
        name: "OP_ENVELOPE_EDIT_FROM",
        description: "edit the from field",
    },
    OpString {
        name: "OP_ENVELOPE_EDIT_HEADERS",
        description: "edit the message with headers",
    },
    OpString {
        name: "OP_ENVELOPE_EDIT_NEWSGROUPS",
        description: "edit the newsgroups list",
    },
    OpString {
        name: "OP_ENVELOPE_EDIT_REPLY_TO",
        description: "edit the Reply-To field",
    },
    OpString {
        name: "OP_ENVELOPE_EDIT_SUBJECT",
        description: "edit the subject of this message",
    },
    OpString {
        name: "OP_ENVELOPE_EDIT_TO",
        description: "edit the TO list",
    },
    OpString {
        name: "OP_ENVELOPE_EDIT_X_COMMENT_TO",
        description: "edit the X-Comment-To field",
    },
];
