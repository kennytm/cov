use clap::ArgMatches;

use std::collections::{HashMap, HashSet};
use std::ffi::OsStr;

lazy_static! {
    static ref SPECIALS: HashSet<&'static str> = [
        "manifest-path",
        "target",
        "profiler",
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

pub fn normalize<'a, I>(args: I, specialized: &mut SpecialMap<'a>) -> Vec<&'a OsStr>
where
    I: IntoIterator<Item = &'a OsStr>,
{
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
