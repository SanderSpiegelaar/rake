use std::ops::Range;

use gpui::{
    App, Bounds, ClipboardItem, Context, ElementInputHandler, EntityInputHandler, EventEmitter,
    FocusHandle, KeyBinding, MouseButton, Pixels, Point, ShapedLine, SharedString, TextRun,
    UTF16Selection, UnderlineStyle, Window, actions, canvas, div, fill, point, prelude::*, px, rgb,
    rgba, size,
};

actions!(
    input,
    [
        Backspace,
        Delete,
        Left,
        Right,
        Up,
        Down,
        SelectLeft,
        SelectRight,
        SelectUp,
        SelectDown,
        SelectAll,
        Home,
        End,
        Paste,
        Cut,
        Copy,
        Newline
    ]
);

pub struct Changed;

pub struct TextInput {
    pub content: SharedString,
    focus: FocusHandle,
    multiline: bool,
    selection: Range<usize>,
    reversed: bool,
    marked: Option<Range<usize>>,
    selecting: bool,
    lines: Vec<ShapedLine>,
    starts: Vec<usize>,
    bounds: Bounds<Pixels>,
    scroll: Point<Pixels>,
}

impl EventEmitter<Changed> for TextInput {}

pub fn bind_keys(cx: &mut App) {
    cx.bind_keys([
        KeyBinding::new("backspace", Backspace, Some("TextInput")),
        KeyBinding::new("delete", Delete, Some("TextInput")),
        KeyBinding::new("left", Left, Some("TextInput")),
        KeyBinding::new("right", Right, Some("TextInput")),
        KeyBinding::new("shift-left", SelectLeft, Some("TextInput")),
        KeyBinding::new("shift-right", SelectRight, Some("TextInput")),
        KeyBinding::new("up", Up, Some("TextInput")),
        KeyBinding::new("down", Down, Some("TextInput")),
        KeyBinding::new("shift-up", SelectUp, Some("TextInput")),
        KeyBinding::new("shift-down", SelectDown, Some("TextInput")),
        KeyBinding::new("cmd-a", SelectAll, Some("TextInput")),
        KeyBinding::new("cmd-v", Paste, Some("TextInput")),
        KeyBinding::new("cmd-c", Copy, Some("TextInput")),
        KeyBinding::new("cmd-x", Cut, Some("TextInput")),
        KeyBinding::new("home", Home, Some("TextInput")),
        KeyBinding::new("cmd-left", Home, Some("TextInput")),
        KeyBinding::new("end", End, Some("TextInput")),
        KeyBinding::new("cmd-right", End, Some("TextInput")),
        KeyBinding::new("enter", Newline, Some("TextInput")),
    ]);
}

impl TextInput {
    pub fn new(content: String, multiline: bool, cx: &mut Context<Self>) -> Self {
        Self {
            content: content.into(),
            focus: cx.focus_handle().tab_stop(true),
            multiline,
            selection: 0..0,
            reversed: false,
            marked: None,
            selecting: false,
            lines: Vec::new(),
            starts: Vec::new(),
            bounds: Bounds::default(),
            scroll: point(px(0.), px(0.)),
        }
    }

    fn cursor(&self) -> usize {
        if self.reversed {
            self.selection.start
        } else {
            self.selection.end
        }
    }

    fn select(&mut self, offset: usize, extend: bool, window: &Window, cx: &mut Context<Self>) {
        let anchor = if !extend {
            offset
        } else if self.reversed {
            self.selection.end
        } else {
            self.selection.start
        };
        self.selection = anchor.min(offset)..anchor.max(offset);
        self.reversed = offset < anchor;
        window.notify_text_selection_changed(&self.focus);
        cx.notify();
    }

    fn previous(&self) -> usize {
        self.content[..self.cursor()]
            .char_indices()
            .next_back()
            .map_or(0, |(i, _)| i)
    }

    fn next(&self) -> usize {
        self.cursor()
            + self.content[self.cursor()..]
                .chars()
                .next()
                .map_or(0, char::len_utf8)
    }

    fn left(&mut self, _: &Left, window: &mut Window, cx: &mut Context<Self>) {
        let offset = if self.selection.is_empty() {
            self.previous()
        } else {
            self.selection.start
        };
        self.select(offset, false, window, cx);
    }

    fn right(&mut self, _: &Right, window: &mut Window, cx: &mut Context<Self>) {
        let offset = if self.selection.is_empty() {
            self.next()
        } else {
            self.selection.end
        };
        self.select(offset, false, window, cx);
    }

