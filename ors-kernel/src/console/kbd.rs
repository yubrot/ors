use super::Input;
use log::trace;
use pc_keyboard::layouts::Jis109Key;
use pc_keyboard::{DecodedKey, HandleControl, KeyCode, KeyState, Keyboard, ScancodeSet1};

pub struct Decoder {
    inner: Keyboard<Jis109Key, ScancodeSet1>,
    lctrl: bool,
    rctrl: bool,
}

impl Decoder {
    pub fn new() -> Self {
        Self {
            inner: Keyboard::new(Jis109Key, ScancodeSet1, HandleControl::Ignore),
            lctrl: false,
            rctrl: false,
        }
    }

    pub fn add(&mut self, byte: u8) -> Option<Input> {
        if let Ok(Some(e)) = self.inner.add_byte(byte) {
            if e.code == KeyCode::ControlLeft {
                self.lctrl = e.state == KeyState::Down;
            }
            if e.code == KeyCode::ControlRight {
                self.rctrl = e.state == KeyState::Down;
            }
            match self.inner.process_keyevent(e)? {
                DecodedKey::RawKey(KeyCode::Insert) => Some(Input::Insert),
                DecodedKey::RawKey(KeyCode::Home) => Some(Input::Home),
                DecodedKey::RawKey(KeyCode::End) => Some(Input::End),
                DecodedKey::RawKey(KeyCode::PageUp) => Some(Input::PageUp),
                DecodedKey::RawKey(KeyCode::PageDown) => Some(Input::PageDown),
                DecodedKey::RawKey(KeyCode::ArrowUp) => Some(Input::ArrowUp),
                DecodedKey::RawKey(KeyCode::ArrowDown) => Some(Input::ArrowDown),
                DecodedKey::RawKey(KeyCode::ArrowLeft) => Some(Input::ArrowLeft),
                DecodedKey::RawKey(KeyCode::ArrowRight) => Some(Input::ArrowRight),
                DecodedKey::Unicode(
                    // BS | HT | LF | DEL | printable characters
                    c @ ('\x08' | '\x09' | '\x0a' | '\x7f' | ' '..='~'),
                ) => {
                    if self.lctrl || self.rctrl {
                        Some(Input::Ctrl(c))
                    } else {
                        Some(Input::Char(c))
                    }
                }
                key => {
                    trace!("kbd: Unhandled key: {:?}", key);
                    None
                }
            }
        } else {
            None
        }
    }
}
