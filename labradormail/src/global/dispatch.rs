//! Global opcode dispatch and default handler implementations.

use std::cell::RefCell;
use std::rc::Rc;

use crate::context::GuiContext;
use crate::dialog::dialog_stack_pop;
use crate::dialog::dialog_stack_push;
use crate::help_data::HelpData;
use crate::help_dialog::HelpDialog;
use crate::opcodes::OpCode;
use crate::window::MuttWindow;
use crate::window::WindowActionFlags;
use crate::window::WindowType;

use super::bindings::generic_default_bindings;
use super::bindings::help_data_from_bindings;

/// Return value for global function handlers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FunctionRetval {
    /// Handler completed successfully.
    Done,
    /// Handler did not handle the opcode.
    Unhandled,
    /// Handler requests abort (e.g., Quit).
    Abort,
}

/// Global function signature.
pub type GlobalFunction = fn(&mut MuttWindow, &mut GuiContext, OpCode) -> FunctionRetval;

/// Opcode -> handler mapping entry.
#[derive(Debug, Clone, Copy)]
pub struct GlobalFunctionEntry {
    /// Opcode to match.
    pub op: OpCode,
    /// Handler for the opcode.
    pub function: GlobalFunction,
}

/// Dispatches an opcode to the matching handler.
pub fn global_function_dispatcher(
    win: &mut MuttWindow,
    ctx: &mut GuiContext,
    op: OpCode,
    table: &[GlobalFunctionEntry],
) -> FunctionRetval {
    for entry in table {
        if entry.op == op {
            return (entry.function)(win, ctx, op);
        }
    }
    FunctionRetval::Unhandled
}

/// Dispatches an opcode to the matching handler using an Rc-wrapped window.
pub fn global_function_dispatcher_rc(
    win: &Rc<RefCell<MuttWindow>>,
    ctx: &mut GuiContext,
    op: OpCode,
    table: &[GlobalFunctionEntry],
) -> FunctionRetval {
    match op {
        OpCode::Help => return op_help_rc(win),
        OpCode::Quit => return op_quit_rc(win),
        _ => {}
    }
    let mut borrowed = win.borrow_mut();
    global_function_dispatcher(&mut borrowed, ctx, op, table)
}

/// Dispatches to the topmost dialog when present, blocking fallthrough to
/// lower dialogs if the active dialog didn't handle the opcode.
pub fn global_function_dispatcher_active(
    win: &Rc<RefCell<MuttWindow>>,
    ctx: &mut GuiContext,
    op: OpCode,
    table: &[GlobalFunctionEntry],
) -> FunctionRetval {
    let (target, blocks_fallback) = active_dialog_target(win);
    let ret = global_function_dispatcher_rc(&target, ctx, op, table);
    if blocks_fallback && matches!(ret, FunctionRetval::Unhandled) {
        return FunctionRetval::Done;
    }
    ret
}

fn active_dialog_target(win: &Rc<RefCell<MuttWindow>>) -> (Rc<RefCell<MuttWindow>>, bool) {
    let root = MuttWindow::get_root(win);
    let Some(all_dialogs) = MuttWindow::find_child(&root, WindowType::AllDialogs) else {
        return (Rc::clone(win), false);
    };
    let top = {
        let borrowed = all_dialogs.borrow();
        borrowed.children.last().cloned()
    };
    if let Some(top) = top {
        let blocks_fallback = !MuttWindow::same_window(&top, win);
        (top, blocks_fallback)
    } else {
        (Rc::clone(win), false)
    }
}

fn op_repaint(win: &mut MuttWindow, _ctx: &mut GuiContext, _op: OpCode) -> FunctionRetval {
    win.actions |= WindowActionFlags::REPAINT;
    FunctionRetval::Done
}

fn op_abort(_win: &mut MuttWindow, _ctx: &mut GuiContext, _op: OpCode) -> FunctionRetval {
    FunctionRetval::Abort
}

