use super::ansi::{Color, ColorScheme, EscapeSequence, Sgr};
use crate::graphics::{FontStyle, FrameBuffer, MonospaceFont, MonospaceTextBuffer};

const FONT_SIZE: u32 = 14;
static FONT_NORMAL: &[u8] = include_bytes!("Tamzen7x14r.ttf");
static FONT_BOLD: &[u8] = include_bytes!("Tamzen7x14b.ttf");

pub struct Screen<'a, T, S> {
    buf: MonospaceTextBuffer<'a, T>,
    theme: S,
    fg: Color,
    bg: Color,
    font_style: FontStyle,
}

impl<'a, T: FrameBuffer, S: ColorScheme> Screen<'a, T, S> {
    pub fn new(buf: T, theme: S) -> Self {
        let format = buf.format();
        Self {
            buf: MonospaceTextBuffer::new(
                buf,
                MonospaceFont::new(FONT_SIZE, FONT_NORMAL, FONT_BOLD, format),
            ),
            theme,
            fg: Color::Default,
            bg: Color::Default,
            font_style: FontStyle::Normal,
        }
    }

    pub fn render(&mut self) {
        self.buf.render();
    }

    pub fn put_char(&mut self, ch: char) {
        self.buf.put(
            ch,
            self.theme.get_fg(self.fg).into(),
            self.theme.get_bg(self.bg).into(),
            self.font_style,
        );
    }

    pub fn erase(
        &mut self,
        before_cursor_lines: bool,
        before_cursor_chars: bool,
        after_cursor_chars: bool,
        after_cursor_lines: bool,
    ) {
        self.buf.erase(
            self.theme.get_bg(self.bg).into(),
            before_cursor_lines,
            before_cursor_chars,
            after_cursor_chars,
            after_cursor_lines,
        );
    }

    pub fn handle_escape_sequence(&mut self, es: EscapeSequence) {
        use EscapeSequence::*;

        match es {
            CursorUp(n) => self.buf.move_cursor(0, -(n as i32)),
            CursorDown(n) => self.buf.move_cursor(0, n as i32),
            CursorForward(n) => self.buf.move_cursor(n as i32, 0),
            CursorBack(n) => self.buf.move_cursor(-(n as i32), 0),
            CursorNextLine(n) => self.buf.move_cursor(i32::MIN, n as i32),
            CursorPreviousLine(n) => self.buf.move_cursor(i32::MIN, -(n as i32)),
            CursorHorizontalAbsolute(n) => self.buf.set_cursor(Some(n - 1), None),
            CursorPosition(n, m) => self.buf.set_cursor(Some(m - 1), Some(n - 1)),
            EraseInDisplay(0) => self.erase(false, false, true, true),
            EraseInDisplay(1) => self.erase(true, true, false, false),
            EraseInDisplay(2) => self.erase(true, true, true, true),
            EraseInLine(0) => self.erase(false, false, true, false),
            EraseInLine(1) => self.erase(false, true, false, false),
            EraseInLine(2) => self.erase(false, true, true, false),
            HorizontalVerticalPosition(n, m) => self.buf.set_cursor(Some(m - 1), Some(n - 1)),
            Sgr(a) => self.handle_sgr(a),
            Sgr2(a, b) => {
                self.handle_sgr(a);
                self.handle_sgr(b);
            }
            Sgr3(a, b, c) => {
                self.handle_sgr(a);
                self.handle_sgr(b);
                self.handle_sgr(c);
            }
            _ => {}
        }
    }

    pub fn handle_sgr(&mut self, sgr: Sgr) {
        use Sgr::*;

        match sgr {
            Reset => {
                self.fg = Color::Default;
                self.bg = Color::Default;
                self.font_style = FontStyle::Normal;
            }
            Bold => {
                self.font_style = FontStyle::Bold;
                self.fg = self.fg.brighter();
            }
            Faint | ResetBoldFaint => {
                self.font_style = FontStyle::Normal;
                self.fg = self.fg.dimmer();
            }
            Italic(_) => {}        // Unsupported
            Underline(_) => {}     // Unsupported
            Blinking(_) => {}      // Unsupported
            Inverse(_) => {}       // Unsupported
            Hidden(_) => {}        // Unsupported
            Strikethrough(_) => {} // Unsupported
            Fg(color) => {
                self.fg = if self.font_style.is_bold() {
                    color.brighter()
                } else {
                    color.dimmer()
                }
            }
            Bg(color) => self.bg = color,
        }
    }
}
