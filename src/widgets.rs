//! Immediate-mode buttons and text fields for Frontier's map and waypoint pages.
//!
//! Sunrise's public widget kit became a retained screen mount in this SDK pin,
//! so Frontier keeps a small local kit with the same look and the same input
//! behaviour those pages already use.

use mod_api::{ClientHost, DrawContext, SoundPlay};

const SHIFT: u32 = 1;
const CTRL: u32 = 2;
const DOUBLE_CLICK_MS: u64 = 250;
const BLINK_MS: u64 = 300;
const TEXT_COLOR: u32 = 0xFFE0E0E0;
const TEXT_INACTIVE: u32 = 0xFF707070;
const FIELD_BG: u32 = 0xFF0A0A0A;
const FIELD_BORDER: u32 = 0xFFA0A0A0;
const FIELD_BORDER_FOCUS: u32 = 0xFF_F9801D;
const BUTTON_BG: u32 = 0xFF2A2A2A;
const BUTTON_HOVER: u32 = 0xFF3A3A3A;
const BUTTON_BORDER: u32 = 0xFF1A1A1A;
const HIGHLIGHT: u32 = 0x66007FFF;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct WidgetId(usize);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Outcome {
    Pressed(WidgetId),
    Submitted(WidgetId),
    Changed(WidgetId),
    Handled,
    Ignored,
}

impl Outcome {
    pub fn handled(self) -> bool {
        !matches!(self, Self::Ignored)
    }
}

pub struct Button {
    pub rect: [f32; 4],
    pub label: String,
    pub active: bool,
}

impl Button {
    pub fn new(rect: [f32; 4], label: impl Into<String>) -> Self {
        Self {
            rect,
            label: label.into(),
            active: true,
        }
    }

    fn active(&self) -> bool {
        self.active
    }
}

struct Edit {
    value: String,
    cursor: usize,
    highlight: usize,
    max_length: usize,
}

impl Edit {
    fn new(max_length: usize) -> Self {
        Self {
            value: String::new(),
            cursor: 0,
            highlight: 0,
            max_length,
        }
    }

    fn set_value(&mut self, value: &str) {
        self.value = truncate_chars(value, self.max_length);
        self.cursor = char_len(&self.value);
        self.highlight = self.cursor;
    }

    fn set_cursor(&mut self, pos: usize, extend: bool) {
        let pos = pos.min(char_len(&self.value));
        self.cursor = pos;
        if !extend {
            self.highlight = pos;
        }
    }

    fn selected_text(&self) -> String {
        let (a, b) = self.range();
        slice_chars(&self.value, a, b)
    }

    fn range(&self) -> (usize, usize) {
        if self.cursor <= self.highlight {
            (self.cursor, self.highlight)
        } else {
            (self.highlight, self.cursor)
        }
    }

    fn insert_text(&mut self, text: &str) {
        let (a, b) = self.range();
        let mut next = String::new();
        next.push_str(&slice_chars(&self.value, 0, a));
        next.push_str(text);
        next.push_str(&slice_chars(&self.value, b, char_len(&self.value)));
        self.value = truncate_chars(&next, self.max_length);
        let inserted = char_len(text).min(self.max_length.saturating_sub(a));
        self.cursor = a + inserted;
        self.highlight = self.cursor;
    }

    fn word_at(&self, pos: usize) -> (usize, usize) {
        let chars: Vec<char> = self.value.chars().collect();
        let n = chars.len();
        let pos = pos.min(n);
        if n == 0 {
            return (0, 0);
        }
        let at = pos.saturating_sub(1).min(n.saturating_sub(1));
        let mut start = at;
        while start > 0 && chars[start - 1].is_alphanumeric() {
            start -= 1;
        }
        let mut end = pos.min(n);
        while end < n && chars[end].is_alphanumeric() {
            end += 1;
        }
        (start, end)
    }

    fn key(&mut self, key: EditKey, _ctrl: bool, shift: bool) -> bool {
        match key {
            EditKey::Backspace => {
                if self.cursor != self.highlight {
                    self.insert_text("");
                } else if self.cursor > 0 {
                    self.set_cursor(self.cursor - 1, false);
                    let (a, _) = (self.cursor, self.cursor + 1);
                    self.highlight = self.cursor + 1;
                    self.cursor = a;
                    self.insert_text("");
                }
                true
            }
            EditKey::Delete => {
                if self.cursor != self.highlight {
                    self.insert_text("");
                } else if self.cursor < char_len(&self.value) {
                    self.highlight = self.cursor + 1;
                    self.insert_text("");
                }
                true
            }
            EditKey::Left => {
                let pos = self.cursor.saturating_sub(1);
                self.set_cursor(pos, shift);
                true
            }
            EditKey::Right => {
                let pos = (self.cursor + 1).min(char_len(&self.value));
                self.set_cursor(pos, shift);
                true
            }
            EditKey::Home => {
                self.set_cursor(0, shift);
                true
            }
            EditKey::End => {
                self.set_cursor(char_len(&self.value), shift);
                true
            }
        }
    }
}

