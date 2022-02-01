//! A rough shell implementation for debugging.

use crate::console::{input_queue, Input};
use crate::devices;
use crate::devices::virtio::block;
use crate::fs::fat;
use crate::fs::volume::virtio::VirtIOBlockVolume;
use crate::interrupts::{ticks, TIMER_FREQ};
use crate::phys_memory::frame_manager;
use alloc::borrow::ToOwned;
use alloc::string::String;
use alloc::vec::Vec;
use core::fmt;

static CLEAR: &str = "\x1b[H\x1b[2J";
static INPUT_START: &str = "\x1b[G\x1b[32m$\x1b[0m ";
static INPUT_END: &str = "\x1b[K";
static CURSOR_START: &str = "\x1b[30;47m";
static CURSOR_END: &str = "\x1b[0m";

pub extern "C" fn run(_: u64) -> ! {
    let mut command_buf = String::new();
    let mut cursor = 0;
    let mut ctx = Context {
        wd: Path::new(),
        fs: fat::FileSystem::new(VirtIOBlockVolume::new(&block::list()[0])).unwrap(),
    };

    cprint!("{}", CLEAR);
    kprintln!("[ors shell]");

    loop {
        kprint!("{}", INPUT_START);
        for (i, c) in command_buf.chars().enumerate() {
            if i == cursor {
                kprint!("{}{}{}", CURSOR_START, c, CURSOR_END);
            } else {
                kprint!("{}", c);
            }
        }
        if cursor == command_buf.chars().count() {
            kprint!("{} {}", CURSOR_START, CURSOR_END);
        }
        kprint!("{}", INPUT_END);

        match input_queue().dequeue() {
            Input::Char('\n') => {
                kprintln!("{}{}{}", INPUT_START, &command_buf, INPUT_END);
                let t = ticks();
                execute_command(&command_buf, &mut ctx);
                let t = ticks() - t;
                command_buf.clear();
                cursor = 0;
                kprintln!(
                    "elapsed = {}ms",
                    (t as f64 / TIMER_FREQ as f64 * 1000.0) as u32
                );
            }
            Input::Char('\x08' /* BS */) if 0 < cursor => {
                cursor -= 1;
                command_buf.remove(cursor);
            }
            Input::Char('\x7f' /* DEL */) if cursor < command_buf.len() => {
                command_buf.remove(cursor);
            }
            Input::Char(c) if ' ' <= c && c <= '~' => {
                command_buf.insert(cursor, c);
                cursor += 1;
            }
            Input::Home => cursor = 0,
            Input::End => cursor = command_buf.len(),
            Input::ArrowLeft if 0 < cursor => cursor -= 1,
            Input::ArrowRight if cursor < command_buf.len() => cursor += 1,
            _ => {}
        }
    }
}

#[derive(Debug)]
struct Context {
    wd: Path,
    fs: fat::FileSystem<VirtIOBlockVolume>, // TODO: Move to appropriate static location
}

