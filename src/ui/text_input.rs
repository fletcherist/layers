use winit::keyboard::{Key, NamedKey};

#[cfg(target_arch = "wasm32")]
use web_time::Instant as TimeInstant;
#[cfg(not(target_arch = "wasm32"))]
use std::time::Instant as TimeInstant;

#[derive(Clone, Debug)]
pub struct TextInputConfig {
    pub multiline: bool,
    pub allow_spaces: bool,
    pub placeholder: String,
}

impl Default for TextInputConfig {
    fn default() -> Self {
        Self {
            multiline: false,
            allow_spaces: true,
            placeholder: String::new(),
        }
    }
}

#[derive(Debug, PartialEq)]
pub enum TextInputAction {
    Changed,
    CursorMoved,
    Submit,
    Cancel,
    Ignored,
}

pub struct TextInput {
    pub text: String,
    pub cursor: usize,
    pub config: TextInputConfig,
    blink_start: TimeInstant,
    blink_visible: bool,
}

impl TextInput {
    pub fn new(config: TextInputConfig) -> Self {
        Self {
            text: String::new(),
            cursor: 0,
            config,
            blink_start: TimeInstant::now(),
            blink_visible: true,
        }
    }

    pub fn with_text(text: String, config: TextInputConfig) -> Self {
        let cursor = text.len();
        Self {
            text,
            cursor,
            config,
            blink_start: TimeInstant::now(),
            blink_visible: true,
        }
    }

    pub fn text(&self) -> &str {
        &self.text
    }

    pub fn into_text(self) -> String {
        self.text
    }

    pub fn is_empty(&self) -> bool {
        self.text.is_empty()
    }

    pub fn clear(&mut self) {
        self.text.clear();
        self.cursor = 0;
        self.reset_cursor_blink();
    }

    pub fn handle_key(&mut self, key: &Key, cmd_held: bool) -> TextInputAction {
        match key {
            Key::Named(NamedKey::Escape) => TextInputAction::Cancel,
            Key::Named(NamedKey::Enter) => {
                if self.config.multiline && !cmd_held {
                    self.text.insert(self.cursor, '\n');
                    self.cursor += 1;
                    self.reset_cursor_blink();
                    TextInputAction::Changed
                } else {
                    TextInputAction::Submit
                }
            }
            Key::Named(NamedKey::Backspace) => {
                if cmd_held {
                    if !self.text.is_empty() {
                        self.text.clear();
                        self.cursor = 0;
                        self.reset_cursor_blink();
                        TextInputAction::Changed
                    } else {
                        TextInputAction::Ignored
                    }
                } else if self.cursor > 0 {
                    let prev = self.text[..self.cursor]
                        .char_indices()
                        .next_back()
                        .map(|(i, _)| i)
                        .unwrap_or(0);
                    self.cursor = prev;
                    self.text.remove(self.cursor);
                    self.reset_cursor_blink();
                    TextInputAction::Changed
                } else {
                    TextInputAction::Ignored
                }
            }
            Key::Named(NamedKey::Delete) => {
                if self.cursor < self.text.len() {
                    self.text.remove(self.cursor);
                    self.reset_cursor_blink();
                    TextInputAction::Changed
                } else {
                    TextInputAction::Ignored
                }
            }
            Key::Named(NamedKey::ArrowLeft) => {
                if self.cursor > 0 {
                    self.cursor = self.text[..self.cursor]
                        .char_indices()
                        .next_back()
                        .map(|(i, _)| i)
                        .unwrap_or(0);
                    self.reset_cursor_blink();
                    TextInputAction::CursorMoved
                } else {
                    TextInputAction::Ignored
                }
            }
            Key::Named(NamedKey::ArrowRight) => {
                if self.cursor < self.text.len() {
                    self.cursor = self.text[self.cursor..]
                        .char_indices()
                        .nth(1)
                        .map(|(i, _)| self.cursor + i)
                        .unwrap_or(self.text.len());
                    self.reset_cursor_blink();
                    TextInputAction::CursorMoved
                } else {
                    TextInputAction::Ignored
                }
            }
            Key::Named(NamedKey::ArrowUp) if self.config.multiline => {
                let before = &self.text[..self.cursor];
                if let Some(cur_line_start) = before.rfind('\n') {
                    let col = self.cursor - cur_line_start - 1;
                    let prev_line_start = before[..cur_line_start]
                        .rfind('\n')
                        .map(|p| p + 1)
                        .unwrap_or(0);
                    let prev_line_len = cur_line_start - prev_line_start;
                    self.cursor = prev_line_start + col.min(prev_line_len);
                    self.reset_cursor_blink();
                    TextInputAction::CursorMoved
                } else {
                    TextInputAction::Ignored
                }
            }
            Key::Named(NamedKey::ArrowDown) if self.config.multiline => {
                let before = &self.text[..self.cursor];
                let cur_line_start = before.rfind('\n').map(|p| p + 1).unwrap_or(0);
                let col = self.cursor - cur_line_start;
                if let Some(next_nl) = self.text[self.cursor..].find('\n') {
                    let next_line_start = self.cursor + next_nl + 1;
                    let next_line_end = self.text[next_line_start..]
                        .find('\n')
                        .map(|p| next_line_start + p)
                        .unwrap_or(self.text.len());
                    let next_line_len = next_line_end - next_line_start;
                    self.cursor = next_line_start + col.min(next_line_len);
                    self.reset_cursor_blink();
                    TextInputAction::CursorMoved
                } else {
                    TextInputAction::Ignored
                }
            }
            Key::Named(NamedKey::Home) => {
                if self.cursor > 0 {
                    self.cursor = 0;
                    self.reset_cursor_blink();
                    TextInputAction::CursorMoved
                } else {
                    TextInputAction::Ignored
                }
            }
            Key::Named(NamedKey::End) => {
                if self.cursor < self.text.len() {
                    self.cursor = self.text.len();
                    self.reset_cursor_blink();
                    TextInputAction::CursorMoved
                } else {
                    TextInputAction::Ignored
                }
            }
            Key::Named(NamedKey::Space) => {
                if self.config.allow_spaces {
                    self.text.insert(self.cursor, ' ');
                    self.cursor += 1;
                    self.reset_cursor_blink();
                    TextInputAction::Changed
                } else {
                    TextInputAction::Ignored
                }
            }
            Key::Character(ch) if cmd_held => {
                match ch.as_ref() {
                    "a" => {
                        self.cursor = self.text.len();
                        self.reset_cursor_blink();
                        TextInputAction::CursorMoved
                    }
                    _ => TextInputAction::Ignored,
                }
            }
            Key::Character(ch) if !cmd_held => {
                for c in ch.chars() {
                    self.text.insert(self.cursor, c);
                    self.cursor += c.len_utf8();
                }
                self.reset_cursor_blink();
                TextInputAction::Changed
            }
            _ => TextInputAction::Ignored,
        }
    }

