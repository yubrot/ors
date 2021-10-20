use super::{Color, FontStyle, FrameBuffer, FrameBufferExt, MonospaceFont, VecBuffer};
use alloc::collections::VecDeque;
use alloc::vec;
use alloc::vec::Vec;

#[derive(Debug)]
pub struct MonospaceTextBuffer<'a, T> {
    lines: VecDeque<Line>,
    buf: T,
    render_diff: RenderDiff,
    font: MonospaceFont<'a>,
    state: State,
    initial_state: State,
}

impl<'a, T: FrameBuffer> MonospaceTextBuffer<'a, T> {
    pub fn new(buf: T, font: MonospaceFont<'a>) -> Self {
        assert_eq!(buf.format(), font.format());
        let height = buf.height() / font.unit_height() as usize;
        let initial_state = State::default(); // NOTE: argument?
        let lines = vec![Line::new(&buf, &initial_state, &font); height].into();
        Self {
            lines,
            buf,
            render_diff: Some((0, height)),
            font,
            state: initial_state,
            initial_state,
        }
    }

    pub fn font_style(&mut self) -> FontStyle {
        self.state.font_style
    }

    pub fn set_font_style(&mut self, font_style: FontStyle) {
        self.state.font_style = font_style;
    }

    pub fn reset_font_style(&mut self) {
        self.state.font_style = self.initial_state.font_style;
    }

    pub fn fg(&mut self) -> Color {
        self.state.fg
    }

    pub fn set_fg(&mut self, fg: Color) {
        self.state.fg = fg;
    }

    pub fn reset_fg(&mut self) {
        self.state.fg = self.initial_state.fg;
    }

    pub fn bg(&mut self) -> Color {
        self.state.bg
    }

    pub fn set_bg(&mut self, bg: Color) {
        self.state.bg = bg;
    }

    pub fn reset_bg(&mut self) {
        self.state.bg = self.initial_state.bg;
    }

    pub fn clear(&mut self) {
        self.state = self.initial_state;
        let mut start = usize::MAX;
        let mut end = 0;
        for (i, l) in self.lines.iter_mut().enumerate() {
            if l.clear(&self.state) {
                start = start.min(i);
                end = end.max(i + 1);
            }
        }
        if start < end {
            extend_render_diff(&mut self.render_diff, start, end);
        }
    }

    pub fn next_line(&mut self) {
        let (_, y) = self.state.cursor;
        if y + 1 >= self.lines.len() {
            let mut first_line = self.lines.pop_front().unwrap(); // remove the first line
            first_line.clear(&self.initial_state);
            self.lines.push_back(first_line);
            self.render_diff = Some((0, self.lines.len())); // all lines
            self.state.cursor = (0, self.lines.len() - 1);
        } else {
            self.state.cursor = (0, y + 1);
        }
    }

    pub fn put(&mut self, c: char) {
        let (x, y) = self.state.cursor;
        match self.lines[y].put(c, x, &self.state) {
            LinePutResult::LineFeed => self.next_line(),
            LinePutResult::Wrapping(c) => {
                self.next_line();
                self.put(c);
            }
            LinePutResult::Next(changed, x) => {
                self.state.cursor = (x, y);
                if changed {
                    extend_render_diff(&mut self.render_diff, y, y + 1);
                }
            }
        }
    }

    pub fn render(&mut self) {
        if let Some((a, b)) = self.render_diff {
            let pad_y =
                (self.buf.height() - self.lines.len() * self.font.unit_height() as usize) as i32;
            for (i, line) in self.lines.iter_mut().enumerate().skip(a).take(b - a) {
                line.render(&mut self.font);
                let pad_x =
                    (self.buf.width() - line.chars.len() * self.font.unit_width() as usize) as i32;
                let ofs_y = (i * self.font.unit_height() as usize) as i32;
                self.buf.blit(pad_x / 2, pad_y / 2 + ofs_y, &line.buf);
            }
            self.render_diff = None;
        }
    }
}

#[derive(PartialEq, Eq, PartialOrd, Ord, Debug, Clone, Copy)]
struct State {
    fg: Color,
    bg: Color,
    font_style: FontStyle,
    cursor: (usize, usize),
}