fn execute_command(command_buf: &str, ctx: &mut Context) {
    let command_and_args = command_buf.trim().split_whitespace().collect::<Vec<_>>();
    let (command, args) = match command_and_args.first() {
        Some(c) => (*c, &command_and_args[1..]),
        None => return,
    };

    match command {
        "clear" => kprint!("{}", CLEAR),
        "pwd" => kprintln!("{}", ctx.wd),
        "cd" => match args.first() {
            Some(path) => {
                let path = ctx.wd.joined(path);
                match path.get_dir(&ctx.fs) {
                    Some(_) => ctx.wd = path,
                    None => kprintln!("Not a directory: {}", path),
                }
            }
            None => ctx.wd.parts.clear(),
        },
        "ls" => match ctx.wd.get_dir(&ctx.fs) {
            Some(dir) => {
                for f in dir.files() {
                    if f.is_dir() {
                        kprintln!("{}/", f.name());
                    } else {
                        kprintln!("{} ({})", f.name(), PrettySize(f.file_size()));
                    }
                }
            }
            None => kprintln!("Directory not found: {}", ctx.wd),
        },
        "read" => match args.first() {
            Some(path) => {
                let path = ctx.wd.joined(path);
                match path.get_file(&ctx.fs) {
                    Some(file) => match file.reader() {
                        Some(reader) => match reader.read_to_end() {
                            Ok(buf) => match String::from_utf8(buf) {
                                Ok(s) => kprintln!("{}", s),
                                Err(e) => kprintln!("<binary file ({} bytes)>", e.as_bytes().len()),
                            },
                            Err(e) => kprintln!("Read error: {}", e),
                        },
                        None => kprintln!("This is a directory: {}", path),
                    },
                    None => kprintln!("File not found: {}", path),
                }
            }
            None => kprintln!("read <file>"),
        },
        "write" | "append" => match args.first() {
            Some(path) => {
                let path = ctx.wd.joined(path);
                match path.get_file(&ctx.fs) {
                    Some(mut file) => match if command == "write" {
                        file.overwriter()
                    } else {
                        file.appender()
                    } {
                        Some(mut writer) => {
                            let mut s = args[1..].join(" ").to_owned();
                            if !s.is_empty() {
                                s.push('\n');
                            }
                            if let Err(e) = writer.write(s.as_bytes()) {
                                kprintln!("Write error: {}", e);
                            }
                        }
                        None => kprintln!("This is a directory: {}", path),
                    },
                    None => kprintln!("File not found: {}", path),
                }
                let _ = ctx.fs.commit();
            }
            None => kprintln!("write|append <file> <text>"),
        },
        "rm" | "rmr" => match args.first() {
            Some(path) => {
                let path = ctx.wd.joined(path);
                match path.get_file(&ctx.fs) {
                    Some(file) => match file.remove(command == "rmr") {
                        Ok(_) => {}
                        Err(e) => kprintln!("Failed to remove {}: {}", path, e),
                    },
                    None => kprintln!("File not found: {}", path),
                }
                let _ = ctx.fs.commit();
            }
            None => kprintln!("rm|rmr <file>"),
        },
        "mv" => match &args[..] {
            [src, dest] => {
                let src = ctx.wd.joined(src);
                let dest = ctx.wd.joined(dest);
                match src.get_file(&ctx.fs) {
                    Some(src) => match dest.get_dir(&ctx.fs) {
                        Some(dest) => match src.mv(Some(dest), None) {
                            Ok(_) => {}
                            Err(e) => kprintln!("Failed to move file: {}", e),
                        },
                        None => match dest.get_file(&ctx.fs) {
                            Some(_) => kprintln!("File already exists: {}", dest),
                            None => {
                                let (dest_dir, file_name) = dest.dir_and_file_name().unwrap();
                                match dest_dir.get_dir(&ctx.fs) {
                                    Some(dest_dir) => {
                                        match src.mv(Some(dest_dir), Some(file_name.as_str())) {
                                            Ok(_) => {}
                                            Err(e) => kprintln!("Failed to move file: {}", e),
                                        }
                                    }
                                    None => {
                                        kprintln!("Destination directory not found: {}", dest_dir);
                                    }
                                }
                            }
                        },
                    },
                    None => kprintln!("Source file not found: {}", src),
                }
                let _ = ctx.fs.commit();
            }
            _ => kprintln!("mv <src> <dest>"),
        },
        "memstats" => {
            kprintln!("[phys_memory]");
            let mut graph = [0.0; 100];
            let (total, available) = {
                let fm = frame_manager();
                let total = fm.total_frames();
                let available = fm.available_frames();
                for i in 0..100 {
                    graph[i] = fm.availability_in_range(i as f64 / 100.0, (i + 1) as f64 / 100.0);
                }
                (total, available)
            };
            for a in graph {
                kprint!("\x1b[48;5;{}m \x1b[0m", 232 + (23.0 * a) as usize);
            }
            kprintln!();
            kprintln!(
                "{}/{} frames ({}/{})",
                available,
                total,
                PrettySize(available * 4096),
                PrettySize(total * 4096)
            );
        }
        "lspci" => {
            for d in devices::pci::devices() {
                unsafe {
                    let ty = d.device_type();
                    kprintln!("{:02x}:{:02x}.{:02x} = {{", d.bus, d.device, d.function);
                    kprint!("  vendor_id = {:x}", d.vendor_id());
                    if d.is_vendor_intel() {
                        kprint!(" (intel)");
                    }
                    kprintln!();
                    kprint!("  device_id = {:x}", d.device_id());
                    if d.is_virtio() {
                        kprint!(" (virtio)");
                    }
                    kprintln!();
                    kprintln!(
                        "  device_type = {{ class_code = {:02x}, subclass = {:02x}, interface = {:02x} }}",
                        ty.class_code,
                        ty.subclass,
                        ty.prog_interface
                    );
                    if d.is_virtio() {
                        kprintln!("  subsystem_id = {}", d.subsystem_id());
                    }
                    if let Some(msi_x) = d.msi_x() {
                        kprintln!("  msi-x = {{ table_size = {} }}", msi_x.table_size());
                    }
                    kprintln!("}}");
                }
            }
        }
        "color" => {
            fn p(n: i32) {
                kprint!("\x1b[48;5;{}m{:>4}\x1b[0m", n, n);
            }

            for i in 0..16 {
                p(i);
                if i % 8 == 7 {
                    kprintln!();
                }
            }
            kprintln!();

            for i in 0..2 {
                for j in 0..6 {
                    for k in 0..3 {
                        for l in 0..6 {
                            p(16 + l + 36 * k + 6 * j + 108 * i);
                        }
                        kprint!(" ");
                    }
                    kprintln!();
                }
                kprintln!();
            }

            for i in 232..256 {
                p(i);
            }
            kprintln!();
            kprintln!();
        }
        "shutdown" => devices::qemu::exit(devices::qemu::ExitCode::Success),
        cmd => kprintln!("Unsupported command: {}", cmd),
    }
}

