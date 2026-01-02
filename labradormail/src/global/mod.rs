//! Global function dispatch and opcode bindings.
//!
//! This mirrors NeoMutt's global dispatcher and default key bindings in a
//! minimal, Rust-friendly form.

mod bindings;
mod dispatch;
mod menu;

pub use bindings::dialog_default_bindings;
pub use bindings::generic_default_bindings;
pub use bindings::help_data_from_bindings;
pub use bindings::lookup_binding;
pub use bindings::Key;
pub use bindings::KeyBinding;
pub use dispatch::default_global_functions;
pub use dispatch::dialog_stub_functions;
pub use dispatch::generic_stub_functions;
pub use dispatch::global_function_dispatcher;
pub use dispatch::global_function_dispatcher_active;
pub use dispatch::global_function_dispatcher_rc;
pub use dispatch::FunctionRetval;
pub use dispatch::GlobalFunction;
pub use dispatch::GlobalFunctionEntry;
pub use menu::dialog_menu_functions;
pub use menu::generic_menu_functions;
pub use menu::MenuFuncFlags;
pub use menu::MenuFuncOp;
