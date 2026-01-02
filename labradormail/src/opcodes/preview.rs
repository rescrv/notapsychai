use super::OpString;

pub const OPCODES: &[OpString] = &[
    OpString {
        name: "OP_PREVIEW_PAGE_DOWN",
        description: "show the next page of the message",
    },
    OpString {
        name: "OP_PREVIEW_PAGE_UP",
        description: "show the previous page of the message",
    },
];
