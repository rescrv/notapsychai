//! Global function dispatch and opcode bindings.
//!
//! This mirrors NeoMutt's global dispatcher and default key bindings in a
//! minimal, Rust-friendly form.

use crossterm::event::KeyCode;
use crossterm::event::KeyEvent;
use crossterm::event::KeyModifiers;
use std::collections::HashMap;

use crate::action::Action;
use crate::agent_dialog::agent_dialog_content_window;
use crate::agent_dialog::agent_dialog_scroll;
use crate::context::GuiContext;
use crate::help_bar::collect_bindings_from_focus;
use crate::help_data::HelpData;
use crate::help_data::HelpItem;
use crate::help_dialog::help_dialog_scroll;
use crate::help_dialog::HelpDialog;
use crate::window::WindowId;
use crate::window::WindowTree;
use crate::window::WindowType;

// -----------------------------------------------------------------------------
// Bindings
// -----------------------------------------------------------------------------

/// Key descriptor for bindings.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Key {
    /// Key code.
    pub code: KeyCode,
    /// Modifiers.
    pub modifiers: KeyModifiers,
}

impl Key {
    /// Creates a new key descriptor.
    pub const fn new(code: KeyCode, modifiers: KeyModifiers) -> Self {
        Self { code, modifiers }
    }
}

/// Builder and storage for key bindings.
#[derive(Debug, Clone, Default)]
pub struct Bindings {
    order: Vec<(Key, Action)>,
    map: HashMap<Key, Action>,
}

impl Bindings {
    /// Creates an empty bindings set.
    pub fn new() -> Self {
        Self {
            order: Vec::new(),
            map: HashMap::new(),
        }
    }

    /// Adds a key binding and returns self for chaining.
    pub fn bind(&mut self, key: Key, action: Action) -> &mut Self {
        self.map.entry(key).or_insert(action);
        self.order.push((key, action));
        self
    }

    /// Adds a character binding.
    pub fn bind_char(&mut self, ch: char, action: Action) -> &mut Self {
        self.bind(key_char(ch), action)
    }

    /// Adds a special key binding.
    pub fn bind_special(&mut self, code: KeyCode, action: Action) -> &mut Self {
        self.bind(key_special(code), action)
    }

    /// Extends bindings from another set.
    pub fn extend(&mut self, other: &Bindings) -> &mut Self {
        for (key, action) in &other.order {
            self.bind(*key, *action);
        }
        self
    }

    /// Extends bindings from a slice.
    pub fn extend_slice(&mut self, bindings: &[(Key, Action)]) -> &mut Self {
        for (key, action) in bindings {
            self.bind(*key, *action);
        }
        self
    }

    /// Creates bindings from a slice.
    pub fn from_slice(bindings: &[(Key, Action)]) -> Self {
        let mut out = Self::new();
        out.extend_slice(bindings);
        out
    }

    /// Returns the bindings in insertion order.
    pub fn iter(&self) -> impl Iterator<Item = &(Key, Action)> {
        self.order.iter()
    }

    /// Returns the bindings as a slice.
    pub fn as_slice(&self) -> &[(Key, Action)] {
        &self.order
    }
}

/// Lookup an opcode for a key event.
pub fn lookup_binding(bindings: &Bindings, event: KeyEvent) -> Option<Action> {
    let direct = Key::new(event.code, event.modifiers);
    if let Some(action) = bindings.map.get(&direct) {
        return Some(*action);
    }
    bindings.iter().find_map(|(key, action)| {
        if key_matches(*key, event) {
            Some(*action)
        } else {
            None
        }
    })
}

fn key_matches(binding: Key, event: KeyEvent) -> bool {
    if binding.code == event.code && binding.modifiers == event.modifiers {
        return true;
    }

    if let (KeyCode::Char(bc), KeyCode::Char(ec)) = (binding.code, event.code) {
        if bc == ec && bc.is_uppercase() {
            let event_mods = event.modifiers;
            if binding.modifiers == KeyModifiers::NONE && event_mods == KeyModifiers::SHIFT {
                return true;
            }
            if binding.modifiers == KeyModifiers::SHIFT && event_mods == KeyModifiers::NONE {
                return true;
            }
        }
    }

    false
}

