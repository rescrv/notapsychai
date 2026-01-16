use std::io::Result;
use std::io::Write;

use agent_inbox_protocol::Message;

use crate::context::ColorId;
use crate::context::GuiContext;
use crate::render::Renderer;
use crate::window::CursorBehavior;
use crate::window::RenderMode;
use crate::window::Window;
use crate::window::WindowId;
use crate::window::WindowTree;
use crate::window::WindowWidget;

use unicode_width::UnicodeWidthChar;
use unicode_width::UnicodeWidthStr;

use super::state::tool_display_name;
use super::state::InboxState;
use super::state::RenderData;
use super::state::ToolFormRenderData;
use super::state::ToolOrigin;

const FIELD_CONTINUATION_INDENT: &str = "  ";

/// Helper to find the inbox state in the window tree.
fn find_inbox_state(tree: &WindowTree, start_win: WindowId) -> Option<&InboxState> {
    if let Some(WindowWidget::InboxState(state)) = tree.get(start_win).widget_ref() {
        return Some(state.as_ref());
    }
    if let Some(parent) = tree.get(start_win).parent {
        return find_inbox_state(tree, parent);
    }
    None
}

/// Sidebar widget showing mailbox list.
#[derive(Debug)]
pub struct SidebarWidget;

impl SidebarWidget {
    pub fn update(&mut self, _tree: &mut WindowTree, _win: WindowId) {}

    pub fn render(
        &mut self,
        tree: &mut WindowTree,
        win: WindowId,
        ctx: &mut GuiContext,
        out: &mut dyn Write,
        mode: RenderMode,
    ) -> Result<CursorBehavior> {
        if matches!(mode, RenderMode::CursorOnly) {
            return Ok(Self::cursor_behavior(tree, win).unwrap_or(CursorBehavior::Hidden));
        }
        Self::draw_impl(tree, win, ctx, out)?;
        Ok(Self::cursor_behavior(tree, win).unwrap_or(CursorBehavior::Hidden))
    }

    fn draw_impl(
        tree: &mut WindowTree,
        win: WindowId,
        ctx: &mut GuiContext,
        out: &mut dyn Write,
    ) -> Result<()> {
        if let Some(state) = find_inbox_state(tree, win) {
            if let Some(data) = state.render_data() {
                let window = tree.get(win);
                let mut renderer = Renderer::new(ctx, out);
                renderer.draw_list_with_selection(window, 0, |idx| {
                    if idx < data.mailbox_names.len() {
                        let mailbox = &data.mailbox_names[idx];
                        let is_selected = idx == data.selected_mailbox;
                        Some((format!(" {} ", mailbox), is_selected))
                    } else {
                        None
                    }
                })?;
            }
        }
        Ok(())
    }

    fn cursor_behavior(tree: &WindowTree, win: WindowId) -> Option<CursorBehavior> {
        let state = find_inbox_state(tree, win)?;
        let data = state.render_data()?;
        let form = data.tool_form.as_ref()?;
        let window = tree.get(win);
        let (row, col) = tool_form_cursor_position(window, form)?;
        Some(CursorBehavior::Positioned {
            row,
            col,
            state: crate::window::CursorState::Visible,
        })
    }
}

/// Index widget showing message list.
#[derive(Debug)]
pub struct IndexWidget;

impl IndexWidget {
    pub fn update(&mut self, _tree: &mut WindowTree, _win: WindowId) {}

    pub fn render(
        &mut self,
        tree: &mut WindowTree,
        win: WindowId,
        ctx: &mut GuiContext,
        out: &mut dyn Write,
        mode: RenderMode,
    ) -> Result<CursorBehavior> {
        if matches!(mode, RenderMode::CursorOnly) {
            return Ok(CursorBehavior::Hidden);
        }
        Self::draw_impl(tree, win, ctx, out)?;
        Ok(CursorBehavior::Hidden)
    }

