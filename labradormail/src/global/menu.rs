//! Menu opcode mappings.

use crate::opcodes::OpCode;

bitflags::bitflags! {
    /// Flags for menu function definitions.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct MenuFuncFlags: u8 {
        /// No special flags.
        const NONE = 0;
        /// Deprecated menu function.
        const DEPRECATED = 0b0000_0001;
    }
}

/// Menu function definition (name -> opcode mapping).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MenuFuncOp {
    /// Function name.
    pub name: &'static str,
    /// Opcode invoked by this function.
    pub op: OpCode,
    /// Flags for the function.
    pub flags: MenuFuncFlags,
}

/// Dialog menu functions (OpDialog).
pub fn dialog_menu_functions() -> &'static [MenuFuncOp] {
    static OPS: [MenuFuncOp; 2] = [
        MenuFuncOp {
            name: "quit",
            op: OpCode::Quit,
            flags: MenuFuncFlags::NONE,
        },
        MenuFuncOp {
            name: "exit",
            op: OpCode::Exit,
            flags: MenuFuncFlags::NONE,
        },
    ];
    &OPS
}

/// Generic menu functions (OpGeneric).
pub fn generic_menu_functions() -> &'static [MenuFuncOp] {
    static OPS: [MenuFuncOp; 46] = [
        MenuFuncOp {
            name: "bottom-page",
            op: OpCode::BottomPage,
            flags: MenuFuncFlags::NONE,
        },
        MenuFuncOp {
            name: "check-stats",
            op: OpCode::CheckStats,
            flags: MenuFuncFlags::NONE,
        },
        MenuFuncOp {
            name: "current-bottom",
            op: OpCode::CurrentBottom,
            flags: MenuFuncFlags::NONE,
        },
        MenuFuncOp {
            name: "current-middle",
            op: OpCode::CurrentMiddle,
            flags: MenuFuncFlags::NONE,
        },
        MenuFuncOp {
            name: "current-top",
            op: OpCode::CurrentTop,
            flags: MenuFuncFlags::NONE,
        },
        MenuFuncOp {
            name: "end-cond",
            op: OpCode::EndCond,
            flags: MenuFuncFlags::NONE,
        },
        MenuFuncOp {
            name: "enter-command",
            op: OpCode::EnterCommand,
            flags: MenuFuncFlags::NONE,
        },
        MenuFuncOp {
            name: "exit",
            op: OpCode::Exit,
            flags: MenuFuncFlags::NONE,
        },
        MenuFuncOp {
            name: "first-entry",
            op: OpCode::FirstEntry,
            flags: MenuFuncFlags::NONE,
        },
        MenuFuncOp {
            name: "half-down",
            op: OpCode::HalfDown,
            flags: MenuFuncFlags::NONE,
        },
        MenuFuncOp {
            name: "half-up",
            op: OpCode::HalfUp,
            flags: MenuFuncFlags::NONE,
        },
        MenuFuncOp {
            name: "help",
            op: OpCode::Help,
            flags: MenuFuncFlags::NONE,
        },
        MenuFuncOp {
            name: "jump",
            op: OpCode::Jump,
            flags: MenuFuncFlags::NONE,
        },
        MenuFuncOp {
            name: "jump",
            op: OpCode::Jump1,
            flags: MenuFuncFlags::NONE,
        },
        MenuFuncOp {
            name: "jump",
            op: OpCode::Jump2,
            flags: MenuFuncFlags::NONE,
        },
        MenuFuncOp {
            name: "jump",
            op: OpCode::Jump3,
            flags: MenuFuncFlags::NONE,
        },
        MenuFuncOp {
            name: "jump",
            op: OpCode::Jump4,
            flags: MenuFuncFlags::NONE,
        },
        MenuFuncOp {
            name: "jump",
            op: OpCode::Jump5,
            flags: MenuFuncFlags::NONE,
        },
        MenuFuncOp {
            name: "jump",
            op: OpCode::Jump6,
            flags: MenuFuncFlags::NONE,
        },
        MenuFuncOp {
            name: "jump",
            op: OpCode::Jump7,
            flags: MenuFuncFlags::NONE,
        },
        MenuFuncOp {
            name: "jump",
            op: OpCode::Jump8,
            flags: MenuFuncFlags::NONE,
        },
        MenuFuncOp {
            name: "jump",
            op: OpCode::Jump9,
            flags: MenuFuncFlags::NONE,
        },
        MenuFuncOp {
            name: "last-entry",
            op: OpCode::LastEntry,
            flags: MenuFuncFlags::NONE,
        },
        MenuFuncOp {
            name: "middle-page",
            op: OpCode::MiddlePage,
            flags: MenuFuncFlags::NONE,
        },
        MenuFuncOp {
            name: "next-entry",
            op: OpCode::NextEntry,
            flags: MenuFuncFlags::NONE,
        },
        MenuFuncOp {
            name: "next-line",
            op: OpCode::NextLine,
            flags: MenuFuncFlags::NONE,
        },
        MenuFuncOp {
            name: "next-page",
            op: OpCode::NextPage,
            flags: MenuFuncFlags::NONE,
        },
        MenuFuncOp {
            name: "previous-entry",
            op: OpCode::PrevEntry,
            flags: MenuFuncFlags::NONE,
        },
        MenuFuncOp {
            name: "previous-line",
            op: OpCode::PrevLine,
            flags: MenuFuncFlags::NONE,
        },
        MenuFuncOp {
            name: "previous-page",
            op: OpCode::PrevPage,
            flags: MenuFuncFlags::NONE,
        },
        MenuFuncOp {
            name: "redraw-screen",
            op: OpCode::Redraw,
            flags: MenuFuncFlags::NONE,
        },
        MenuFuncOp {
            name: "search",
            op: OpCode::Search,
            flags: MenuFuncFlags::NONE,
        },
        MenuFuncOp {
            name: "search-next",
            op: OpCode::SearchNext,
            flags: MenuFuncFlags::NONE,
        },
        MenuFuncOp {
            name: "search-opposite",
            op: OpCode::SearchOpposite,
            flags: MenuFuncFlags::NONE,
        },
        MenuFuncOp {
            name: "search-reverse",
            op: OpCode::SearchReverse,
            flags: MenuFuncFlags::NONE,
        },
        MenuFuncOp {
            name: "select-entry",
            op: OpCode::GenericSelectEntry,
            flags: MenuFuncFlags::NONE,
        },
        MenuFuncOp {
            name: "shell-escape",
            op: OpCode::ShellEscape,
            flags: MenuFuncFlags::NONE,
        },
        MenuFuncOp {
            name: "show-log-messages",
            op: OpCode::ShowLogMessages,
            flags: MenuFuncFlags::NONE,
        },
        MenuFuncOp {
            name: "show-version",
            op: OpCode::Version,
            flags: MenuFuncFlags::NONE,
        },
        MenuFuncOp {
            name: "tag-entry",
            op: OpCode::Tag,
            flags: MenuFuncFlags::NONE,
        },
        MenuFuncOp {
            name: "tag-prefix",
            op: OpCode::TagPrefix,
            flags: MenuFuncFlags::NONE,
        },
        MenuFuncOp {
            name: "tag-prefix-cond",
            op: OpCode::TagPrefixCond,
            flags: MenuFuncFlags::NONE,
        },
        MenuFuncOp {
            name: "top-page",
            op: OpCode::TopPage,
            flags: MenuFuncFlags::NONE,
        },
        MenuFuncOp {
            name: "what-key",
            op: OpCode::WhatKey,
            flags: MenuFuncFlags::NONE,
        },
        MenuFuncOp {
            name: "error-history",
            op: OpCode::ShowLogMessages,
            flags: MenuFuncFlags::DEPRECATED,
        },
        MenuFuncOp {
            name: "refresh",
            op: OpCode::Redraw,
            flags: MenuFuncFlags::DEPRECATED,
        },
    ];
    &OPS
}
