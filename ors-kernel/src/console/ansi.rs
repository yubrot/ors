//! This module handles a subset of ANSI escape codes.

use super::Input;
use core::convert::{TryFrom, TryInto};
use log::trace;

#[derive(Debug)]
pub struct Decoder {
    state: State,
}

impl Decoder {
    pub fn new() -> Self {
        Self { state: State::Init }
    }

    pub fn add_char(&mut self, ch: char) -> Option<DecodeResult> {
        use State::*;

        fn param(n: Option<u32>, ch: char) -> Option<u32> {
            Some(match n {
                Some(n) => n * 10 + ch.to_digit(10).unwrap(),
                None => ch.to_digit(10).unwrap(),
            })
        }

        match (ch, self.state) {
            ('\x1b', Init) => self.continue_state(Esc),
            ('\x08' | '\x09' | '\x0a' | '\x7f' | ' '..='~', Init) => {
                self.complete_state(DecodeResult::Just(ch))
            }
            ('[', Esc) => self.continue_state(Csi(None)), // Control Sequence Introducer
            ('0'..='9', Csi(n)) => self.continue_state(Csi(param(n, ch))),
            ('0'..='9', Csi2(n, m)) => self.continue_state(Csi2(n, param(m, ch))),
            ('0'..='9', Csi3(n, m, l)) => self.continue_state(Csi3(n, m, param(l, ch))),
            (';', Csi(n)) => self.continue_state(Csi2(n, None)),
            (';', Csi2(n, m)) => self.continue_state(Csi3(n, m, None)),
            (';', Csi3(n, m, _)) => {
                trace!("ansi: Unsupported ;: {:?}", self.state);
                self.continue_state(Csi3(n, m, None)) // overwrite third parameter
            }
            (c, Csi(n)) => match EscapeSequence::from_csi(n, None, None, c) {
                Ok(es) => self.complete_state(DecodeResult::EscapeSequence(es)),
                Err(()) => self.incomplete_state(ch),
            },
            (c, Csi2(n, m)) => match EscapeSequence::from_csi(n, m, None, c) {
                Ok(es) => self.complete_state(DecodeResult::EscapeSequence(es)),
                Err(()) => self.incomplete_state(ch),
            },
            (c, Csi3(n, m, l)) => match EscapeSequence::from_csi(n, m, l, c) {
                Ok(es) => self.complete_state(DecodeResult::EscapeSequence(es)),
                Err(()) => self.incomplete_state(ch),
            },
            _ => self.incomplete_state(ch),
        }
    }

    fn continue_state(&mut self, state: State) -> Option<DecodeResult> {
        self.state = state;
        None
    }

    fn complete_state(&mut self, result: DecodeResult) -> Option<DecodeResult> {
        self.state = State::Init;
        Some(result)
    }

    fn incomplete_state(&mut self, ch: char) -> Option<DecodeResult> {
        match self.state {
            State::Init => {
                trace!("ansi: Unhandled character: {} ({:x})", ch, ch as u32);
                None
            }
            state => {
                trace!(
                    "ansi: Unhandled character at {:?}: {} ({:x})",
                    state,
                    ch,
                    ch as u32
                );
                self.state = State::Init;
                self.add_char(ch)
            }
        }
    }
}

#[derive(PartialEq, Eq, PartialOrd, Ord, Debug, Clone, Copy)]
enum State {
    Init,
    Esc,                                         // ^[
    Csi(Option<u32>),                            // ^[ [ n
    Csi2(Option<u32>, Option<u32>),              // ^[ [ n ; m
    Csi3(Option<u32>, Option<u32>, Option<u32>), // ^[ [ n ; m ; l
}

#[derive(PartialEq, Eq, PartialOrd, Ord, Debug, Clone, Copy, Hash)]
pub enum DecodeResult {
    Just(char),
    EscapeSequence(EscapeSequence),
}

impl TryFrom<DecodeResult> for Input {
    type Error = ();

    fn try_from(value: DecodeResult) -> Result<Self, Self::Error> {
        match value {
            DecodeResult::Just(a) => Ok(Input::Char(a)),
            DecodeResult::EscapeSequence(s) => s.try_into(),
        }
    }
}