const fn key_char(ch: char) -> Key {
    Key::new(KeyCode::Char(ch), KeyModifiers::NONE)
}

const fn key_special(code: KeyCode) -> Key {
    Key::new(code, KeyModifiers::NONE)
}

/// Default generic bindings (available everywhere).
pub fn generic_default_bindings() -> Bindings {
    let mut bindings = Bindings::new();
    bindings
        .bind_char(':', Action::EnterCommand)
        .bind_char('?', Action::Help)
        .bind_char('[', Action::HalfUp)
        .bind_char(']', Action::HalfDown)
        .bind_char('*', Action::LastEntry)
        .bind_char('/', Action::Search)
        .bind_char('<', Action::PrevLine)
        .bind_char('=', Action::FirstEntry)
        .bind_char('>', Action::NextLine)
        .bind_char('a', Action::Agent)
        .bind_char('H', Action::TopPage)
        .bind_char('j', Action::NextEntry)
        .bind_char('J', Action::SidebarNext)
        .bind_char('k', Action::PrevEntry)
        .bind_char('K', Action::SidebarPrev)
        .bind_char('L', Action::BottomPage)
        .bind_char('m', Action::Mail)
        .bind_char('y', Action::YankMessage)
        .bind_char('z', Action::NextPage)
        .bind_char('Z', Action::PrevPage)
        .bind_special(KeyCode::Up, Action::PrevEntry)
        .bind_special(KeyCode::Down, Action::NextEntry)
        .bind_special(KeyCode::Left, Action::PrevPage)
        .bind_special(KeyCode::Right, Action::NextPage)
        .bind_special(KeyCode::Home, Action::FirstEntry)
        .bind_special(KeyCode::End, Action::LastEntry)
        .bind_special(KeyCode::PageUp, Action::PrevPage)
        .bind_special(KeyCode::PageDown, Action::NextPage);
    bindings
}

/// Default dialog bindings.
pub fn dialog_default_bindings() -> Bindings {
    let mut bindings = Bindings::new();
    bindings
        .bind_char('q', Action::Quit)
        .bind_special(KeyCode::Esc, Action::Quit);
    bindings
}

/// Builds help data from a list of key bindings.
pub fn help_data_from_bindings(bindings: &[(Key, Action)]) -> HelpData {
    let mut items = Vec::new();
    for (key, action) in bindings {
        items.push(HelpItem::new(key_to_string(*key), action.description()));
    }
    HelpData::from_items(items)
}

/// Converts a key to its string representation for display.
pub fn key_to_string(key: Key) -> String {
    let mut out = String::new();
    if key.modifiers.contains(KeyModifiers::CONTROL) {
        out.push_str("Ctrl-");
    }
    if key.modifiers.contains(KeyModifiers::ALT) {
        out.push_str("Alt-");
    }
    if key.modifiers.contains(KeyModifiers::SHIFT) {
        out.push_str("Shift-");
    }
    if let KeyCode::Char(ch) = key.code {
        out.push(ch);
        return out;
    }
    match key.code {
        KeyCode::Esc => out.push_str("Esc"),
        KeyCode::Enter => out.push_str("Enter"),
        KeyCode::Tab => out.push_str("Tab"),
        KeyCode::Backspace => out.push_str("Backspace"),
        KeyCode::Left => out.push_str("Left"),
        KeyCode::Right => out.push_str("Right"),
        KeyCode::Up => out.push_str("Up"),
        KeyCode::Down => out.push_str("Down"),
        KeyCode::PageUp => out.push_str("PageUp"),
        KeyCode::PageDown => out.push_str("PageDown"),
        KeyCode::Home => out.push_str("Home"),
        KeyCode::End => out.push_str("End"),
        KeyCode::Insert => out.push_str("Insert"),
        KeyCode::Delete => out.push_str("Delete"),
        KeyCode::F(n) => out.push_str(&format!("F{}", n)),
        _ => out.push_str("Key"),
    };
    out
}

// -----------------------------------------------------------------------------
// Dispatch
// -----------------------------------------------------------------------------

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
pub type GlobalFunction = fn(&mut WindowTree, WindowId, &mut GuiContext, Action) -> FunctionRetval;

