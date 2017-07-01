//! Additional methods for libstd and external crates.

use error::{ErrorKind, Result as CargoCovResult};

use natord::compare_iter;
use serde_json::Value;

use std::cmp::Ordering;
use std::fs::{File, Permissions, remove_dir_all};
use std::io;
#[cfg(target_os = "redox")]
use std::os::redox::fs::PermissionsExt;
#[cfg(any(target_os = "redox", unix))]
use std::os::unix::ffi::OsStrExt;
#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;
#[cfg(windows)]
use std::os::windows::ffi::{OsStrExt, OsStringExt};
use std::path::Path;
use std::process::Command;

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

#[cfg(any(target_os = "redox", unix))]
pub fn set_executable(file: &File) -> io::Result<()> {
    file.set_permissions(Permissions::from_mode(0o755))
}

#[cfg(windows)]
pub fn set_executable(file: &File) -> io::Result<()> {
    Ok(())
}

pub trait CommandExt {
    fn ensure_success(&mut self, name: &'static str) -> CargoCovResult<()>;
}

impl CommandExt for Command {
    fn ensure_success(&mut self, name: &'static str) -> CargoCovResult<()> {
        let status = self.status()?;
        ensure!(status.success(), ErrorKind::ForwardFailed(name, status));
        Ok(())
    }
}


pub fn parent_3(path: &Path) -> &Path {
    path.parent().expect("..").parent().expect("../..").parent().expect("../../..")
}