    fn draw_impl(
        tree: &mut WindowTree,
        win: WindowId,
        ctx: &mut GuiContext,
        out: &mut dyn Write,
    ) -> Result<()> {
        if let Some(state) = find_inbox_state(tree, win) {
            if let Some(data) = state.render_data() {
                let window = tree.get(win);
                let mut renderer = Renderer::new(ctx, out);
                renderer.draw_list_with_selection(window, data.scroll_offset, |idx| {
                    if idx < data.messages.len() {
                        let msg = &data.messages[idx];
                        let is_selected = idx == data.selected_message;
                        let date_str = msg.date.format("%Y-%m-%d").to_string();
                        let line = format!(
                            "{} | {:20} | {}",
                            date_str,
                            msg.from.as_str(),
                            msg.body.as_str()
                        );
                        Some((line, is_selected))
                    } else {
                        None
                    }
                })?;
            }
        }
        Ok(())
    }
}

/// Pager widget showing message content.
#[derive(Debug)]
pub struct PagerWidget;

impl PagerWidget {
    pub fn update(&mut self, _tree: &mut WindowTree, _win: WindowId) {}

    pub fn render(
        &mut self,
        tree: &mut WindowTree,
        win: WindowId,
        ctx: &mut GuiContext,
        out: &mut dyn Write,
        mode: RenderMode,
    ) -> Result<CursorBehavior> {
        if matches!(mode, RenderMode::CursorOnly) {
            return Ok(CursorBehavior::Hidden);
        }
        Self::draw_impl(tree, win, ctx, out)?;
        Ok(CursorBehavior::Hidden)
    }

    fn draw_impl(
        tree: &mut WindowTree,
        win: WindowId,
        ctx: &mut GuiContext,
        out: &mut dyn Write,
    ) -> Result<()> {
        if let Some(state) = find_inbox_state(tree, win) {
            if let Some(data) = state.render_data() {
                let window = tree.get(win);
                ctx.set_color_by_id(out, ColorId::Normal)?;

                if data.messages.is_empty() {
                    let mut renderer = Renderer::new(ctx, out);
                    for row in 0i16..window.state.rect.size.rows {
                        renderer.move_cursor(window, row, 0)?;
                        renderer.write_padded(window, "")?;
                    }
                    return Ok(());
                }

                let message = &data.messages[data.selected_message];
                let origins = data.selected_message_origins.as_deref();

                if let Some(form) = &data.tool_form {
                    return draw_tool_form(ctx, out, window, message, form, origins);
                }

                let lines = build_message_lines(message, origins);

                let mut renderer = Renderer::new(ctx, out);
                for row in 0i16..window.state.rect.size.rows {
                    renderer.move_cursor(window, row, 0)?;

                    if (row as usize) < lines.len() {
                        let line = &lines[row as usize];
                        renderer.write_padded(window, line)?;
                    } else {
                        renderer.write_padded(window, "")?;
                    }
                }
            }
        }
        Ok(())
    }
}

/// Index status bar widget.
#[derive(Debug)]
pub struct IndexBarWidget;

impl IndexBarWidget {
    pub fn update(&mut self, _tree: &mut WindowTree, _win: WindowId) {}

    pub fn render(
        &mut self,
        tree: &mut WindowTree,
        win: WindowId,
        ctx: &mut GuiContext,
        out: &mut dyn Write,
        mode: RenderMode,
    ) -> Result<CursorBehavior> {
        render_status_bar(tree, win, ctx, out, mode, |data| {
            let mailbox_name = &data.mailbox_names[data.selected_mailbox];
            let msg_count = data.messages.len();
            let filter_status = if let Some(ref pat) = data.search_pattern {
                format!(" [Search: {}]", pat)
            } else {
                String::new()
            };
            if msg_count > 0 {
                format!(
                    " {} [{}/{}] {} ",
                    mailbox_name,
                    data.selected_message + 1,
                    msg_count,
                    filter_status
                )
            } else {
                format!(" {} [empty]{} ", mailbox_name, filter_status)
            }
        })
    }
}

/// Pager status bar widget.
#[derive(Debug)]
pub struct PagerBarWidget;

impl PagerBarWidget {
    pub fn update(&mut self, _tree: &mut WindowTree, _win: WindowId) {}