#[derive(Debug, Clone)]
struct Path {
    parts: Vec<String>,
}

impl Path {
    fn new() -> Self {
        Self { parts: Vec::new() }
    }

    fn joined(&self, path: &str) -> Self {
        let mut p = self.clone();
        p.join(path);
        p
    }

    fn join(&mut self, path: &str) {
        for p in path.split('/') {
            match p {
                ".." => {
                    self.parts.pop();
                }
                "" | "." => {}
                p => self.parts.push(p.to_owned()),
            }
        }
    }

    fn dir_and_file_name(mut self) -> Option<(Path, String)> {
        let file_name = self.parts.pop()?;
        Some((self, file_name))
    }

    fn get_dir<'a>(
        &self,
        fs: &'a fat::FileSystem<VirtIOBlockVolume>,
    ) -> Option<fat::Dir<'a, VirtIOBlockVolume>> {
        if self.parts.is_empty() {
            Some(fs.root_dir())
        } else {
            self.get_file(fs)?.as_dir()
        }
    }

    fn get_file<'a>(
        &self,
        fs: &'a fat::FileSystem<VirtIOBlockVolume>,
    ) -> Option<fat::File<'a, VirtIOBlockVolume>> {
        if self.parts.is_empty() {
            None
        } else {
            let mut dir = fs.root_dir();
            let last_index = self.parts.len() - 1;
            for p in self.parts[0..last_index].iter() {
                dir = dir.files().find(|f| f.name() == p)?.as_dir()?;
            }
            dir.files().find(|f| f.name() == &self.parts[last_index])
        }
    }
}

impl fmt::Display for Path {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.parts.is_empty() {
            write!(f, "/")?;
        } else {
            for p in self.parts.iter() {
                write!(f, "/{}", p)?;
            }
        }
        Ok(())
    }
}

struct PrettySize(usize);

impl fmt::Display for PrettySize {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.0 < 1024 {
            write!(f, "{}B", self.0)
        } else if self.0 < 1024 * 1024 {
            write!(f, "{:.2}KiB", (self.0 as f64) / 1024.0)
        } else if self.0 < 1024 * 1024 * 1024 {
            write!(f, "{:.2}MiB", (self.0 as f64) / (1024.0 * 1024.0))
        } else {
            write!(f, "{:.2}GiB", (self.0 as f64) / (1024.0 * 1024.0 * 1024.0))
        }
    }
}