enum EditKey {
    Backspace,
    Delete,
    Left,
    Right,
    Home,
    End,
}

pub struct TextField {
    pub rect: [f32; 4],
    edit: Edit,
    pub active: bool,
    scroll: usize,
    edges: Vec<f32>,
    moved_at: u64,
}

impl TextField {
    pub fn new(rect: [f32; 4], max_length: usize) -> Self {
        Self {
            rect,
            edit: Edit::new(max_length),
            active: true,
            scroll: 0,
            edges: Vec::new(),
            moved_at: 0,
        }
    }

    pub fn value(&self) -> &str {
        &self.edit.value
    }

    pub fn set_value(&mut self, value: &str) {
        self.edit.set_value(value);
        self.scroll_to_cursor();
    }

    fn scroll_to_cursor(&mut self) {
        self.scroll = self.scroll.min(char_len(&self.edit.value));
        if self.edit.cursor < self.scroll {
            self.scroll = self.edit.cursor;
        }
    }

    fn position_at(&self, x: f32) -> usize {
        let x = x.floor();
        let shown = self
            .edges
            .iter()
            .skip(1)
            .take_while(|&&edge| edge <= x)
            .count();
        self.scroll + shown
    }

    fn moved(&mut self, now: u64) {
        self.scroll_to_cursor();
        self.moved_at = now;
    }

    fn active(&self) -> bool {
        self.active
    }
}

enum Widget {
    Button(Button),
    TextField(TextField),
}

impl Widget {
    fn rect(&self) -> [f32; 4] {
        match self {
            Self::Button(b) => b.rect,
            Self::TextField(f) => f.rect,
        }
    }

    fn active(&self) -> bool {
        match self {
            Self::Button(b) => b.active(),
            Self::TextField(f) => f.active(),
        }
    }
}

pub struct Widgets {
    widgets: Vec<Widget>,
    focus: Option<usize>,
    last_press: Option<u64>,
    dragging: bool,
}

impl Widgets {
    pub fn new() -> Self {
        Self {
            widgets: Vec::new(),
            focus: None,
            last_press: None,
            dragging: false,
        }
    }

    pub fn add_button(&mut self, button: Button) -> WidgetId {
        self.widgets.push(Widget::Button(button));
        WidgetId(self.widgets.len() - 1)
    }

    pub fn add_text_field(&mut self, field: TextField) -> WidgetId {
        self.widgets.push(Widget::TextField(field));
        WidgetId(self.widgets.len() - 1)
    }

    pub fn text_field(&self, id: WidgetId) -> Option<&TextField> {
        match self.widgets.get(id.0) {
            Some(Widget::TextField(f)) => Some(f),
            _ => None,
        }
    }

    pub fn set_focus(&mut self, host: &dyn ClientHost, id: Option<WidgetId>) {
        self.focus_index(id.map(|i| i.0), host.millis());
    }

    fn focus_index(&mut self, index: Option<usize>, now: u64) {
        self.focus = index;
        if let Some(i) = index {
            if let Some(Widget::TextField(field)) = self.widgets.get_mut(i) {
                field.moved(now);
            }
        }
    }

    fn active_focus(&self) -> Option<usize> {
        self.focus
            .filter(|&i| self.widgets.get(i).is_some_and(|w| w.active()))
    }

    pub fn draw(
        &mut self,
        draw: &mut dyn DrawContext,
        host: &dyn ClientHost,
        mouse: [f32; 2],
    ) -> Result<(), ()> {
        let now = host.millis();
        for (i, widget) in self.widgets.iter_mut().enumerate() {
            let focused = self.focus == Some(i);
            match widget {
                Widget::Button(button) => draw_button(draw, button, over(button.rect, mouse)),
                Widget::TextField(field) => draw_field(draw, field, focused, now),
            }
        }
        Ok(())
    }

    pub fn mouse_pressed(
        &mut self,
        host: &dyn ClientHost,
        at: [f32; 2],
        button: i32,
        modifiers: u32,
    ) -> Outcome {
        if button != 0 {
            return Outcome::Ignored;
        }
        let now = host.millis();
        let double = self
            .last_press
            .is_some_and(|then| now.saturating_sub(then) < DOUBLE_CLICK_MS);
        let Some(index) = self
            .widgets
            .iter()
            .position(|widget| widget.active() && over(widget.rect(), at))
        else {
            self.focus_index(None, now);
            return Outcome::Ignored;
        };
        self.last_press = Some(now);
        self.focus_index(Some(index), now);
        self.dragging = true;
        match &mut self.widgets[index] {
            Widget::Button(_) => {
                let _ = host.play_local_sound(&SoundPlay::ui("minecraft:ui.button.click"), 0);
                Outcome::Pressed(WidgetId(index))
            }
            Widget::TextField(field) => {
                let pos = field.position_at(at[0]);
                if double {
                    let (start, end) = field.edit.word_at(pos);
                    field.edit.set_cursor(start, false);
                    field.edit.set_cursor(end, true);
                } else {
                    field.edit.set_cursor(pos, modifiers & SHIFT != 0);
                }
                field.moved(now);
                Outcome::Handled
            }
        }
    }