    pub fn render(
        &mut self,
        tree: &mut WindowTree,
        win: WindowId,
        ctx: &mut GuiContext,
        out: &mut dyn Write,
        mode: RenderMode,
    ) -> Result<CursorBehavior> {
        render_status_bar(tree, win, ctx, out, mode, |data| {
            if let Some(ref tool_name) = data.tool_confirm_name {
                format!(" Execute '{}'? (y=yes, n/Esc=cancel) ", tool_name)
            } else if let Some(form) = &data.tool_form {
                format!(" Tool: {} ", form.tool_name)
            } else if !data.messages.is_empty() {
                format!(
                    " Message: {} ",
                    data.messages[data.selected_message].body.as_str()
                )
            } else {
                " No messages ".to_string()
            }
        })
    }
}

/// Message window widget showing status messages.
#[derive(Debug)]
pub struct MessageWidget;

impl MessageWidget {
    pub fn update(&mut self, _tree: &mut WindowTree, _win: WindowId) {}

    pub fn render(
        &mut self,
        tree: &mut WindowTree,
        win: WindowId,
        ctx: &mut GuiContext,
        out: &mut dyn Write,
        mode: RenderMode,
    ) -> Result<CursorBehavior> {
        if matches!(mode, RenderMode::CursorOnly) {
            return Ok(CursorBehavior::Hidden);
        }
        Self::draw_impl(tree, win, ctx, out)?;
        Ok(CursorBehavior::Hidden)
    }

    fn draw_impl(
        tree: &mut WindowTree,
        win: WindowId,
        ctx: &mut GuiContext,
        out: &mut dyn Write,
    ) -> Result<()> {
        if let Some(state) = find_inbox_state(tree, win) {
            if let Some(data) = state.render_data() {
                let window = tree.get(win);
                ctx.set_color_by_id(out, ColorId::Normal)?;
                {
                    let mut renderer = Renderer::new(ctx, out);
                    renderer.move_cursor(window, 0, 0)?;
                    renderer.write_padded(window, &data.status_message)?;
                }
            }
        }
        Ok(())
    }
}

fn build_message_lines(message: &Message, origins: Option<&[ToolOrigin]>) -> Vec<String> {
    let mut lines = vec![
        format!("From: {}", message.from.as_str()),
        format!("Date: {}", message.date.format("%Y-%m-%d %H:%M:%S")),
    ];

    if !message.tools.is_empty() {
        lines.push("Actions:".to_string());
        for (idx, tool) in message.tools.iter().enumerate() {
            let origin = origins.and_then(|list| list.get(idx));
            let display_name = tool_display_name(&tool.name, origin);
            lines.push(format!("- [{}] {}", idx + 1, display_name));
        }
    }

    lines.push(String::new());
    lines.push(message.body.as_str().to_string());
    lines
}

fn render_status_bar<F>(
    tree: &mut WindowTree,
    win: WindowId,
    ctx: &mut GuiContext,
    out: &mut dyn Write,
    mode: RenderMode,
    title_fn: F,
) -> Result<CursorBehavior>
where
    F: FnOnce(&RenderData) -> String,
{
    if matches!(mode, RenderMode::CursorOnly) {
        return Ok(CursorBehavior::Hidden);
    }
    if let Some(state) = find_inbox_state(tree, win) {
        if let Some(data) = state.render_data() {
            let title = title_fn(&data);
            let window = tree.get(win);
            ctx.set_normal_backed_color_by_id(out, ColorId::Status)?;
            {
                let mut renderer = Renderer::new(ctx, out);
                renderer.move_cursor(window, 0, 0)?;
                renderer.write_padded(window, &title)?;
            }
            ctx.set_color_by_id(out, ColorId::Normal)?;
        }
    }
    Ok(CursorBehavior::Hidden)
}