fn op_help(win: &mut MuttWindow, _ctx: &mut GuiContext, _op: OpCode) -> FunctionRetval {
    let window_type = win.window_type;
    let help_data = win.help_data.as_ref().map(|data| data.as_ref().clone());
    let Some(root) = root_for_window(win) else {
        return FunctionRetval::Unhandled;
    };
    let Some(all_dialogs) = MuttWindow::find_child(&root, WindowType::AllDialogs) else {
        return FunctionRetval::Unhandled;
    };

    if window_type == WindowType::DlgHelp {
        win.set_visible(false);
        win.parent = None;
        let _ = dialog_stack_pop(&all_dialogs, false);
        return FunctionRetval::Done;
    }

    show_help_dialog(help_data, all_dialogs, || win.set_visible(false))
}

fn op_help_rc(win: &Rc<RefCell<MuttWindow>>) -> FunctionRetval {
    let (window_type, root, help_data) = {
        let borrowed = win.borrow();
        let help_data = borrowed
            .help_data
            .as_ref()
            .map(|data| data.as_ref().clone());
        (borrowed.window_type, root_for_window(&borrowed), help_data)
    };
    let Some(root) = root else {
        return FunctionRetval::Unhandled;
    };
    let Some(all_dialogs) = MuttWindow::find_child(&root, WindowType::AllDialogs) else {
        return FunctionRetval::Unhandled;
    };

    if window_type == WindowType::DlgHelp {
        {
            let mut borrowed = win.borrow_mut();
            borrowed.set_visible(false);
            borrowed.parent = None;
        }
        let _ = dialog_stack_pop(&all_dialogs, false);
        return FunctionRetval::Done;
    }

    show_help_dialog(help_data, all_dialogs, || {
        let mut borrowed = win.borrow_mut();
        borrowed.set_visible(false);
    })
}

fn show_help_dialog<F>(
    help_data: Option<HelpData>,
    all_dialogs: Rc<RefCell<MuttWindow>>,
    hide_window: F,
) -> FunctionRetval
where
    F: FnOnce(),
{
    let help_data =
        help_data.unwrap_or_else(|| help_data_from_bindings(generic_default_bindings()));
    let dialog = HelpDialog::new(&help_data);
    let dialog_win = Rc::clone(dialog.window());
    hide_window();
    dialog_stack_push(&all_dialogs, Rc::clone(&dialog_win), false);
    MuttWindow::set_focus(&dialog_win);
    FunctionRetval::Done
}

fn op_quit(win: &mut MuttWindow, _ctx: &mut GuiContext, _op: OpCode) -> FunctionRetval {
    let Some(root) = root_for_window(win) else {
        return FunctionRetval::Abort;
    };
    op_quit_inner(root)
}

fn op_quit_rc(win: &Rc<RefCell<MuttWindow>>) -> FunctionRetval {
    let root = {
        let borrowed = win.borrow();
        root_for_window(&borrowed)
    };
    let Some(root) = root else {
        return FunctionRetval::Abort;
    };
    op_quit_inner(root)
}

fn op_quit_inner(root: Rc<RefCell<MuttWindow>>) -> FunctionRetval {
    if let Some(all_dialogs) = MuttWindow::find_child(&root, WindowType::AllDialogs) {
        let is_help_top = {
            let borrowed = all_dialogs.borrow();
            borrowed
                .children
                .last()
                .map(|top| top.borrow().window_type == WindowType::DlgHelp)
                .unwrap_or(false)
        };
        if is_help_top {
            let _ = dialog_stack_pop(&all_dialogs, true);
            let next_focus = {
                let borrowed = all_dialogs.borrow();
                borrowed.children.last().cloned()
            };
            if let Some(next_focus) = next_focus {
                MuttWindow::set_focus(&next_focus);
            } else {
                MuttWindow::set_focus(&root);
            }
            return FunctionRetval::Done;
        }
    }
    FunctionRetval::Abort
}

fn root_for_window(win: &MuttWindow) -> Option<Rc<RefCell<MuttWindow>>> {
    win.parent
        .as_ref()
        .and_then(|parent| parent.upgrade())
        .map(|parent| MuttWindow::get_root(&parent))
}