    pub fn mouse_released(&mut self, _button: i32) -> Outcome {
        self.dragging = false;
        Outcome::Ignored
    }

    pub fn mouse_dragged(&mut self, host: &dyn ClientHost, at: [f32; 2], button: i32) -> Outcome {
        if button != 0 || !self.dragging {
            return Outcome::Ignored;
        }
        let Some(index) = self.active_focus() else {
            return Outcome::Ignored;
        };
        let Some(Widget::TextField(field)) = self.widgets.get_mut(index) else {
            return Outcome::Ignored;
        };
        field.edit.set_cursor(field.position_at(at[0]), true);
        field.moved(host.millis());
        Outcome::Handled
    }

    pub fn key_pressed(&mut self, host: &dyn ClientHost, key: &str, modifiers: u32) -> Outcome {
        let now = host.millis();
        let ctrl = modifiers & CTRL != 0;
        let shift = modifiers & SHIFT != 0;
        if key == "key.keyboard.tab" {
            return self.tab(!shift, now);
        }
        if key == "key.keyboard.enter" || key == "key.keyboard.kp.enter" {
            if let Some(index) = self.active_focus() {
                if matches!(self.widgets.get(index), Some(Widget::TextField(_))) {
                    return Outcome::Submitted(WidgetId(index));
                }
            }
            return Outcome::Ignored;
        }
        let Some(index) = self.active_focus() else {
            return Outcome::Ignored;
        };
        let id = WidgetId(index);
        let Some(Widget::TextField(field)) = self.widgets.get_mut(index) else {
            return Outcome::Ignored;
        };
        let before = field.edit.value.clone();
        let edit_key = match key {
            "key.keyboard.backspace" => Some(EditKey::Backspace),
            "key.keyboard.delete" => Some(EditKey::Delete),
            "key.keyboard.left" => Some(EditKey::Left),
            "key.keyboard.right" => Some(EditKey::Right),
            "key.keyboard.home" => Some(EditKey::Home),
            "key.keyboard.end" => Some(EditKey::End),
            _ => None,
        };
        let handled = match edit_key {
            Some(k) => field.edit.key(k, ctrl, shift),
            None if ctrl => match key {
                "key.keyboard.a" => {
                    field.edit.set_cursor(0, false);
                    field
                        .edit
                        .set_cursor(char_len(&field.edit.value), true);
                    true
                }
                "key.keyboard.c" => {
                    let _ = host.set_clipboard_text(&field.edit.selected_text());
                    true
                }
                "key.keyboard.x" => {
                    let _ = host.set_clipboard_text(&field.edit.selected_text());
                    field.edit.insert_text("");
                    true
                }
                "key.keyboard.v" => {
                    if let Some(text) = host.clipboard_text() {
                        field.edit.insert_text(&text);
                    }
                    true
                }
                _ => false,
            },
            None => false,
        };
        if !handled {
            return Outcome::Ignored;
        }
        field.moved(now);
        if field.edit.value != before {
            Outcome::Changed(id)
        } else {
            Outcome::Handled
        }
    }

    pub fn char_typed(&mut self, host: &dyn ClientHost, c: char) -> Outcome {
        let Some(index) = self.active_focus() else {
            return Outcome::Ignored;
        };
        let Some(Widget::TextField(field)) = self.widgets.get_mut(index) else {
            return Outcome::Ignored;
        };
        if !is_allowed_chat_character(c) {
            return Outcome::Ignored;
        }
        let before = field.edit.value.clone();
        let mut buf = [0; 4];
        field.edit.insert_text(c.encode_utf8(&mut buf));
        field.moved(host.millis());
        if field.edit.value != before {
            Outcome::Changed(WidgetId(index))
        } else {
            Outcome::Handled
        }
    }

    fn tab(&mut self, forward: bool, now: u64) -> Outcome {
        let count = self.widgets.len();
        if count == 0 {
            return Outcome::Ignored;
        }
        let start = match (self.focus, forward) {
            (Some(i), true) => i + 1,
            (Some(i), false) => i + count - 1,
            (None, true) => 0,
            (None, false) => count - 1,
        };
        let next = (0..count)
            .map(|step| {
                if forward {
                    (start + step) % count
                } else {
                    (start + count - step) % count
                }
            })
            .find(|&i| self.widgets[i].active());
        match next {
            Some(i) => {
                self.focus_index(Some(i), now);
                Outcome::Handled
            }
            None => Outcome::Ignored,
        }
    }
}