fn draw_buttons_line(
    ctx: &mut GuiContext,
    out: &mut dyn Write,
    win: &Window,
    row: i16,
    focus_submit: bool,
    focus_cancel: bool,
) -> Result<()> {
    let mut renderer = Renderer::new(ctx, out);
    renderer.move_cursor(win, row, 0)?;
    renderer.set_color_by_id(ColorId::Normal)?;

    let submit_label = "[Submit]";
    let cancel_label = "[Cancel]";
    let gap = "  ";
    let mut used = 0usize;

    if focus_submit {
        renderer.set_color_by_id(ColorId::Indicator)?;
    }
    renderer.write_str(submit_label)?;
    used += submit_label.len();

    renderer.set_color_by_id(ColorId::Normal)?;
    renderer.write_str(gap)?;
    used += gap.len();

    if focus_cancel {
        renderer.set_color_by_id(ColorId::Indicator)?;
    }
    renderer.write_str(cancel_label)?;
    used += cancel_label.len();

    renderer.set_color_by_id(ColorId::Normal)?;
    let cols = win.state.rect.size.cols as usize;
    if cols > used {
        renderer.write_str(&" ".repeat(cols - used))?;
    }
    Ok(())
}

fn draw_tool_form(
    ctx: &mut GuiContext,
    out: &mut dyn Write,
    win: &Window,
    message: &Message,
    form: &ToolFormRenderData,
    origins: Option<&[ToolOrigin]>,
) -> Result<()> {
    let total_rows = win.state.rect.size.rows as usize;
    if total_rows == 0 {
        return Ok(());
    }
    let min_form = 6usize;
    let mut form_height = total_rows / 2;
    if form_height < min_form {
        form_height = min_form.min(total_rows);
    }
    if form_height >= total_rows {
        form_height = total_rows;
    }
    let message_height = total_rows.saturating_sub(form_height);

    let message_lines = build_message_lines(message, origins);
    let mut row = message_height;
    {
        let mut renderer = Renderer::new(ctx, out);
        for row in 0..message_height {
            renderer.move_cursor(win, row as i16, 0)?;
            let line = message_lines.get(row).map(String::as_str).unwrap_or("");
            renderer.write_padded(win, line)?;
        }

        let header = format!("Tool: {}", form.tool_name);
        renderer.move_cursor(win, row as i16, 0)?;
        renderer.write_padded(win, &header)?;
        row += 1;

        if row < total_rows {
            if let Some(desc) = &form.tool_description {
                renderer.move_cursor(win, row as i16, 0)?;
                renderer.write_padded(win, desc)?;
                row += 1;
            }
        }

        if row < total_rows {
            renderer.move_cursor(win, row as i16, 0)?;
            renderer.write_padded(win, "")?;
            row += 1;
        }

        if form.fields.is_empty() && row < total_rows {
            renderer.move_cursor(win, row as i16, 0)?;
            renderer.write_padded(win, "No fields")?;
            row += 1;
        }

        for (idx, field) in form.fields.iter().enumerate() {
            if row >= total_rows {
                break;
            }
            let prefix = format!("{}: ", field.name);
            let cols = win.state.rect.size.cols as usize;
            let lines = wrap_field_lines(&prefix, FIELD_CONTINUATION_INDENT, &field.value, cols);
            for line in lines {
                if row >= total_rows {
                    break;
                }
                renderer.move_cursor(win, row as i16, 0)?;
                if form.focused == idx {
                    renderer.set_color_by_id(ColorId::Indicator)?;
                } else {
                    renderer.set_color_by_id(ColorId::Normal)?;
                }
                renderer.write_padded(win, &line)?;
                row += 1;
            }
            renderer.set_color_by_id(ColorId::Normal)?;
            if let Some(desc) = &field.description {
                if row < total_rows {
                    renderer.move_cursor(win, row as i16, 0)?;
                    let desc_line = format!("  {}", desc);
                    renderer.write_padded(win, &desc_line)?;
                    row += 1;
                }
            }
        }
    }

    let buttons_row = if total_rows > 0 { total_rows - 1 } else { 0 };
    if buttons_row >= message_height {
        let focus_submit = form.focused == form.fields.len();
        let focus_cancel = form.focused == form.fields.len() + 1;
        draw_buttons_line(
            ctx,
            out,
            win,
            buttons_row as i16,
            focus_submit,
            focus_cancel,
        )?;
    }

    {
        let mut renderer = Renderer::new(ctx, out);
        while row < total_rows.saturating_sub(1) {
            renderer.move_cursor(win, row as i16, 0)?;
            renderer.write_padded(win, "")?;
            row += 1;
        }
    }

    Ok(())
}