    fn select_left(&mut self, _: &SelectLeft, window: &mut Window, cx: &mut Context<Self>) {
        self.select(self.previous(), true, window, cx);
    }

    fn select_right(&mut self, _: &SelectRight, window: &mut Window, cx: &mut Context<Self>) {
        self.select(self.next(), true, window, cx);
    }

    fn vertical(&self, down: bool) -> usize {
        let position = self.cursor_position(self.cursor());
        let row = (position.y / px(22.)) as usize;
        let target = if down { row + 1 } else { row.wrapping_sub(1) };
        match self.lines.get(target) {
            Some(line) => self.starts[target] + line.closest_index_for_x(position.x),
            None if down => self.content.len(),
            None => 0,
        }
    }

    fn up(&mut self, _: &Up, window: &mut Window, cx: &mut Context<Self>) {
        self.select(self.vertical(false), false, window, cx);
    }

    fn down(&mut self, _: &Down, window: &mut Window, cx: &mut Context<Self>) {
        self.select(self.vertical(true), false, window, cx);
    }

    fn select_up(&mut self, _: &SelectUp, window: &mut Window, cx: &mut Context<Self>) {
        self.select(self.vertical(false), true, window, cx);
    }

    fn select_down(&mut self, _: &SelectDown, window: &mut Window, cx: &mut Context<Self>) {
        self.select(self.vertical(true), true, window, cx);
    }

    fn select_all(&mut self, _: &SelectAll, window: &mut Window, cx: &mut Context<Self>) {
        self.select(0, false, window, cx);
        self.select(self.content.len(), true, window, cx);
    }

    fn home(&mut self, _: &Home, window: &mut Window, cx: &mut Context<Self>) {
        self.select(0, false, window, cx);
    }

    fn end(&mut self, _: &End, window: &mut Window, cx: &mut Context<Self>) {
        self.select(self.content.len(), false, window, cx);
    }

    fn backspace(&mut self, _: &Backspace, window: &mut Window, cx: &mut Context<Self>) {
        if self.selection.is_empty() {
            self.select(self.previous(), true, window, cx);
        }
        self.replace_text_in_range(None, "", window, cx);
    }

    fn delete(&mut self, _: &Delete, window: &mut Window, cx: &mut Context<Self>) {
        if self.selection.is_empty() {
            self.select(self.next(), true, window, cx);
        }
        self.replace_text_in_range(None, "", window, cx);
    }

    fn newline(&mut self, _: &Newline, window: &mut Window, cx: &mut Context<Self>) {
        if self.multiline {
            self.replace_text_in_range(None, "\n", window, cx);
        }
    }

    fn paste(&mut self, _: &Paste, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(text) = cx.read_from_clipboard().and_then(|item| item.text()) {
            self.replace_text_in_range(None, &text, window, cx);
        }
    }

    fn copy(&mut self, _: &Copy, _: &mut Window, cx: &mut Context<Self>) {
        if !self.selection.is_empty() {
            cx.write_to_clipboard(ClipboardItem::new_string(
                self.content[self.selection.clone()].into(),
            ));
        }
    }

    fn cut(&mut self, _: &Cut, window: &mut Window, cx: &mut Context<Self>) {
        self.copy(&Copy, window, cx);
        self.replace_text_in_range(None, "", window, cx);
    }

    fn offset_from_utf16(&self, offset: usize) -> usize {
        let mut units = 0;
        self.content
            .char_indices()
            .find_map(|(i, ch)| {
                if units >= offset {
                    Some(i)
                } else {
                    units += ch.len_utf16();
                    None
                }
            })
            .unwrap_or(self.content.len())
    }

    fn to_utf16(&self, offset: usize) -> usize {
        self.content[..offset].encode_utf16().count()
    }

    fn byte_range(&self, range: Range<usize>) -> Range<usize> {
        self.offset_from_utf16(range.start)..self.offset_from_utf16(range.end)
    }

    fn utf16_range(&self, range: Range<usize>) -> Range<usize> {
        self.to_utf16(range.start)..self.to_utf16(range.end)
    }

    fn index_at(&self, position: Point<Pixels>) -> usize {
        let row = ((position.y - self.bounds.top() + self.scroll.y) / px(22.))
            .floor()
            .max(0.) as usize;
        let row = row.min(self.lines.len().saturating_sub(1));
        self.lines.get(row).map_or(0, |line| {
            self.starts[row]
                + line.closest_index_for_x(position.x - self.bounds.left() + self.scroll.x)
        })
    }