#[derive(PartialEq, Eq, PartialOrd, Ord, Debug, Clone, Copy, Hash)]
pub enum EscapeSequence {
    CursorUp(u32),
    CursorDown(u32),
    CursorForward(u32),
    CursorBack(u32),
    CursorNextLine(u32),
    CursorPreviousLine(u32),
    CursorHorizontalAbsolute(u32),
    CursorPosition(u32, u32),
    EraseInDisplay(u32),
    EraseInLine(u32),
    HorizontalVerticalPosition(u32, u32),
    Sgr(Sgr),
    Sgr2(Sgr, Sgr),
    Sgr3(Sgr, Sgr, Sgr),
    Home,
    Insert,
    Delete,
    End,
    PgUp,
    PgDn,
}

impl EscapeSequence {
    pub fn from_csi(n: Option<u32>, m: Option<u32>, l: Option<u32>, ch: char) -> Result<Self, ()> {
        use EscapeSequence::*;
        Ok(match ch {
            'A' => CursorUp(n.unwrap_or(1)),
            'B' => CursorDown(n.unwrap_or(1)),
            'C' => CursorForward(n.unwrap_or(1)),
            'D' => CursorBack(n.unwrap_or(1)),
            'E' => CursorNextLine(n.unwrap_or(1)),
            'F' => CursorPreviousLine(n.unwrap_or(1)),
            'G' => CursorHorizontalAbsolute(n.unwrap_or(1)),
            'H' => CursorPosition(n.unwrap_or(1), m.unwrap_or(1)),
            'J' => EraseInDisplay(n.unwrap_or(0)),
            'K' => EraseInLine(n.unwrap_or(0)),
            'f' => HorizontalVerticalPosition(n.unwrap_or(1), m.unwrap_or(1)),
            'm' => Self::from_sgr_params(n.unwrap_or(0), m, l)?,
            '~' => match n.ok_or(())? {
                1 => Home,
                2 => Insert,
                3 => Delete,
                4 => End,
                5 => PgUp,
                6 => PgDn,
                7 => Home,
                8 => End,
                _ => Err(())?,
            },
            _ => Err(())?,
        })
    }

    pub fn from_sgr_params(n: u32, m: Option<u32>, l: Option<u32>) -> Result<Self, ()> {
        Ok(match (n, m, l) {
            (38, Some(5), Some(n)) => Self::Sgr(Sgr::Fg(Color::from_256(n)?)),
            (48, Some(5), Some(n)) => Self::Sgr(Sgr::Bg(Color::from_256(n)?)),
            (n, None, None) => Self::Sgr(Sgr::from_param(n)?),
            (n, Some(m), None) => Self::Sgr2(Sgr::from_param(n)?, Sgr::from_param(m)?),
            (n, Some(m), Some(l)) => Self::Sgr3(
                Sgr::from_param(n)?,
                Sgr::from_param(m)?,
                Sgr::from_param(l)?,
            ),
            _ => Err(())?,
        })
    }
}

impl TryFrom<EscapeSequence> for Input {
    type Error = ();

    fn try_from(value: EscapeSequence) -> Result<Self, Self::Error> {
        Ok(match value {
            EscapeSequence::CursorUp(1) => Input::ArrowUp,
            EscapeSequence::CursorDown(1) => Input::ArrowDown,
            EscapeSequence::CursorForward(1) => Input::ArrowRight,
            EscapeSequence::CursorBack(1) => Input::ArrowLeft,
            EscapeSequence::CursorPreviousLine(1) => Input::End,
            EscapeSequence::CursorPosition(1, 1) => Input::Home,
            EscapeSequence::Home => Input::Home,
            EscapeSequence::Insert => Input::Insert,
            EscapeSequence::Delete => Input::Char('\x7f'),
            EscapeSequence::End => Input::End,
            EscapeSequence::PgUp => Input::PageUp,
            EscapeSequence::PgDn => Input::PageDown,
            _ => Err(())?,
        })
    }
}

/// Select Graphic Rendition
#[derive(PartialEq, Eq, PartialOrd, Ord, Debug, Clone, Copy, Hash)]
pub enum Sgr {
    Reset,
    Bold,
    Faint,
    ResetBoldFaint,
    Italic(bool),
    Underline(bool),
    Blinking(bool),
    Inverse(bool),
    Hidden(bool),
    Strikethrough(bool),
    Fg(Color),
    Bg(Color),
}