impl Default for State {
    fn default() -> Self {
        Self {
            fg: Color::WHITE,
            bg: Color::BLACK,
            font_style: FontStyle::Normal,
            cursor: (0, 0),
        }
    }
}

#[derive(Debug, Clone)]
struct Line {
    chars: Vec<Char>,
    buf: VecBuffer,
    render_diff: RenderDiff,
}

impl Line {
    fn new(parent_buf: &impl FrameBuffer, state: &State, font: &MonospaceFont) -> Self {
        let width = parent_buf.width() / font.unit_width() as usize;
        Self {
            chars: vec![Char::empty(state); width],
            buf: VecBuffer::new(
                width * font.unit_width() as usize,
                font.unit_height() as usize,
                parent_buf.format(),
            ),
            render_diff: Some((0, width)),
        }
    }

    fn clear(&mut self, state: &State) -> bool {
        let mut start = usize::MAX;
        let mut end = 0;
        for (i, c) in self.chars.iter_mut().enumerate() {
            if c.clear(state) {
                start = start.min(i);
                end = end.max(i + 1);
            }
        }
        if start < end {
            extend_render_diff(&mut self.render_diff, start, end);
            true
        } else {
            false
        }
    }

    fn put(&mut self, c: char, cursor: usize, state: &State) -> LinePutResult {
        if c == '\n' {
            LinePutResult::LineFeed
        } else if cursor >= self.chars.len() {
            LinePutResult::Wrapping(c)
        } else if self.chars[cursor].update(c, state) {
            extend_render_diff(&mut self.render_diff, cursor, cursor + 1);
            LinePutResult::Next(true, cursor + 1)
        } else {
            LinePutResult::Next(false, cursor + 1)
        }
    }

    fn render(&mut self, font: &mut MonospaceFont) {
        if let Some((a, b)) = self.render_diff {
            for (i, c) in self.chars.iter().copied().enumerate().skip(a).take(b - a) {
                let ofs_x = (i * font.unit_width() as usize) as i32;
                c.render_to(&mut self.buf, ofs_x, 0, font);
            }
            self.render_diff = None;
        }
    }
}

#[derive(Debug, Clone, Copy)]
enum LinePutResult {
    LineFeed,
    Wrapping(char),
    Next(bool, usize),
}

#[derive(PartialEq, Eq, PartialOrd, Ord, Debug, Clone, Copy)]
struct Char {
    // Since MonospaceFont caches the rendered glyphs, Char does not hold a VecBuffer.
    value: char,
    fg: Color,
    bg: Color,
    font_style: FontStyle,
}

impl Char {
    const EMPTY_CHAR: char = ' ';

    fn new(value: char, state: &State) -> Self {
        Self {
            value,
            fg: state.fg,
            bg: state.bg,
            font_style: state.font_style,
        }
    }

    fn empty(state: &State) -> Self {
        Self::new(Self::EMPTY_CHAR, state)
    }

    fn clear(&mut self, state: &State) -> bool {
        self.update(Self::EMPTY_CHAR, state)
    }

    fn update(&mut self, value: char, state: &State) -> bool {
        if *self != Self::new(value, state) {
            *self = Self::new(value, state);
            true
        } else {
            false
        }
    }

    fn render_to(&self, buf: &mut impl FrameBuffer, x: i32, y: i32, font: &mut MonospaceFont) {
        buf.blit(
            x,
            y,
            font.get(self.value, self.fg, self.bg, self.font_style),
        );
    }
}

type RenderDiff = Option<(usize, usize)>;

fn extend_render_diff(a: &mut RenderDiff, start: usize, end: usize) {
    *a = match *a {
        None => Some((start, end)),
        Some((a, b)) => Some((a.min(start), b.max(end))),
    };
}

// Workaround for linker error

#[no_mangle]
#[doc(hidden)]
pub extern "C" fn fminf(x: f32, y: f32) -> f32 {
    libm::fminf(x, y)
}

#[no_mangle]
#[doc(hidden)]
pub extern "C" fn fmaxf(x: f32, y: f32) -> f32 {
    libm::fmaxf(x, y)
}
