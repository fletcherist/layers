use std::borrow::Cow;

/// Reusable click-to-edit text field. `input = None` means display mode; `Some` means editing.
pub struct ValueEntry {
    pub input: Option<String>,
}

impl ValueEntry {
    pub fn new() -> Self {
        Self { input: None }
    }

    pub fn is_editing(&self) -> bool {
        self.input.is_some()
    }

    pub fn enter(&mut self) {
        self.input = Some(String::new());
    }

    pub fn cancel(&mut self) {
        self.input = None;
    }

    pub fn push_char(&mut self, s: &str) {
        if let Some(ref mut text) = self.input {
            text.push_str(s);
        }
    }

    pub fn pop_char(&mut self) {
        if let Some(ref mut text) = self.input {
            text.pop();
        }
    }

    /// Takes and clears the input, returning it.
    pub fn commit(&mut self) -> Option<String> {
        self.input.take()
    }

    /// Returns "42.5|" when editing, or `idle` otherwise.
    pub fn display<'a>(&'a self, idle: &'a str) -> Cow<'a, str> {
        match &self.input {
            Some(text) => Cow::Owned(format!("{}|", text)),
            None => Cow::Borrowed(idle),
        }
    }
}
