use std::ffi::OsStr;
use std::os::windows::ffi::OsStrExt;

pub fn wide(value: impl AsRef<OsStr>) -> Vec<u16> {
    value.as_ref().encode_wide().chain(Some(0)).collect()
}

pub fn point_from_lparam(value: isize) -> (i32, i32) {
    let x = (value & 0xffff) as i16 as i32;
    let y = ((value >> 16) & 0xffff) as i16 as i32;
    (x, y)
}
