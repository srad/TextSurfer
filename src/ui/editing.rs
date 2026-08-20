use unicode_segmentation::UnicodeSegmentation;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct EditBuffer {
    text: String,
    cursor: usize,
}

impl EditBuffer {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn set_text(&mut self, text: &str) {
        self.text.clear();
        self.text.push_str(text);
        self.cursor = self.len();
    }

    pub fn insert(&mut self, ch: char) {
        let byte = self.byte_at(self.cursor);
        self.text.insert(byte, ch);
        let inserted_end = byte + ch.len_utf8();
        self.cursor = self
            .text
            .grapheme_indices(true)
            .position(|(start, grapheme)| start + grapheme.len() >= inserted_end)
            .map_or_else(|| self.len(), |index| index + 1);
    }

    pub fn backspace(&mut self) {
        if self.cursor > 0 {
            let start = self.byte_at(self.cursor - 1);
            let end = self.byte_at(self.cursor);
            self.text.replace_range(start..end, "");
            self.cursor -= 1;
        }
    }

    pub fn delete(&mut self) {
        if self.cursor < self.len() {
            let start = self.byte_at(self.cursor);
            let end = self.byte_at(self.cursor + 1);
            self.text.replace_range(start..end, "");
        }
    }

    pub fn left(&mut self) {
        self.cursor = self.cursor.saturating_sub(1);
    }

    pub fn right(&mut self) {
        self.cursor = (self.cursor + 1).min(self.len());
    }

    pub fn home(&mut self) {
        self.cursor = 0;
    }

    pub fn end(&mut self) {
        self.cursor = self.len();
    }

    pub fn text(&self) -> &str {
        &self.text
    }

    pub fn cursor(&self) -> usize {
        self.cursor
    }

    pub fn len(&self) -> usize {
        self.text.graphemes(true).count()
    }

    pub fn is_empty(&self) -> bool {
        self.text.is_empty()
    }

    fn byte_at(&self, grapheme: usize) -> usize {
        self.text
            .grapheme_indices(true)
            .nth(grapheme)
            .map_or(self.text.len(), |(byte, _)| byte)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    #[derive(Clone, Copy, Debug)]
    enum Op {
        Insert(char),
        Backspace,
        Delete,
        Left,
        Right,
        Home,
        End,
    }

    fn any_op() -> impl Strategy<Value = Op> {
        prop_oneof![
            any::<char>().prop_map(Op::Insert),
            Just(Op::Backspace),
            Just(Op::Delete),
            Just(Op::Left),
            Just(Op::Right),
            Just(Op::Home),
            Just(Op::End),
        ]
    }

    proptest! {
        #[test]
        fn cursor_never_leaves_the_buffer(ops in prop::collection::vec(any_op(), 0..128)) {
            let mut buffer = EditBuffer::new();
            for op in ops {
                match op {
                    Op::Insert(ch) => buffer.insert(ch),
                    Op::Backspace => buffer.backspace(),
                    Op::Delete => buffer.delete(),
                    Op::Left => buffer.left(),
                    Op::Right => buffer.right(),
                    Op::Home => buffer.home(),
                    Op::End => buffer.end(),
                }
                prop_assert!(buffer.cursor() <= buffer.len());
            }
        }
    }

    #[test]
    fn insert_places_the_char_at_the_cursor() {
        let mut buffer = EditBuffer::new();
        buffer.set_text("abc");
        buffer.home();
        buffer.insert('X');
        assert_eq!(buffer.text(), "Xabc");
        assert_eq!(buffer.cursor(), 1);
    }

    #[test]
    fn backspace_removes_before_the_cursor() {
        let mut buffer = EditBuffer::new();
        buffer.set_text("abc");
        buffer.left();
        buffer.backspace();
        assert_eq!(buffer.text(), "ac");
        assert_eq!(buffer.cursor(), 1);
    }

    #[test]
    fn delete_removes_at_the_cursor() {
        let mut buffer = EditBuffer::new();
        buffer.set_text("abc");
        buffer.home();
        buffer.delete();
        assert_eq!(buffer.text(), "bc");
    }

    #[test]
    fn movement_clamps_at_the_edges() {
        let mut buffer = EditBuffer::new();
        buffer.set_text("ab");
        buffer.home();
        buffer.left();
        assert_eq!(buffer.cursor(), 0);
        buffer.end();
        buffer.right();
        assert_eq!(buffer.cursor(), 2);
        buffer.backspace();
        buffer.backspace();
        buffer.backspace();
        assert!(buffer.is_empty());
        assert_eq!(buffer.cursor(), 0);
    }

    #[test]
    fn set_text_resets_the_cursor_to_the_end() {
        let mut buffer = EditBuffer::new();
        buffer.insert('x');
        buffer.home();
        buffer.set_text("hello");
        assert_eq!(buffer.cursor(), 5);
        assert_eq!(buffer.text(), "hello");
    }

    #[test]
    fn movement_and_deletion_operate_on_grapheme_clusters() {
        let mut buffer = EditBuffer::new();
        buffer.set_text("a\u{301}👩‍💻z");
        assert_eq!(buffer.len(), 3);
        buffer.left();
        buffer.backspace();
        assert_eq!(buffer.text(), "a\u{301}z");
        buffer.home();
        buffer.delete();
        assert_eq!(buffer.text(), "z");
    }

    #[test]
    fn typed_combining_sequences_remain_one_cursor_step() {
        let mut buffer = EditBuffer::new();
        buffer.insert('a');
        buffer.insert('\u{301}');
        assert_eq!(buffer.len(), 1);
        assert_eq!(buffer.cursor(), 1);
    }
}
