use super::OpString;

pub const OPCODES: &[OpString] = &[
    OpString {
        name: "OP_COMPOSE_EDIT_FILE",
        description: "edit the file to be attached",
    },
    OpString {
        name: "OP_COMPOSE_EDIT_MESSAGE",
        description: "edit the message",
    },
    OpString {
        name: "OP_COMPOSE_ISPELL",
        description: "run ispell on the message",
    },
    OpString {
        name: "OP_COMPOSE_POSTPONE_MESSAGE",
        description: "save this message to send later",
    },
    OpString {
        name: "OP_COMPOSE_RENAME_FILE",
        description: "rename/move an attached file",
    },
    OpString {
        name: "OP_COMPOSE_SEND_MESSAGE",
        description: "send the message",
    },
    OpString {
        name: "OP_COMPOSE_TO_SENDER",
        description: "compose new message to the current message sender",
    },
    OpString {
        name: "OP_COMPOSE_WRITE_MESSAGE",
        description: "write the message to a folder",
    },
    OpString {
        name: "OP_COMPOSE_PGP_MENU",
        description: "show PGP options",
    },
    OpString {
        name: "OP_COMPOSE_SMIME_MENU",
        description: "show S/MIME options",
    },
];