/// Opcode -> handler mapping entry.
#[derive(Debug, Clone, Copy)]
pub struct GlobalFunctionEntry {
    /// Action to match.
    pub action: Action,
    /// Handler for the action.
    pub function: GlobalFunction,
}

/// Dispatches an opcode to the matching handler.
pub fn global_function_dispatcher(
    win: WindowId,
    tree: &mut WindowTree,
    ctx: &mut GuiContext,
    action: Action,
    table: &[GlobalFunctionEntry],
) -> FunctionRetval {
    for entry in table {
        if entry.action == action {
            return (entry.function)(tree, win, ctx, action);
        }
    }
    FunctionRetval::Unhandled
}

/// Dispatches an opcode to the matching handler, with help dialog scroll handling.
pub fn global_function_dispatcher_handle_help(
    win: WindowId,
    tree: &mut WindowTree,
    ctx: &mut GuiContext,
    action: Action,
    table: &[GlobalFunctionEntry],
) -> FunctionRetval {
    match action {
        Action::Help => return op_help_shared(tree, win),
        Action::Quit => return op_quit_shared(tree, win),
        _ => {}
    }
    if help_dialog_scroll(tree, win, action) {
        return FunctionRetval::Done;
    }
    if agent_dialog_scroll(tree, win, action) {
        return FunctionRetval::Done;
    }
    global_function_dispatcher(win, tree, ctx, action, table)
}

/// Dispatches to the topmost dialog when present, blocking fallthrough to
/// lower dialogs if the active dialog didn't handle the opcode.
pub fn global_function_dispatcher_active(
    win: WindowId,
    tree: &mut WindowTree,
    ctx: &mut GuiContext,
    action: Action,
    table: &[GlobalFunctionEntry],
) -> FunctionRetval {
    let (target, blocks_fallback) = active_dialog_target(tree, win);
    let ret = global_function_dispatcher_handle_help(target, tree, ctx, action, table);
    if blocks_fallback && matches!(ret, FunctionRetval::Unhandled) {
        return FunctionRetval::Done;
    }
    ret
}

fn active_dialog_target(tree: &WindowTree, win: WindowId) -> (WindowId, bool) {
    let root = tree.get_root(win);
    let Some(all_dialogs) = tree.find_child(root, WindowType::AllDialogs) else {
        return (win, false);
    };
    let top = tree.stack_top(all_dialogs);
    if let Some(top) = top {
        let blocks_fallback = top != win;
        (top, blocks_fallback)
    } else {
        (win, false)
    }
}

fn op_repaint(
    tree: &mut WindowTree,
    win: WindowId,
    _ctx: &mut GuiContext,
    _action: Action,
) -> FunctionRetval {
    tree.get_mut(win).mark_repaint();
    FunctionRetval::Done
}

fn op_abort(
    _tree: &mut WindowTree,
    _win: WindowId,
    _ctx: &mut GuiContext,
    _action: Action,
) -> FunctionRetval {
    FunctionRetval::Abort
}

fn op_help(
    tree: &mut WindowTree,
    win: WindowId,
    _ctx: &mut GuiContext,
    _action: Action,
) -> FunctionRetval {
    op_help_shared(tree, win)
}

/// Collects bindings from a borrowed Window and its ancestors.
fn collect_bindings_from_window(tree: &WindowTree, win: WindowId) -> Vec<(Key, Action)> {
    let mut all_bindings = Vec::new();
    all_bindings.extend(tree.get(win).bindings.iter().copied());

    let mut current_parent = tree.get(win).parent;
    while let Some(parent) = current_parent {
        let borrowed = tree.get(parent);
        all_bindings.extend(borrowed.bindings.iter().copied());
        current_parent = borrowed.parent;
    }
    all_bindings
}