    pub fn paste(&mut self, clipboard_text: &str) {
        let filtered: String = if self.config.allow_spaces {
            if self.config.multiline {
                clipboard_text.to_string()
            } else {
                clipboard_text.chars().filter(|c| *c != '\n' && *c != '\r').collect()
            }
        } else {
            clipboard_text.chars().filter(|c| !c.is_whitespace()).collect()
        };
        if !filtered.is_empty() {
            self.text.insert_str(self.cursor, &filtered);
            self.cursor += filtered.len();
            self.reset_cursor_blink();
        }
    }

    pub fn display_text(&self) -> String {
        if self.blink_visible {
            let mut result = String::with_capacity(self.text.len() + 1);
            result.push_str(&self.text[..self.cursor]);
            result.push('|');
            result.push_str(&self.text[self.cursor..]);
            result
        } else {
            self.text.clone()
        }
    }

    pub fn reset_cursor_blink(&mut self) {
        self.blink_start = TimeInstant::now();
        self.blink_visible = true;
    }

    pub fn tick_cursor_blink(&mut self) -> bool {
        let visible = self.blink_start.elapsed().as_millis() % 1000 < 500;
        if visible != self.blink_visible {
            self.blink_visible = visible;
            true
        } else {
            false
        }
    }

    pub fn next_cursor_blink_toggle(&self) -> TimeInstant {
        let elapsed_ms = self.blink_start.elapsed().as_millis();
        let phase = elapsed_ms % 1000;
        let remaining = if phase < 500 { 500 - phase } else { 1000 - phase };
        TimeInstant::now() + std::time::Duration::from_millis(remaining as u64)
    }