fn tool_form_cursor_position(win: &Window, form: &ToolFormRenderData) -> Option<(i16, i16)> {
    let total_rows = win.state.rect.size.rows as usize;
    if total_rows == 0 {
        return None;
    }
    if form.focused >= form.fields.len() {
        return None;
    }
    let min_form = 6usize;
    let mut form_height = total_rows / 2;
    if form_height < min_form {
        form_height = min_form.min(total_rows);
    }
    if form_height >= total_rows {
        form_height = total_rows;
    }
    let message_height = total_rows.saturating_sub(form_height);

    let mut row = message_height;
    row += 1; // header
    if row < total_rows && form.tool_description.is_some() {
        row += 1;
    }
    if row < total_rows {
        row += 1; // blank line
    }

    for (idx, field) in form.fields.iter().enumerate() {
        if row >= total_rows {
            return None;
        }
        let prefix = format!("{}: ", field.name);
        let cols = win.state.rect.size.cols as usize;
        let lines = wrap_field_lines(&prefix, FIELD_CONTINUATION_INDENT, &field.value, cols);
        let lines_count = lines.len();
        let last_line = lines.last().map(String::as_str).unwrap_or("");
        if form.focused == idx {
            let mut col = last_line.width();
            if cols > 0 && col >= cols {
                col = cols - 1;
            }
            let focus_row = row + lines_count.saturating_sub(1);
            if focus_row >= total_rows {
                return None;
            }
            return Some((focus_row as i16, col as i16));
        }
        row += lines_count;
        if field.description.is_some() {
            row += 1;
        }
    }
    None
}

fn wrap_field_lines(prefix: &str, indent: &str, value: &str, max_width: usize) -> Vec<String> {
    let mut out = Vec::new();
    if max_width == 0 {
        return out;
    }
    let prefix_width = prefix.width();
    let indent_width = indent.width();
    let mut is_first_line = true;

    for para in value.split('\n') {
        let available = if is_first_line {
            max_width.saturating_sub(prefix_width)
        } else {
            max_width.saturating_sub(indent_width)
        };
        let wrapped = wrap_text_line(para, available);
        let mut had_line = false;
        for line in wrapped {
            let line_prefix = if is_first_line { prefix } else { indent };
            out.push(format!("{}{}", line_prefix, line));
            is_first_line = false;
            had_line = true;
        }
        if !had_line {
            let line_prefix = if is_first_line { prefix } else { indent };
            out.push(format!("{}{}", line_prefix, ""));
            is_first_line = false;
        }
    }

    if out.is_empty() {
        out.push(format!("{}{}", prefix, ""));
    }

    out
}

fn wrap_text_line(line: &str, max_width: usize) -> Vec<String> {
    if max_width == 0 {
        return vec![String::new()];
    }
    if line.is_empty() {
        return vec![String::new()];
    }
    let mut lines = Vec::new();
    let mut current = String::new();
    let mut current_width = 0usize;
    let mut last_space_idx: Option<usize> = None;

    for ch in line.chars() {
        let ch_width = ch.width().unwrap_or(0);
        current.push(ch);
        current_width += ch_width;
        if ch.is_whitespace() {
            last_space_idx = Some(current.len());
        }
        if current_width > max_width && !current.is_empty() {
            if let Some(space_idx) = last_space_idx {
                let mut next = current.split_off(space_idx);
                let line_out = current.trim_end_matches(char::is_whitespace).to_string();
                lines.push(line_out);
                next = next.trim_start_matches(char::is_whitespace).to_string();
                current = next;
            } else {
                let last = current.pop().unwrap();
                lines.push(current);
                current = last.to_string();
            }
            current_width = current.width();
            last_space_idx = None;
        }
    }
    lines.push(current);
    lines
}
