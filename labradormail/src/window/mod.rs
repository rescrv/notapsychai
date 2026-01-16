//! Window module split into core/focus/notify submodules.

mod core;
mod focus;
mod layout;
mod notify;
mod widget;

pub use core::*;
pub use layout::LayoutBuilder;
pub use notify::*;
pub use widget::WindowWidget;