/// Default global functions.
pub fn default_global_functions() -> &'static [GlobalFunctionEntry] {
    static FUNCTIONS: [GlobalFunctionEntry; 4] = [
        GlobalFunctionEntry {
            op: OpCode::Repaint,
            function: op_repaint,
        },
        GlobalFunctionEntry {
            op: OpCode::Abort,
            function: op_abort,
        },
        GlobalFunctionEntry {
            op: OpCode::Help,
            function: op_help,
        },
        GlobalFunctionEntry {
            op: OpCode::Quit,
            function: op_quit,
        },
    ];
    &FUNCTIONS
}

fn op_stub(_win: &mut MuttWindow, _ctx: &mut GuiContext, _op: OpCode) -> FunctionRetval {
    FunctionRetval::Unhandled
}

/// Creates stub function entries from menu function definitions.
fn stub_functions_from(menu_funcs: &[super::menu::MenuFuncOp]) -> Vec<GlobalFunctionEntry> {
    menu_funcs
        .iter()
        .map(|entry| GlobalFunctionEntry {
            op: entry.op,
            function: op_stub,
        })
        .collect()
}

/// Dialog stub functions used during porting.
pub fn dialog_stub_functions() -> Vec<GlobalFunctionEntry> {
    stub_functions_from(super::menu::dialog_menu_functions())
}