impl Sgr {
    pub fn from_param(n: u32) -> Result<Self, ()> {
        use NamedColor::*;
        use NamedColorVariation::*;
        use Sgr::*;
        Ok(match n {
            0 => Reset,
            1 => Bold,
            2 => Faint,
            3 => Italic(true),
            4 => Underline(true),
            5 => Blinking(true),
            7 => Inverse(true),
            8 => Hidden(true),
            9 => Strikethrough(true),
            22 => ResetBoldFaint,
            23 => Italic(false),
            24 => Underline(false),
            25 => Blinking(false),
            27 => Inverse(false),
            28 => Hidden(false),
            29 => Strikethrough(false),
            30 => Fg(Color::Named(Black, Dimmer)),
            31 => Fg(Color::Named(Red, Dimmer)),
            32 => Fg(Color::Named(Green, Dimmer)),
            33 => Fg(Color::Named(Yellow, Dimmer)),
            34 => Fg(Color::Named(Blue, Dimmer)),
            35 => Fg(Color::Named(Magenta, Dimmer)),
            36 => Fg(Color::Named(Cyan, Dimmer)),
            37 => Fg(Color::Named(White, Dimmer)),
            39 => Fg(Color::Default),
            40 => Bg(Color::Named(Black, ForceDimmer)),
            41 => Bg(Color::Named(Red, ForceDimmer)),
            42 => Bg(Color::Named(Green, ForceDimmer)),
            43 => Bg(Color::Named(Yellow, ForceDimmer)),
            44 => Bg(Color::Named(Blue, ForceDimmer)),
            45 => Bg(Color::Named(Magenta, ForceDimmer)),
            46 => Bg(Color::Named(Cyan, ForceDimmer)),
            47 => Bg(Color::Named(White, ForceDimmer)),
            49 => Bg(Color::Default),
            90 => Fg(Color::Named(Black, ForceBrighter)),
            91 => Fg(Color::Named(Red, ForceBrighter)),
            92 => Fg(Color::Named(Green, ForceBrighter)),
            93 => Fg(Color::Named(Yellow, ForceBrighter)),
            94 => Fg(Color::Named(Blue, ForceBrighter)),
            95 => Fg(Color::Named(Magenta, ForceBrighter)),
            96 => Fg(Color::Named(Cyan, ForceBrighter)),
            97 => Fg(Color::Named(White, ForceBrighter)),
            100 => Bg(Color::Named(Black, ForceBrighter)),
            101 => Bg(Color::Named(Red, ForceBrighter)),
            102 => Bg(Color::Named(Green, ForceBrighter)),
            103 => Bg(Color::Named(Yellow, ForceBrighter)),
            104 => Bg(Color::Named(Blue, ForceBrighter)),
            105 => Bg(Color::Named(Magenta, ForceBrighter)),
            106 => Bg(Color::Named(Cyan, ForceBrighter)),
            107 => Bg(Color::Named(White, ForceBrighter)),
            _ => Err(())?,
        })
    }
}

#[derive(PartialEq, Eq, PartialOrd, Ord, Debug, Clone, Copy, Hash)]
pub enum Color {
    Default,
    Named(NamedColor, NamedColorVariation),
    Rgb(u8),       // 0..=215, 36 * r + 6 * g + b (0 <= r, g, b <= 5)
    Grayscale(u8), // 0..=23, black to white
}

impl Color {
    pub fn from_256(n: u32) -> Result<Color, ()> {
        use Color::*;
        use NamedColor::*;
        use NamedColorVariation::*;
        Ok(match n {
            0 => Named(Black, Dimmer),
            1 => Named(Red, Dimmer),
            2 => Named(Green, Dimmer),
            3 => Named(Yellow, Dimmer),
            4 => Named(Blue, Dimmer),
            5 => Named(Magenta, Dimmer),
            6 => Named(Cyan, Dimmer),
            7 => Named(White, Dimmer),
            8 => Named(Black, ForceBrighter),
            9 => Named(Red, ForceBrighter),
            10 => Named(Green, ForceBrighter),
            11 => Named(Yellow, ForceBrighter),
            12 => Named(Blue, ForceBrighter),
            13 => Named(Magenta, ForceBrighter),
            14 => Named(Cyan, ForceBrighter),
            15 => Named(White, ForceBrighter),
            16..=231 => Rgb((n - 16) as u8),
            232..=255 => Grayscale((n - 232) as u8),
            _ => Err(())?,
        })
    }

