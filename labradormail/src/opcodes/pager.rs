use super::OpString;

pub const OPCODES: &[OpString] = &[
    OpString {
        name: "OP_PAGER_BOTTOM",
        description: "jump to the bottom of the message",
    },
    OpString {
        name: "OP_PAGER_HIDE_QUOTED",
        description: "toggle display of quoted text",
    },
    OpString {
        name: "OP_PAGER_SKIP_HEADERS",
        description: "jump to first line after headers",
    },
    OpString {
        name: "OP_PAGER_SKIP_QUOTED",
        description: "skip beyond quoted text",
    },
    OpString {
        name: "OP_PAGER_TOP",
        description: "jump to the top of the message",
    },
];