/// Generic stub functions used during porting.
pub fn generic_stub_functions() -> Vec<GlobalFunctionEntry> {
    stub_functions_from(super::menu::generic_menu_functions())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn global_dispatcher_empty_table_returns_unhandled() {
        let win = MuttWindow::new(
            WindowType::Root,
            crate::window::WindowOrientation::Vertical,
            crate::window::WindowSize::Fixed,
            80,
            24,
        );
        let mut borrowed = win.borrow_mut();
        let mut ctx = GuiContext::new();
        let ret = global_function_dispatcher(&mut borrowed, &mut ctx, OpCode::Repaint, &[]);
        assert_eq!(ret, FunctionRetval::Unhandled);
    }

    #[test]
    fn global_dispatcher_active_without_all_dialogs() {
        let win = MuttWindow::new(
            WindowType::Root,
            crate::window::WindowOrientation::Vertical,
            crate::window::WindowSize::Fixed,
            80,
            24,
        );
        let mut ctx = GuiContext::new();
        let ret = global_function_dispatcher_active(&win, &mut ctx, OpCode::Repaint, &[]);
        assert_eq!(ret, FunctionRetval::Unhandled);
    }

    #[test]
    fn global_dispatcher_active_blocks_fallback() {
        let root = MuttWindow::new(
            WindowType::Root,
            crate::window::WindowOrientation::Vertical,
            crate::window::WindowSize::Fixed,
            80,
            24,
        );
        let all_dialogs = MuttWindow::new(
            WindowType::AllDialogs,
            crate::window::WindowOrientation::Vertical,
            crate::window::WindowSize::Maximise,
            0,
            0,
        );
        let other = MuttWindow::new(
            WindowType::Container,
            crate::window::WindowOrientation::Vertical,
            crate::window::WindowSize::Fixed,
            10,
            10,
        );
        let dialog = MuttWindow::new(
            WindowType::DlgHelp,
            crate::window::WindowOrientation::Vertical,
            crate::window::WindowSize::Maximise,
            0,
            0,
        );

        MuttWindow::add_child(&root, Rc::clone(&all_dialogs));
        MuttWindow::add_child(&root, Rc::clone(&other));
        crate::dialog::dialog_stack_push(&all_dialogs, Rc::clone(&dialog), true);

        let mut ctx = GuiContext::new();
        let ret = global_function_dispatcher_active(&other, &mut ctx, OpCode::Repaint, &[]);
        assert_eq!(ret, FunctionRetval::Done);
    }

    #[test]
    fn quit_pops_help_dialog_when_on_top() {
        let root = MuttWindow::new(
            WindowType::Root,
            crate::window::WindowOrientation::Vertical,
            crate::window::WindowSize::Fixed,
            80,
            24,
        );
        let all_dialogs = MuttWindow::new(
            WindowType::AllDialogs,
            crate::window::WindowOrientation::Vertical,
            crate::window::WindowSize::Maximise,
            0,
            0,
        );
        MuttWindow::add_child(&root, Rc::clone(&all_dialogs));

        let dialog = MuttWindow::new(
            WindowType::DlgIndex,
            crate::window::WindowOrientation::Vertical,
            crate::window::WindowSize::Maximise,
            0,
            0,
        );
        let help = MuttWindow::new(
            WindowType::DlgHelp,
            crate::window::WindowOrientation::Vertical,
            crate::window::WindowSize::Maximise,
            0,
            0,
        );
        crate::dialog::dialog_stack_push(&all_dialogs, Rc::clone(&dialog), true);
        crate::dialog::dialog_stack_push(&all_dialogs, Rc::clone(&help), true);

        let mut ctx = GuiContext::new();
        let ret = global_function_dispatcher_rc(&all_dialogs, &mut ctx, OpCode::Quit, &[]);
        assert_eq!(ret, FunctionRetval::Done);
        assert_eq!(all_dialogs.borrow().children.len(), 1);
        let borrowed = all_dialogs.borrow();
        let top = borrowed.children.last().unwrap().borrow();
        assert_eq!(top.window_type, WindowType::DlgIndex);
    }

    #[test]
    fn help_opens_dialog_when_available() {
        let root = MuttWindow::new(
            WindowType::Root,
            crate::window::WindowOrientation::Vertical,
            crate::window::WindowSize::Fixed,
            80,
            24,
        );
        let all_dialogs = MuttWindow::new(
            WindowType::AllDialogs,
            crate::window::WindowOrientation::Vertical,
            crate::window::WindowSize::Maximise,
            0,
            0,
        );
        let child = MuttWindow::new(
            WindowType::Container,
            crate::window::WindowOrientation::Vertical,
            crate::window::WindowSize::Fixed,
            10,
            10,
        );
        MuttWindow::add_child(&root, Rc::clone(&all_dialogs));
        MuttWindow::add_child(&root, Rc::clone(&child));

        let mut ctx = GuiContext::new();
        let ret = global_function_dispatcher_rc(&child, &mut ctx, OpCode::Help, &[]);
        assert_eq!(ret, FunctionRetval::Done);
        assert_eq!(all_dialogs.borrow().children.len(), 1);
        assert_eq!(
            all_dialogs.borrow().children[0].borrow().window_type,
            WindowType::DlgHelp
        );
        assert!(!child.borrow().state.visible);
    }

    #[test]
    fn help_closes_when_on_help_dialog() {
        let root = MuttWindow::new(
            WindowType::Root,
            crate::window::WindowOrientation::Vertical,
            crate::window::WindowSize::Fixed,
            80,
            24,
        );
        let all_dialogs = MuttWindow::new(
            WindowType::AllDialogs,
            crate::window::WindowOrientation::Vertical,
            crate::window::WindowSize::Maximise,
            0,
            0,
        );
        let help = MuttWindow::new(
            WindowType::DlgHelp,
            crate::window::WindowOrientation::Vertical,
            crate::window::WindowSize::Maximise,
            0,
            0,
        );
        MuttWindow::add_child(&root, Rc::clone(&all_dialogs));
        crate::dialog::dialog_stack_push(&all_dialogs, Rc::clone(&help), true);

        let mut ctx = GuiContext::new();
        let ret = global_function_dispatcher_rc(&help, &mut ctx, OpCode::Help, &[]);
        assert_eq!(ret, FunctionRetval::Done);
        assert!(all_dialogs.borrow().children.is_empty());
    }

    #[test]
    fn quit_without_root_aborts() {
        let win = MuttWindow::new(
            WindowType::Container,
            crate::window::WindowOrientation::Vertical,
            crate::window::WindowSize::Fixed,
            10,
            10,
        );
        let mut ctx = GuiContext::new();
        let ret = global_function_dispatcher_rc(&win, &mut ctx, OpCode::Quit, &[]);
        assert_eq!(ret, FunctionRetval::Abort);
    }
}