fn op_help_shared(tree: &mut WindowTree, win: WindowId) -> FunctionRetval {
    let window_type = tree.get(win).window_type;
    let root = tree.get_root(win);
    let Some(all_dialogs) = tree.find_child(root, WindowType::AllDialogs) else {
        return FunctionRetval::Unhandled;
    };

    if window_type == WindowType::DlgHelp {
        tree.set_visible(win, false);
        tree.get_mut(win).parent = None;
        let _ = tree.stack_pop(all_dialogs);
        return FunctionRetval::Done;
    }

    let bindings = if tree.is_focused(win) {
        collect_bindings_from_focus(tree, win)
    } else {
        collect_bindings_from_window(tree, win)
    };
    show_help_dialog(tree, bindings, all_dialogs, Some(win))
}

fn show_help_dialog(
    tree: &mut WindowTree,
    mut bindings: Vec<(Key, Action)>,
    all_dialogs: WindowId,
    hide_window: Option<WindowId>,
) -> FunctionRetval {
    // Sort bindings by key string for a cleaner help screen
    bindings.sort_by(|(a_key, _), (b_key, _)| {
        let a_str = key_to_string(*a_key);
        let b_str = key_to_string(*b_key);
        a_str.cmp(&b_str)
    });
    // Deduplicate by key (keep first occurrence after sort)
    bindings.dedup_by(|(a_key, _), (b_key, _)| a_key == b_key);

    let help_data = help_data_from_bindings(&bindings);
    let dialog = HelpDialog::new(tree, &help_data);
    if let Some(win) = hide_window {
        tree.set_visible(win, false);
    }
    tree.stack_push(all_dialogs, dialog.window_id());
    tree.set_focus(dialog.window_id());
    FunctionRetval::Done
}

fn op_quit(
    tree: &mut WindowTree,
    win: WindowId,
    _ctx: &mut GuiContext,
    _action: Action,
) -> FunctionRetval {
    op_quit_shared(tree, win)
}

fn op_quit_shared(tree: &mut WindowTree, win: WindowId) -> FunctionRetval {
    let root = tree.get_root(win);
    op_quit_inner(tree, root)
}

fn op_quit_inner(tree: &mut WindowTree, root: WindowId) -> FunctionRetval {
    if let Some(all_dialogs) = tree.find_child(root, WindowType::AllDialogs) {
        let top_type = tree
            .stack_top(all_dialogs)
            .map(|top| tree.get(top).window_type);
        // Pop help dialogs with 'q' or Esc, agent dialogs only with Esc
        // (agent dialog handles 'q' as text input, so this only triggers for Esc)
        if matches!(top_type, Some(WindowType::DlgHelp | WindowType::DlgAgent)) {
            if matches!(top_type, Some(WindowType::DlgAgent)) {
                let _ = tree.stack_demote_top(all_dialogs);
            } else {
                let _ = tree.stack_pop(all_dialogs);
            }
            let next_focus = tree.stack_top(all_dialogs).unwrap_or(root);
            let focus_target = if tree.get(next_focus).window_type == WindowType::DlgAgent {
                agent_dialog_content_window(tree, next_focus).unwrap_or(next_focus)
            } else {
                next_focus
            };
            tree.set_focus(focus_target);
            return FunctionRetval::Done;
        }
    }
    FunctionRetval::Abort
}

/// Default global functions.
pub fn default_global_functions() -> &'static [GlobalFunctionEntry] {
    static FUNCTIONS: [GlobalFunctionEntry; 4] = [
        GlobalFunctionEntry {
            action: Action::Repaint,
            function: op_repaint,
        },
        GlobalFunctionEntry {
            action: Action::Abort,
            function: op_abort,
        },
        GlobalFunctionEntry {
            action: Action::Help,
            function: op_help,
        },
        GlobalFunctionEntry {
            action: Action::Quit,
            function: op_quit,
        },
    ];
    &FUNCTIONS
}

#[cfg(test)]
mod tests {
    use super::*;

    // Bindings tests
    #[test]
    fn key_matches_shift_normalizes_uppercase() {
        let binding = Key::new(KeyCode::Char('A'), KeyModifiers::NONE);
        let event = KeyEvent::new(KeyCode::Char('A'), KeyModifiers::SHIFT);
        assert!(key_matches(binding, event));
    }