fn draw_button(draw: &mut dyn DrawContext, button: &Button, hover: bool) {
    let [x, y, w, h] = button.rect;
    draw.fill([x, y, w, h], BUTTON_BORDER);
    draw.fill(
        [x + 1.0, y + 1.0, (w - 2.0).max(0.0), (h - 2.0).max(0.0)],
        if hover { BUTTON_HOVER } else { BUTTON_BG },
    );
    let label = &button.label;
    let tw = draw.text_width(label);
    let tx = x + ((w - tw) / 2.0).max(0.0);
    let ty = y + ((h - 9.0) / 2.0).max(0.0);
    draw.text(label, [tx.round(), ty.round()], TEXT_COLOR, false);
}

fn draw_field(draw: &mut dyn DrawContext, field: &mut TextField, focused: bool, now: u64) {
    let [x, y, w, h] = field.rect;
    let border = if focused {
        FIELD_BORDER_FOCUS
    } else {
        FIELD_BORDER
    };
    draw.fill([x, y, w, h], border);
    draw.fill(
        [x + 1.0, y + 1.0, (w - 2.0).max(0.0), (h - 2.0).max(0.0)],
        FIELD_BG,
    );
    let pad = 4.0;
    let text_x = x + pad;
    let text_y = y + ((h - 9.0) / 2.0).max(1.0);
    let inner_w = (w - pad * 2.0).max(0.0);
    let chars: Vec<char> = field.edit.value.chars().collect();
    let n = chars.len();
    // Keep the cursor in view.
    if field.edit.cursor < field.scroll {
        field.scroll = field.edit.cursor;
    }
    loop {
        let shown: String = chars.iter().skip(field.scroll).collect();
        if field.scroll >= field.edit.cursor || draw.text_width(&shown) <= inner_w {
            break;
        }
        field.scroll += 1;
        if field.scroll >= n {
            break;
        }
    }
    let shown: String = chars.iter().skip(field.scroll).collect();
    // Clip by measuring prefix that fits.
    let mut fit = String::new();
    for c in shown.chars() {
        let mut trial = fit.clone();
        trial.push(c);
        if draw.text_width(&trial) > inner_w {
            break;
        }
        fit = trial;
    }
    // Edges for click-to-cursor.
    field.edges.clear();
    field.edges.push(text_x);
    let mut acc = String::new();
    for c in chars.iter().skip(field.scroll) {
        acc.push(*c);
        if acc.chars().count() > fit.chars().count() {
            break;
        }
        field.edges.push(text_x + draw.text_width(&acc));
    }
    // Selection.
    let (a, b) = field.edit.range();
    if focused && a != b {
        let sx = char_x(&chars, field.scroll, a, text_x, draw);
        let ex = char_x(&chars, field.scroll, b, text_x, draw);
        let x0 = sx.max(text_x);
        let x1 = ex.min(text_x + inner_w);
        if x1 > x0 {
            draw.fill([x0, y + 2.0, x1 - x0, (h - 4.0).max(1.0)], HIGHLIGHT);
        }
    }
    let color = if field.active { TEXT_COLOR } else { TEXT_INACTIVE };
    draw.text(&fit, [text_x, text_y.round()], color, false);
    let caret_shown =
        focused && (now.saturating_sub(field.moved_at) / BLINK_MS).is_multiple_of(2);
    if caret_shown {
        let cx = char_x(&chars, field.scroll, field.edit.cursor, text_x, draw)
            .clamp(text_x, text_x + inner_w);
        draw.fill([cx, y + 3.0, 1.0, (h - 6.0).max(1.0)], TEXT_COLOR);
    }
}

fn char_x(chars: &[char], scroll: usize, index: usize, origin: f32, draw: &dyn DrawContext) -> f32 {
    if index <= scroll {
        return origin;
    }
    let prefix: String = chars.iter().skip(scroll).take(index - scroll).collect();
    origin + draw.text_width(&prefix)
}

fn over(rect: [f32; 4], at: [f32; 2]) -> bool {
    let [x, y, w, h] = rect;
    at[0] >= x && at[0] < x + w && at[1] >= y && at[1] < y + h
}

fn char_len(s: &str) -> usize {
    s.chars().count()
}

fn slice_chars(s: &str, start: usize, end: usize) -> String {
    s.chars().skip(start).take(end.saturating_sub(start)).collect()
}

fn truncate_chars(s: &str, max: usize) -> String {
    s.chars().take(max).collect()
}

fn is_allowed_chat_character(c: char) -> bool {
    c != '§' && c != '\u{7f}' && !c.is_control()
}