    pub fn brighter(self) -> Self {
        match self {
            Self::Named(color, NamedColorVariation::Dimmer) => {
                Self::Named(color, NamedColorVariation::Brighter)
            }
            _ => self,
        }
    }

    pub fn dimmer(self) -> Self {
        match self {
            Self::Named(color, NamedColorVariation::Brighter) => {
                Self::Named(color, NamedColorVariation::Dimmer)
            }
            _ => self,
        }
    }
}

#[derive(PartialEq, Eq, PartialOrd, Ord, Debug, Clone, Copy, Hash)]
pub enum NamedColor {
    Black,
    Red,
    Green,
    Yellow,
    Blue,
    Magenta,
    Cyan,
    White,
}

#[derive(PartialEq, Eq, PartialOrd, Ord, Debug, Clone, Copy, Hash)]
pub enum NamedColorVariation {
    ForceDimmer,
    Dimmer,   // switch to brighter if bold
    Brighter, // switch to dimmer if faint
    ForceBrighter,
}

impl NamedColorVariation {
    pub fn is_dimmer(self) -> bool {
        matches!(self, Self::ForceDimmer | Self::Dimmer)
    }
}

pub trait ColorScheme {
    fn get_fg(&self, color: Color) -> (u8, u8, u8) {
        self.get(color).unwrap_or(self.foreground())
    }

    fn get_bg(&self, color: Color) -> (u8, u8, u8) {
        self.get(color).unwrap_or(self.background())
    }

    fn get(&self, color: Color) -> Option<(u8, u8, u8)> {
        Some(match color {
            Color::Default => None?,
            Color::Named(color, variation) => match (color, variation.is_dimmer()) {
                (NamedColor::Black, true) => self.black(),
                (NamedColor::Black, false) => self.bright_black(),
                (NamedColor::Red, true) => self.red(),
                (NamedColor::Red, false) => self.bright_red(),
                (NamedColor::Green, true) => self.green(),
                (NamedColor::Green, false) => self.bright_green(),
                (NamedColor::Yellow, true) => self.yellow(),
                (NamedColor::Yellow, false) => self.bright_yellow(),
                (NamedColor::Blue, true) => self.blue(),
                (NamedColor::Blue, false) => self.bright_blue(),
                (NamedColor::Magenta, true) => self.magenta(),
                (NamedColor::Magenta, false) => self.bright_magenta(),
                (NamedColor::Cyan, true) => self.cyan(),
                (NamedColor::Cyan, false) => self.bright_cyan(),
                (NamedColor::White, true) => self.white(),
                (NamedColor::White, false) => self.bright_white(),
            },
            Color::Rgb(n) => {
                let b = n % 6;
                let g = ((n - b) / 6) % 6;
                let r = (((n - b) / 6) - g) / 6;
                (51 * r, 51 * g, 51 * b)
            }
            Color::Grayscale(23) => (255, 255, 255),
            Color::Grayscale(n) => (n * 11, n * 11, n * 11),
        })
    }

    fn foreground(&self) -> (u8, u8, u8);
    fn background(&self) -> (u8, u8, u8);
    fn black(&self) -> (u8, u8, u8);
    fn red(&self) -> (u8, u8, u8);
    fn green(&self) -> (u8, u8, u8);
    fn yellow(&self) -> (u8, u8, u8);
    fn blue(&self) -> (u8, u8, u8);
    fn magenta(&self) -> (u8, u8, u8);
    fn cyan(&self) -> (u8, u8, u8);
    fn white(&self) -> (u8, u8, u8);
    fn bright_black(&self) -> (u8, u8, u8);
    fn bright_red(&self) -> (u8, u8, u8);
    fn bright_green(&self) -> (u8, u8, u8);
    fn bright_yellow(&self) -> (u8, u8, u8);
    fn bright_blue(&self) -> (u8, u8, u8);
    fn bright_magenta(&self) -> (u8, u8, u8);
    fn bright_cyan(&self) -> (u8, u8, u8);
    fn bright_white(&self) -> (u8, u8, u8);
}