    #[test]
    fn key_matches_lowercase_shift_does_not_match() {
        let binding = Key::new(KeyCode::Char('a'), KeyModifiers::NONE);
        let event = KeyEvent::new(KeyCode::Char('a'), KeyModifiers::SHIFT);
        assert!(!key_matches(binding, event));
    }

    #[test]
    fn lookup_binding_empty_list_returns_none() {
        let event = KeyEvent::new(KeyCode::Char('q'), KeyModifiers::NONE);
        let bindings = Bindings::new();
        assert!(lookup_binding(&bindings, event).is_none());
    }

    #[test]
    fn lookup_binding_first_match_wins() {
        let bindings = [
            (
                Key::new(KeyCode::Char('q'), KeyModifiers::NONE),
                Action::Quit,
            ),
            (
                Key::new(KeyCode::Char('q'), KeyModifiers::NONE),
                Action::Help,
            ),
        ];
        let event = KeyEvent::new(KeyCode::Char('q'), KeyModifiers::NONE);
        let op = lookup_binding(&Bindings::from_slice(&bindings), event);
        assert_eq!(op, Some(Action::Quit));
    }

    #[test]
    fn key_matches_combined_modifiers() {
        let binding = Key::new(
            KeyCode::Char('a'),
            KeyModifiers::CONTROL | KeyModifiers::SHIFT,
        );
        let event = KeyEvent::new(
            KeyCode::Char('a'),
            KeyModifiers::CONTROL | KeyModifiers::SHIFT,
        );
        assert!(key_matches(binding, event));
    }

    #[test]
    fn key_to_string_variants() {
        let key = Key::new(KeyCode::Insert, KeyModifiers::NONE);
        assert_eq!(key_to_string(key), "Insert");
        let key = Key::new(KeyCode::Delete, KeyModifiers::NONE);
        assert_eq!(key_to_string(key), "Delete");
        let key = Key::new(KeyCode::F(2), KeyModifiers::NONE);
        assert_eq!(key_to_string(key), "F2");
    }

    #[test]
    fn help_data_from_bindings_includes_special_keys() {
        let bindings = [
            (Key::new(KeyCode::F(1), KeyModifiers::NONE), Action::Help),
            (Key::new(KeyCode::F(12), KeyModifiers::NONE), Action::Help),
            (Key::new(KeyCode::Insert, KeyModifiers::NONE), Action::Help),
            (Key::new(KeyCode::Delete, KeyModifiers::NONE), Action::Help),
            (Key::new(KeyCode::Home, KeyModifiers::NONE), Action::Help),
        ];

        let help = help_data_from_bindings(&bindings);
        let keys: Vec<_> = help.items.iter().map(|item| item.key.as_str()).collect();
        assert_eq!(keys, ["F1", "F12", "Insert", "Delete", "Home"]);
    }

    // Dispatch tests
    #[test]
    fn global_dispatcher_empty_table_returns_unhandled() {
        let mut tree = WindowTree::new();
        let win = tree.add_window(
            WindowType::Root,
            crate::window::WindowOrientation::Vertical,
            crate::window::WindowSize::Fixed,
            80,
            24,
        );
        let mut ctx = GuiContext::new();
        let ret = global_function_dispatcher(win, &mut tree, &mut ctx, Action::Repaint, &[]);
        assert_eq!(ret, FunctionRetval::Unhandled);
    }

    #[test]
    fn global_dispatcher_active_without_all_dialogs() {
        let mut tree = WindowTree::new();
        let win = tree.add_window(
            WindowType::Root,
            crate::window::WindowOrientation::Vertical,
            crate::window::WindowSize::Fixed,
            80,
            24,
        );
        let mut ctx = GuiContext::new();
        let ret = global_function_dispatcher_active(win, &mut tree, &mut ctx, Action::Repaint, &[]);
        assert_eq!(ret, FunctionRetval::Unhandled);
    }

