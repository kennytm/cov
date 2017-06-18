//! Additional methods for libstd and external crates.

use natord::compare_iter;
use serde_json::Value;

use std::cmp::Ordering;
use std::fs::remove_dir_all;
use std::io;
#[cfg(any(target_os = "redox", unix))]
use std::os::unix::ffi::OsStrExt;
#[cfg(windows)]
use std::os::windows::ffi::{OsStrExt, OsStringExt};
use std::path::Path;

pub trait OptionExt {
    type Value;
    fn unwrap_or_catch<E, F>(self, f: F) -> Result<Self::Value, E>
    where
        F: FnOnce() -> Result<Self::Value, E>;
}

impl<T> OptionExt for Option<T> {
    type Value = T;

    fn unwrap_or_catch<E, F>(self, f: F) -> Result<Self::Value, E>
    where
        F: FnOnce() -> Result<Self::Value, E>,
    {
        match self {
            Some(v) => Ok(v),
            None => f(),
        }
    }
}

pub fn clean_dir(dir: &Path) -> io::Result<()> {
    match remove_dir_all(dir) {
        Err(ref e) if e.kind() == io::ErrorKind::NotFound => Ok(()),
        res => res,
    }
}

macro_rules! do_compare {
    ($lhs:expr, $rhs:expr) => {
        compare_iter($lhs, $rhs, |_| false, |a, b| a.cmp(&b), |c| {
            match **c {
                b @ 0x30 ... 0x39 => Some((b - 0x30) as isize),
                _ => None,
            }
        })
    }
}

/// Compares two paths using natural sorting.
#[cfg(any(target_os = "redox", unix))]
pub fn compare_naturally(lhs: &Path, rhs: &Path) -> Ordering {
    let lhs = lhs.as_os_str().as_bytes().iter();
    let rhs = rhs.as_os_str().as_bytes().iter();
    do_compare!(lhs, rhs)
}

/// Compares two paths using natural sorting.
#[cfg(windows)]
pub fn compare_naturally(lhs: &Path, rhs: &Path) -> Ordering {
    let lhs = lhs.as_os_str().encode_wide();
    let rhs = rhs.as_os_str().encode_wide();
    do_compare!(lhs, rhs)
}

pub trait ValueExt {
    fn try_into_string(self) -> Option<String>;
}

impl ValueExt for Value {
    fn try_into_string(self) -> Option<String> {
        match self {
            Value::String(s) => Some(s),
            _ => None,
        }
    }
}