    fn cursor_position(&self, offset: usize) -> Point<Pixels> {
        let row = self
            .starts
            .partition_point(|start| *start <= offset)
            .saturating_sub(1);
        let x = self
            .lines
            .get(row)
            .map_or(px(0.), |line| line.x_for_index(offset - self.starts[row]));
        point(x, px(row as f32 * 22.))
    }

    fn layout(&mut self, bounds: Bounds<Pixels>, window: &mut Window) {
        self.bounds = bounds;
        self.lines.clear();
        self.starts.clear();
        let style = window.text_style();
        let mut start = 0;
        for text in self.content.split('\n') {
            self.starts.push(start);
            let run = TextRun {
                len: text.len(),
                font: style.font(),
                color: style.color,
                background_color: None,
                underline: None,
                strikethrough: None,
            };
            let mut runs = vec![run.clone()];
            if let Some(marked) = &self.marked {
                let first = marked.start.saturating_sub(start).min(text.len());
                let last = marked.end.saturating_sub(start).min(text.len());
                runs = vec![
                    TextRun {
                        len: first,
                        ..run.clone()
                    },
                    TextRun {
                        len: last - first,
                        underline: Some(UnderlineStyle {
                            color: Some(style.color),
                            thickness: px(1.),
                            wavy: false,
                        }),
                        ..run.clone()
                    },
                    TextRun {
                        len: text.len() - last,
                        ..run
                    },
                ];
                runs.retain(|run| run.len > 0);
            }
            self.lines.push(window.text_system().shape_line(
                text.to_owned().into(),
                style.font_size.to_pixels(window.rem_size()),
                &runs,
                None,
            ));
            start += text.len() + 1;
        }
        let cursor = self.cursor_position(self.cursor());
        self.scroll.x = self
            .scroll
            .x
            .min(cursor.x)
            .max(cursor.x - bounds.size.width + px(2.))
            .max(px(0.));
        self.scroll.y = self
            .scroll
            .y
            .min(cursor.y)
            .max(cursor.y + px(22.) - bounds.size.height)
            .max(px(0.));
    }

    fn paint(&self, window: &mut Window, cx: &mut App) {
        for (row, line) in self.lines.iter().enumerate() {
            let origin =
                self.bounds.origin + point(-self.scroll.x, px(row as f32 * 22.) - self.scroll.y);
            let start = self.starts[row];
            let end = start + line.text.len();
            if self.focus.is_focused(window)
                && self.selection.start <= end
                && self.selection.end > start
            {
                let left = line.x_for_index(
                    self.selection
                        .start
                        .saturating_sub(start)
                        .min(line.text.len()),
                );
                let right = if self.selection.end > end {
                    line.width + px(4.)
                } else {
                    line.x_for_index(self.selection.end - start)
                };
                window.paint_quad(fill(
                    Bounds::new(origin + point(left, px(0.)), size(right - left, px(22.))),
                    rgba(0x007aff35),
                ));
            }
            line.paint(origin, px(22.), gpui::TextAlign::Left, None, window, cx)
                .expect("Cannot paint input text");
        }
        if self.focus.is_focused(window) && self.selection.is_empty() {
            let origin = self.bounds.origin + self.cursor_position(self.cursor()) - self.scroll;
            window.paint_quad(fill(
                Bounds::new(origin, size(px(1.5), px(22.))),
                rgb(0x0066cc),
            ));
        }
    }
}