    #[test]
    fn global_dispatcher_active_blocks_fallback() {
        let mut tree = WindowTree::new();
        let root = tree.add_window(
            WindowType::Root,
            crate::window::WindowOrientation::Vertical,
            crate::window::WindowSize::Fixed,
            80,
            24,
        );
        let all_dialogs = tree.add_window(
            WindowType::AllDialogs,
            crate::window::WindowOrientation::Vertical,
            crate::window::WindowSize::Maximise,
            0,
            0,
        );
        let other = tree.add_window(
            WindowType::Container,
            crate::window::WindowOrientation::Vertical,
            crate::window::WindowSize::Fixed,
            10,
            10,
        );
        let dialog = tree.add_dialog(WindowType::DlgHelp);

        tree.add_child(root, all_dialogs);
        tree.add_child(root, other);
        tree.stack_push(all_dialogs, dialog);

        let mut ctx = GuiContext::new();
        let ret =
            global_function_dispatcher_active(other, &mut tree, &mut ctx, Action::Repaint, &[]);
        assert_eq!(ret, FunctionRetval::Done);
    }

    #[test]
    fn quit_pops_help_dialog_when_on_top() {
        let mut tree = WindowTree::new();
        let root = tree.add_window(
            WindowType::Root,
            crate::window::WindowOrientation::Vertical,
            crate::window::WindowSize::Fixed,
            80,
            24,
        );
        let all_dialogs = tree.add_window(
            WindowType::AllDialogs,
            crate::window::WindowOrientation::Vertical,
            crate::window::WindowSize::Maximise,
            0,
            0,
        );
        tree.add_child(root, all_dialogs);

        let dialog = tree.add_dialog(WindowType::DlgIndex);
        let help = tree.add_dialog(WindowType::DlgHelp);
        tree.stack_push(all_dialogs, dialog);
        tree.stack_push(all_dialogs, help);

        let mut ctx = GuiContext::new();
        let ret = global_function_dispatcher_handle_help(
            all_dialogs,
            &mut tree,
            &mut ctx,
            Action::Quit,
            &[],
        );
        assert_eq!(ret, FunctionRetval::Done);
        assert_eq!(tree.get(all_dialogs).children.len(), 1);
        let top = tree.get(all_dialogs).children.last().copied().unwrap();
        assert_eq!(tree.get(top).window_type, WindowType::DlgIndex);
    }

    #[test]
    fn help_opens_dialog_when_available() {
        let mut tree = WindowTree::new();
        let root = tree.add_window(
            WindowType::Root,
            crate::window::WindowOrientation::Vertical,
            crate::window::WindowSize::Fixed,
            80,
            24,
        );
        let all_dialogs = tree.add_window(
            WindowType::AllDialogs,
            crate::window::WindowOrientation::Vertical,
            crate::window::WindowSize::Maximise,
            0,
            0,
        );
        let child = tree.add_window(
            WindowType::Container,
            crate::window::WindowOrientation::Vertical,
            crate::window::WindowSize::Fixed,
            10,
            10,
        );
        tree.add_child(root, all_dialogs);
        tree.add_child(root, child);

        let mut ctx = GuiContext::new();
        let ret =
            global_function_dispatcher_handle_help(child, &mut tree, &mut ctx, Action::Help, &[]);
        assert_eq!(ret, FunctionRetval::Done);
        assert_eq!(tree.get(all_dialogs).children.len(), 1);
        let top = tree.get(all_dialogs).children[0];
        assert_eq!(tree.get(top).window_type, WindowType::DlgHelp);
        assert!(!tree.get(child).state.visible);
    }

    #[test]
    fn help_closes_when_on_help_dialog() {
        let mut tree = WindowTree::new();
        let root = tree.add_window(
            WindowType::Root,
            crate::window::WindowOrientation::Vertical,
            crate::window::WindowSize::Fixed,
            80,
            24,
        );
        let all_dialogs = tree.add_window(
            WindowType::AllDialogs,
            crate::window::WindowOrientation::Vertical,
            crate::window::WindowSize::Maximise,
            0,
            0,
        );
        let help = tree.add_dialog(WindowType::DlgHelp);
        tree.add_child(root, all_dialogs);
        tree.stack_push(all_dialogs, help);

        let mut ctx = GuiContext::new();
        let ret =
            global_function_dispatcher_handle_help(help, &mut tree, &mut ctx, Action::Help, &[]);
        assert_eq!(ret, FunctionRetval::Done);
        assert!(tree.get(all_dialogs).children.is_empty());
    }
}