    pub fn cursor_blink_visible(&self) -> bool {
        self.blink_visible
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn single_line() -> TextInput {
        TextInput::new(TextInputConfig::default())
    }

    fn single_line_no_spaces() -> TextInput {
        TextInput::new(TextInputConfig {
            allow_spaces: false,
            ..Default::default()
        })
    }

    fn multiline() -> TextInput {
        TextInput::new(TextInputConfig {
            multiline: true,
            ..Default::default()
        })
    }

    fn key_char(ch: &str) -> Key {
        Key::Character(ch.into())
    }

    #[test]
    fn test_character_input() {
        let mut input = single_line();
        assert_eq!(input.handle_key(&key_char("h"), false), TextInputAction::Changed);
        assert_eq!(input.handle_key(&key_char("i"), false), TextInputAction::Changed);
        assert_eq!(input.text(), "hi");
        assert_eq!(input.cursor, 2);
    }

    #[test]
    fn test_backspace() {
        let mut input = TextInput::with_text("abc".into(), TextInputConfig::default());
        assert_eq!(input.handle_key(&Key::Named(NamedKey::Backspace), false), TextInputAction::Changed);
        assert_eq!(input.text(), "ab");
        assert_eq!(input.cursor, 2);
    }

    #[test]
    fn test_backspace_at_start() {
        let mut input = single_line();
        assert_eq!(input.handle_key(&Key::Named(NamedKey::Backspace), false), TextInputAction::Ignored);
    }

    #[test]
    fn test_cmd_backspace_clears() {
        let mut input = TextInput::with_text("hello".into(), TextInputConfig::default());
        assert_eq!(input.handle_key(&Key::Named(NamedKey::Backspace), true), TextInputAction::Changed);
        assert_eq!(input.text(), "");
        assert_eq!(input.cursor, 0);
    }

    #[test]
    fn test_arrow_left_right() {
        let mut input = TextInput::with_text("abc".into(), TextInputConfig::default());
        assert_eq!(input.cursor, 3);
        assert_eq!(input.handle_key(&Key::Named(NamedKey::ArrowLeft), false), TextInputAction::CursorMoved);
        assert_eq!(input.cursor, 2);
        assert_eq!(input.handle_key(&Key::Named(NamedKey::ArrowLeft), false), TextInputAction::CursorMoved);
        assert_eq!(input.cursor, 1);
        assert_eq!(input.handle_key(&Key::Named(NamedKey::ArrowRight), false), TextInputAction::CursorMoved);
        assert_eq!(input.cursor, 2);
    }

    #[test]
    fn test_insert_at_cursor() {
        let mut input = TextInput::with_text("ac".into(), TextInputConfig::default());
        input.handle_key(&Key::Named(NamedKey::ArrowLeft), false);
        assert_eq!(input.cursor, 1);
        input.handle_key(&key_char("b"), false);
        assert_eq!(input.text(), "abc");
        assert_eq!(input.cursor, 2);
    }

    #[test]
    fn test_backspace_at_cursor() {
        let mut input = TextInput::with_text("abc".into(), TextInputConfig::default());
        input.handle_key(&Key::Named(NamedKey::ArrowLeft), false);
        assert_eq!(input.cursor, 2);
        input.handle_key(&Key::Named(NamedKey::Backspace), false);
        assert_eq!(input.text(), "ac");
        assert_eq!(input.cursor, 1);
    }

    #[test]
    fn test_delete_at_cursor() {
        let mut input = TextInput::with_text("abc".into(), TextInputConfig::default());
        input.cursor = 1;
        assert_eq!(input.handle_key(&Key::Named(NamedKey::Delete), false), TextInputAction::Changed);
        assert_eq!(input.text(), "ac");
        assert_eq!(input.cursor, 1);
    }

    #[test]
    fn test_space_allowed() {
        let mut input = single_line();
        assert_eq!(input.handle_key(&Key::Named(NamedKey::Space), false), TextInputAction::Changed);
        assert_eq!(input.text(), " ");
    }

    #[test]
    fn test_space_rejected() {
        let mut input = single_line_no_spaces();
        assert_eq!(input.handle_key(&Key::Named(NamedKey::Space), false), TextInputAction::Ignored);
        assert_eq!(input.text(), "");
    }

    #[test]
    fn test_enter_submits_single_line() {
        let mut input = single_line();
        assert_eq!(input.handle_key(&Key::Named(NamedKey::Enter), false), TextInputAction::Submit);
    }

    #[test]
    fn test_enter_newline_multiline() {
        let mut input = multiline();
        assert_eq!(input.handle_key(&Key::Named(NamedKey::Enter), false), TextInputAction::Changed);
        assert_eq!(input.text(), "\n");
    }

    #[test]
    fn test_cmd_enter_submits_multiline() {
        let mut input = multiline();
        assert_eq!(input.handle_key(&Key::Named(NamedKey::Enter), true), TextInputAction::Submit);
    }

    #[test]
    fn test_escape_cancels() {
        let mut input = single_line();
        assert_eq!(input.handle_key(&Key::Named(NamedKey::Escape), false), TextInputAction::Cancel);
    }

    #[test]
    fn test_home_end() {
        let mut input = TextInput::with_text("abc".into(), TextInputConfig::default());
        assert_eq!(input.handle_key(&Key::Named(NamedKey::Home), false), TextInputAction::CursorMoved);
        assert_eq!(input.cursor, 0);
        assert_eq!(input.handle_key(&Key::Named(NamedKey::End), false), TextInputAction::CursorMoved);
        assert_eq!(input.cursor, 3);
    }

    #[test]
    fn test_cmd_a_moves_to_end() {
        let mut input = TextInput::with_text("hello".into(), TextInputConfig::default());
        input.cursor = 0;
        assert_eq!(input.handle_key(&key_char("a"), true), TextInputAction::CursorMoved);
        assert_eq!(input.cursor, 5);
    }

    #[test]
    fn test_multiline_arrow_up_down() {
        let mut input = TextInput::with_text("abc\ndef\nghi".into(), TextInputConfig {
            multiline: true,
            ..Default::default()
        });
        // cursor at end: position 11 (after 'i')
        // Move to middle of last line
        input.cursor = 9; // 'h' in "ghi"
        // ArrowUp: should go to 'd' in "def"
        input.handle_key(&Key::Named(NamedKey::ArrowUp), false);
        assert_eq!(input.cursor, 5); // 'd' in "def" (line start=4, col=1 -> 4+1=5)

        // ArrowUp again: should go to 'b' in "abc"
        input.handle_key(&Key::Named(NamedKey::ArrowUp), false);
        assert_eq!(input.cursor, 1); // 'b' in "abc"

        // ArrowDown: should go back to 'e' in "def"
        input.handle_key(&Key::Named(NamedKey::ArrowDown), false);
        assert_eq!(input.cursor, 5); // 'e' in "def"
    }

    #[test]
    fn test_paste_single_line() {
        let mut input = single_line();
        input.paste("hello world");
        assert_eq!(input.text(), "hello world");
        assert_eq!(input.cursor, 11);
    }

    #[test]
    fn test_paste_no_spaces() {
        let mut input = single_line_no_spaces();
        input.paste("hello world");
        assert_eq!(input.text(), "helloworld");
    }

    #[test]
    fn test_paste_strips_newlines_single_line() {
        let mut input = single_line();
        input.paste("line1\nline2");
        assert_eq!(input.text(), "line1line2");
    }

    #[test]
    fn test_paste_at_cursor() {
        let mut input = TextInput::with_text("ac".into(), TextInputConfig::default());
        input.cursor = 1;
        input.paste("b");
        assert_eq!(input.text(), "abc");
        assert_eq!(input.cursor, 2);
    }

    #[test]
    fn test_display_text_with_cursor() {
        let mut input = TextInput::with_text("abc".into(), TextInputConfig::default());
        // blink_visible is true by default
        assert_eq!(input.display_text(), "abc|");
        input.cursor = 1;
        assert_eq!(input.display_text(), "a|bc");
        input.cursor = 0;
        assert_eq!(input.display_text(), "|abc");
    }

    #[test]
    fn test_utf8_handling() {
        let mut input = single_line();
        input.handle_key(&key_char("é"), false);
        input.handle_key(&key_char("a"), false);
        assert_eq!(input.text(), "éa");
        // Backspace should delete 'a', not half of 'é'
        input.handle_key(&Key::Named(NamedKey::Backspace), false);
        assert_eq!(input.text(), "é");
        // Arrow left then right
        input.handle_key(&Key::Named(NamedKey::ArrowLeft), false);
        assert_eq!(input.cursor, 0);
        input.handle_key(&Key::Named(NamedKey::ArrowRight), false);
        assert_eq!(input.cursor, "é".len());
    }

    #[test]
    fn test_clear() {
        let mut input = TextInput::with_text("hello".into(), TextInputConfig::default());
        input.clear();
        assert_eq!(input.text(), "");
        assert_eq!(input.cursor, 0);
    }

    #[test]
    fn test_arrow_up_on_first_line_ignored() {
        let mut input = TextInput::with_text("abc".into(), TextInputConfig {
            multiline: true,
            ..Default::default()
        });
        assert_eq!(input.handle_key(&Key::Named(NamedKey::ArrowUp), false), TextInputAction::Ignored);
    }

    #[test]
    fn test_arrow_down_on_last_line_ignored() {
        let mut input = TextInput::with_text("abc".into(), TextInputConfig {
            multiline: true,
            ..Default::default()
        });
        assert_eq!(input.handle_key(&Key::Named(NamedKey::ArrowDown), false), TextInputAction::Ignored);
    }
}