impl EntityInputHandler for TextInput {
    fn text_for_range(
        &mut self,
        range: Range<usize>,
        actual: &mut Option<Range<usize>>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<String> {
        let range = self.byte_range(range);
        *actual = Some(self.utf16_range(range.clone()));
        Some(self.content[range].into())
    }

    fn selected_text_range(
        &mut self,
        _: bool,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<UTF16Selection> {
        Some(UTF16Selection {
            range: self.utf16_range(self.selection.clone()),
            reversed: self.reversed,
        })
    }

    fn marked_text_range(&self, _: &mut Window, _: &mut Context<Self>) -> Option<Range<usize>> {
        self.marked.clone().map(|range| self.utf16_range(range))
    }

    fn unmark_text(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.marked = None;
        window.notify_text_input_changed(&self.focus);
        cx.notify();
    }

    fn replace_text_in_range(
        &mut self,
        range: Option<Range<usize>>,
        text: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.replace_and_mark_text_in_range(range, text, None, window, cx);
        self.marked = None;
    }

    fn replace_and_mark_text_in_range(
        &mut self,
        range: Option<Range<usize>>,
        text: &str,
        selected: Option<Range<usize>>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let range = range
            .map(|range| self.byte_range(range))
            .or(self.marked.clone())
            .unwrap_or(self.selection.clone());
        let text = text.replace("\r\n", "\n").replace('\r', "\n");
        let text = if self.multiline {
            text
        } else {
            text.replace('\n', " ")
        };
        self.content =
            (self.content[..range.start].to_owned() + &text + &self.content[range.end..]).into();
        self.marked = (!text.is_empty()).then_some(range.start..range.start + text.len());
        let insertion = self.to_utf16(range.start);
        let length = text.encode_utf16().count();
        self.selection = selected
            .map(|range| {
                self.byte_range(
                    insertion + range.start.min(length)..insertion + range.end.min(length),
                )
            })
            .unwrap_or(range.start + text.len()..range.start + text.len());
        self.reversed = false;
        self.lines.clear();
        self.starts.clear();
        window.notify_text_input_changed(&self.focus);
        cx.emit(Changed);
        cx.notify();
    }

    fn bounds_for_range(
        &mut self,
        range: Range<usize>,
        _: Bounds<Pixels>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<Bounds<Pixels>> {
        if self.lines.is_empty() {
            return None;
        }
        let range = self.byte_range(range);
        let row = self
            .starts
            .partition_point(|start| *start <= range.start)
            .saturating_sub(1);
        let end_offset = range.end.min(self.starts[row] + self.lines[row].text.len());
        let start = self.bounds.origin + self.cursor_position(range.start) - self.scroll;
        let end = self.bounds.origin + self.cursor_position(end_offset) - self.scroll
            + point(px(0.), px(22.));
        Some(Bounds::from_corners(start, end))
    }

    fn character_index_for_point(
        &mut self,
        point: Point<Pixels>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<usize> {
        Some(self.to_utf16(self.index_at(point)))
    }
}

impl gpui::Focusable for TextInput {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus.clone()
    }
}

impl Render for TextInput {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let input = cx.entity();
        let paint_input = input.clone();
        div()
            .key_context("TextInput")
            .track_focus(&self.focus)
            .w_full()
            .min_w_0()
            .p_1()
            .border_1()
            .rounded_sm()
            .border_color(if self.focus.is_focused(window) {
                rgb(0x0066cc)
            } else {
                rgb(0xaaaaaa)
            })
            .bg(rgb(0xffffff))
            .cursor_text()
            .overflow_hidden()
            .on_action(cx.listener(Self::left))
            .on_action(cx.listener(Self::right))
            .on_action(cx.listener(Self::select_left))
            .on_action(cx.listener(Self::select_right))
            .on_action(cx.listener(Self::up))
            .on_action(cx.listener(Self::down))
            .on_action(cx.listener(Self::select_up))
            .on_action(cx.listener(Self::select_down))
            .on_action(cx.listener(Self::select_all))
            .on_action(cx.listener(Self::home))
            .on_action(cx.listener(Self::end))
            .on_action(cx.listener(Self::backspace))
            .on_action(cx.listener(Self::delete))
            .on_action(cx.listener(Self::newline))
            .on_action(cx.listener(Self::paste))
            .on_action(cx.listener(Self::copy))
            .on_action(cx.listener(Self::cut))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|input, event: &gpui::MouseDownEvent, window, cx| {
                    window.focus(&input.focus, cx);
                    input.selecting = true;
                    input.select(
                        input.index_at(event.position),
                        event.modifiers.shift,
                        window,
                        cx,
                    );
                }),
            )
            .on_mouse_move(
                cx.listener(|input, event: &gpui::MouseMoveEvent, window, cx| {
                    if input.selecting {
                        input.select(input.index_at(event.position), true, window, cx);
                    }
                }),
            )
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(|input, _, _, _| input.selecting = false),
            )
            .on_mouse_up_out(
                MouseButton::Left,
                cx.listener(|input, _, _, _| input.selecting = false),
            )
            .child(
                canvas(
                    move |bounds, window, cx| {
                        input.update(cx, |input, _| input.layout(bounds, window))
                    },
                    move |bounds, (), window, cx| {
                        let focus = paint_input.read(cx).focus.clone();
                        window.handle_input(
                            &focus,
                            ElementInputHandler::new(bounds, paint_input.clone()),
                            cx,
                        );
                        paint_input.update(cx, |input, cx| input.paint(window, cx));
                    },
                )
                .w_full()
                .h(px(if self.multiline { 66. } else { 22. })),
            )
    }
}
