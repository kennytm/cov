use std::env;
use std::path::{MAIN_SEPARATOR, PathBuf};
use std::str::FromStr;

const MACOS_RUSTSRC_DIR: &str = "/Users/travis/build/rust-lang/rust/";
const DOCKER_RUSTSRC_DIR: &str = "/checkout/";
const WINDOWS_RUSTSRC_DIR: &str = r"C:\projects\rust\";

lazy_static! {
    static ref REGISTRY_PATH: String = {
        let mut cargo_home = env::var_os("CARGO_HOME").map_or_else(|| {
            let mut home = env::home_dir().unwrap_or_else(|| "/".into());
            home.push(".cargo");
            home
        }, PathBuf::from);
        cargo_home.push("registry");
        cargo_home.push("src");
        let mut registry_path = cargo_home.to_string_lossy().into_owned();
        registry_path.push(MAIN_SEPARATOR);
        registry_path
    };
}

bitflags! {
    pub struct SourceType: u8 {
        const SOURCE_TYPE_LOCAL = 1;
        const SOURCE_TYPE_MACROS = 2;
        const SOURCE_TYPE_UNKNOWN = 4;
        const SOURCE_TYPE_CRATES = 8;
        const SOURCE_TYPE_RUSTSRC = 16;

        const SOURCE_TYPE_DEFAULT = SOURCE_TYPE_LOCAL.bits | SOURCE_TYPE_MACROS.bits | SOURCE_TYPE_UNKNOWN.bits;
    }
}

#[derive(Debug)]
pub struct UnsupportedSourceTypeName;

impl SourceType {
    pub fn from_multi_str<'a, I>(strings: I) -> Result<SourceType, UnsupportedSourceTypeName>
    where
        I: Iterator<Item = &'a str>,
    {
        let mut res = SourceType::empty();
        for s in strings {
            res |= s.parse()?;
        }
        Ok(res)
    }

    pub fn prefix(self) -> &'static str {
        match self {
            SOURCE_TYPE_LOCAL => ".",
            SOURCE_TYPE_RUSTSRC => "«rust»",
            SOURCE_TYPE_CRATES => "«crates»",
            _ => "",
        }
    }
}


impl FromStr for SourceType {
    type Err = UnsupportedSourceTypeName;
    fn from_str(s: &str) -> Result<SourceType, UnsupportedSourceTypeName> {
        Ok(match s {
            "local" => SOURCE_TYPE_LOCAL,
            "macros" => SOURCE_TYPE_MACROS,
            "rustsrc" => SOURCE_TYPE_RUSTSRC,
            "crates" => SOURCE_TYPE_CRATES,
            "unknown" => SOURCE_TYPE_UNKNOWN,
            "all" => SourceType::all(),
            _ => return Err(UnsupportedSourceTypeName),
        })
    }
}

pub fn identify_source_path(path: &str, crates_path: &str) -> (SourceType, usize) {
    if path.starts_with(crates_path) {
        (SOURCE_TYPE_LOCAL, crates_path.len())
    } else if path.starts_with(&*REGISTRY_PATH) {
        let subpath = &path[REGISTRY_PATH.len()..];
        let first_slash = subpath.find(MAIN_SEPARATOR).map_or(0, |s| s + MAIN_SEPARATOR.len_utf8());
        (SOURCE_TYPE_CRATES, REGISTRY_PATH.len() + first_slash)
    } else if path.starts_with('<') && path.ends_with(" macros>") {
        (SOURCE_TYPE_MACROS, 0)
    } else if path.starts_with(MACOS_RUSTSRC_DIR) {
        (SOURCE_TYPE_RUSTSRC, MACOS_RUSTSRC_DIR.len())
    } else if path.starts_with(DOCKER_RUSTSRC_DIR) {
        (SOURCE_TYPE_RUSTSRC, DOCKER_RUSTSRC_DIR.len())
    } else if path.starts_with(WINDOWS_RUSTSRC_DIR) {
        (SOURCE_TYPE_RUSTSRC, WINDOWS_RUSTSRC_DIR.len())
    } else {
        (SOURCE_TYPE_UNKNOWN, 0)
    }
}
