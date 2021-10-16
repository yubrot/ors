use crate::devices;
use crate::graphics;
use core::fmt;

#[derive(Debug)]
pub struct KernelWrite;

impl fmt::Write for KernelWrite {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        devices::serial::default_port().write_str(s)?;
        if let Some(mut sc) = graphics::screen_console_if_initialized() {
            sc.write_str(s)?;
        }
        Ok(())
    }
}

#[allow(unused_macros)]
macro_rules! kprintln {
    ($( $t:tt )*) => {{
        use core::fmt::Write;
        writeln!(crate::print::KernelWrite, $( $t )*).unwrap();
    }};
}

#[allow(unused_macros)]
macro_rules! kprint {
    ($( $t:tt )*) => {{
        use core::fmt::Write;
        write!(crate::print::KernelWrite, $( $t )*).unwrap();
    }};
}

/// Write to raw_default_port. Used for debugging output in interrupt handlers and panic handlers.
#[allow(unused_macros)]
macro_rules! sprintln {
    ($( $t:tt )*) => {{
        use core::fmt::Write;
        writeln!(crate::devices::serial::raw_default_port(), $( $t )*).unwrap();
    }};
}

#[allow(unused_macros)]
macro_rules! sprint {
    ($( $t:tt )*) => {{
        use core::fmt::Write;
        write!(crate::devices::serial::raw_default_port(), $( $t )*).unwrap();
    }};
}
