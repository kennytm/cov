use clap::ArgMatches;

use std::collections::{HashMap, HashSet};
use std::ffi::OsStr;
use std::path::Path;

lazy_static! {
    static ref SPECIALS: HashSet<&'static str> = [
        "manifest-path",
        "target",
        "profiler",
    ].iter().cloned().collect();

    static ref RUSTC_FLAGS_WITH_VALUES: HashSet<&'static str> = [
        "--allow",
        "--cap-lints",
        "--cfg",
        "--codegen",
        "--color",
        "--crate-name",
        "--crate-type",
        "--deny",
        "--emit",
        "--error-format",
        "--explain",
        "--extern",
        "--forbid",
        "--out-dir",
        "--pretty",
        "--print",
        "--sysroot",
        "--target",
        "--unpretty",
        "--warn",
        "-A",
        "-C",
        "-D",
        "-F",
        "-l",
        "-L",
        "-o",
        "-W",
        "-Z",
    ].iter().cloned().collect();
}

pub type SpecialMap<'a> = HashMap<&'static str, &'a OsStr>;

pub fn update_from_clap<'a>(matches: &'a ArgMatches, specialized: &mut SpecialMap<'a>) {
    for name in SPECIALS.iter() {
        if let Some(value) = matches.value_of_os(name) {
            specialized.insert(name, value);
        }
    }
}

pub fn is_rustc_compiling_local_crate<'a, I: IntoIterator<Item = &'a OsStr>>(args: I, workspace_path: &Path) -> bool {
    let mut skip_next = false;
    for arg in args {
        if skip_next {
            skip_next = false;
            continue;
        }

        if let Some(s) = arg.to_str() {
            if RUSTC_FLAGS_WITH_VALUES.contains(s) {
                skip_next = true;
                continue;
            } else if s == "--" {
                return false;
            } else if s.starts_with('-') {
                continue;
            }
        }

        let crate_path = Path::new(arg);
        return crate_path.starts_with(workspace_path);
    }
    false
}

pub fn normalize<'a, I: IntoIterator<Item = &'a OsStr>>(args: I, specialized: &mut SpecialMap<'a>) -> Vec<&'a OsStr> {
    let mut normalized = Vec::new();

    let mut current_name = None;
    let mut encountered_double_minus = false;
    for arg in args {
        if !encountered_double_minus {
            if let Some(name) = current_name.take() {
                specialized.insert(name, arg);
                continue;
            }

            if let Some(s) = arg.to_str() {
                if s.starts_with("--") {
                    let s = &s[2..];
                    if s.is_empty() {
                        encountered_double_minus = true;
                    } else if let Some(name) = SPECIALS.get(s) {
                        current_name = Some(name);
                        continue;
                    } else if let Some(eq_index) = s.find('=') {
                        if let Some(name) = SPECIALS.get(&s[..eq_index]) {
                            let value = OsStr::new(&s[(eq_index + 1)..]);
                            specialized.insert(name, value);
                            continue;
                        }
                    }
                }
            }
        }

        normalized.push(arg);
    }

    normalized
}
