//! A thin wrapper of the UEFI file system protocol.

use alloc::boxed::Box;
use alloc::vec::Vec;
use uefi::prelude::*;
use uefi::proto::media::file::{
    Directory, File, FileAttribute, FileInfo, FileMode, FileType, RegularFile,
};

pub fn open_root_dir(image: Handle, bs: &BootServices) -> Directory {
    let sfs = bs.get_image_file_system(image).unwrap_success();
    unsafe { &mut *sfs.get() }.open_volume().unwrap_success()
}

pub fn create(dir: &mut Directory, filename: &str, create_dir: bool) -> FileType {
    let attr = if create_dir {
        FileAttribute::DIRECTORY
    } else {
        FileAttribute::empty()
    };
    dir.open(filename, FileMode::CreateReadWrite, attr)
        .expect_success("Failed to create file")
        .into_type()
        .unwrap_success()
}

pub fn open(dir: &mut Directory, filename: &str) -> FileType {
    dir.open(filename, FileMode::Read, FileAttribute::empty())
        .expect_success("Failed to open file")
        .into_type()
        .unwrap_success()
}

pub fn create_file(dir: &mut Directory, filename: &str) -> RegularFile {
    match create(dir, filename, false) {
        FileType::Regular(file) => file,
        FileType::Dir(_) => panic!("Not a regular file: {}", filename),
    }
}

pub fn open_file(dir: &mut Directory, filename: &str) -> RegularFile {
    match open(dir, filename) {
        FileType::Regular(file) => file,
        FileType::Dir(_) => panic!("Not a regular file: {}", filename),
    }
}

pub fn read_file_to_vec(file: &mut RegularFile) -> Vec<u8> {
    let size = get_file_info(file).file_size() as usize;
    let mut buf = vec![0; size];
    file.read(&mut buf).unwrap_success();
    buf
}

pub fn get_file_info(file: &mut impl File) -> Box<FileInfo> {
    file.get_boxed_info::<FileInfo>().unwrap_success()
}

macro_rules! fwrite {
    ($file:expr, $format:tt $( $rest:tt )*) => {
        $file.write(format!($format $( $rest )*).as_bytes()).unwrap_success()
    };
}

macro_rules! fwriteln {
    ($file:expr, $format:tt $( $rest:tt )*) => {
        fwrite!($file, concat!($format, "\n") $( $rest )*)
    };
}
