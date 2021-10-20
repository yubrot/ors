//! This module handles a subset of ANSI escape codes.

use super::Input;
use core::convert::{TryFrom, TryInto};
use log::trace;

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
    Home,
    Insert,
    Delete,
    End,
    PgUp,
    PgDn,
}

impl TryFrom<EscapeSequence> for Input {
    type Error = ();

    fn try_from(value: EscapeSequence) -> Result<Self, Self::Error> {
        match value {
            EscapeSequence::CursorUp(1) => Ok(Input::ArrowUp),
            EscapeSequence::CursorDown(1) => Ok(Input::ArrowDown),
            EscapeSequence::CursorForward(1) => Ok(Input::ArrowRight),
            EscapeSequence::CursorBack(1) => Ok(Input::ArrowLeft),
            EscapeSequence::CursorPreviousLine(1) => Ok(Input::End),
            EscapeSequence::CursorPosition(1, 1) => Ok(Input::Home),
            EscapeSequence::Home => Ok(Input::Home),
            EscapeSequence::Insert => Ok(Input::Insert),
            EscapeSequence::Delete => Ok(Input::Char('\x7f')),
            EscapeSequence::End => Ok(Input::End),
            EscapeSequence::PgUp => Ok(Input::PageUp),
            EscapeSequence::PgDn => Ok(Input::PageDown),
            _ => Err(()),
        }
    }
}

#[derive(Debug)]
pub struct Decoder {
    state: State,
}

impl Decoder {
    pub fn new() -> Self {
        Self { state: State::Init }
    }

    fn set_state(&mut self, state: State) -> Option<DecodeResult> {
        self.state = state;
        None
    }

    fn consume_state(&mut self, seq: EscapeSequence) -> Option<DecodeResult> {
        self.state = State::Init;
        Some(DecodeResult::EscapeSequence(seq))
    }

    pub fn add_char(&mut self, ch: char) -> Option<DecodeResult> {
        use EscapeSequence::*;
        use State::*;

        fn param(ch: char) -> u32 {
            ch.to_digit(10).unwrap()
        }

        match (self.state, ch) {
            (Init, '\x1b') => self.set_state(Esc),
            (Init, '\x08' | '\x09' | '\x0a' | '\x7f' | ' '..='~') => Some(DecodeResult::Just(ch)),
            (Esc, '[') => self.set_state(Csi(None)),
            (Csi(None), '0'..='9') => self.set_state(Csi(Some(param(ch)))),
            (Csi(Some(n)), '0'..='9') => self.set_state(Csi(Some(n * 10 + param(ch)))),
            (Csi(n), ';') => self.set_state(Csi2(n, None)),
            (Csi2(n, None), '0'..='9') => self.set_state(Csi2(n, Some(param(ch)))),
            (Csi2(n, Some(m)), '0'..='9') => self.set_state(Csi2(n, Some(m * 10 + param(ch)))),
            (Csi(n), 'A') => self.consume_state(CursorUp(n.unwrap_or(1))),
            (Csi(n), 'B') => self.consume_state(CursorDown(n.unwrap_or(1))),
            (Csi(n), 'C') => self.consume_state(CursorForward(n.unwrap_or(1))),
            (Csi(n), 'D') => self.consume_state(CursorBack(n.unwrap_or(1))),
            (Csi(n), 'E') => self.consume_state(CursorNextLine(n.unwrap_or(1))),
            (Csi(n), 'F') => self.consume_state(CursorPreviousLine(n.unwrap_or(1))),
            (Csi(n), 'G') => self.consume_state(CursorHorizontalAbsolute(n.unwrap_or(1))),
            (Csi(n), 'H') => self.consume_state(CursorPosition(n.unwrap_or(1), 1)),
            (Csi2(n, m), 'H') => self.consume_state(CursorPosition(n.unwrap_or(1), m.unwrap_or(1))),
            (Csi(Some(1)), '~') => self.consume_state(Home),
            (Csi(Some(2)), '~') => self.consume_state(Insert),
            (Csi(Some(3)), '~') => self.consume_state(Delete),
            (Csi(Some(4)), '~') => self.consume_state(End),
            (Csi(Some(5)), '~') => self.consume_state(PgUp),
            (Csi(Some(6)), '~') => self.consume_state(PgDn),
            (Csi(Some(7)), '~') => self.consume_state(Home),
            (Csi(Some(8)), '~') => self.consume_state(End),
            (state, ch) => {
                trace!(
                    "ansi: Unhandled character at {:?}: {} ({:x})",
                    state,
                    ch,
                    ch as u32
                );
                self.state = State::Init;
                Some(DecodeResult::Incomplete(ch))
            }
        }
    }
}

#[derive(PartialEq, Eq, PartialOrd, Ord, Debug, Clone, Copy)]
enum State {
    Init,
    Esc,                            // ^[
    Csi(Option<u32>),               // ^[ [ n
    Csi2(Option<u32>, Option<u32>), // ^[ [ n ; m
}

#[derive(PartialEq, Eq, PartialOrd, Ord, Debug, Clone, Copy, Hash)]
pub enum DecodeResult {
    Just(char),
    EscapeSequence(EscapeSequence),
    Incomplete(char),
}

impl TryFrom<DecodeResult> for Input {
    type Error = ();

    fn try_from(value: DecodeResult) -> Result<Self, Self::Error> {
        match value {
            DecodeResult::Just(a) => Ok(Input::Char(a)),
            DecodeResult::EscapeSequence(s) => s.try_into(),
            DecodeResult::Incomplete(a) => Ok(Input::Char(a)),
        }
    }
}
