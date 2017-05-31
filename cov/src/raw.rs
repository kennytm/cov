//! The raw structures of a gcov file.

use error::*;
use intern::{Interner, Symbol};
#[cfg(feature = "serde")]
use intern::SerializeWithInterner;
use reader::Reader;
use utils::EntryExt;

use byteorder::{BigEndian, ByteOrder};
#[cfg(feature = "serde")]
use serde::{Deserialize, Deserializer, Serialize, Serializer};

use std::{fmt, u64};
use std::cmp::{max, min};
use std::collections::BTreeMap;
use std::fs::File;
use std::io::BufReader;
use std::path::Path;
#[cfg(feature = "serde")]
use std::result::Result as StdResult;
use std::str::FromStr;

//----------------------------------------------------------------------------------------------------------------------
//{{{ Gcov & RecordIndex

derive_serialize_with_interner! {
    /// The raw file.
    #[derive(Clone, PartialEq, Eq, Hash, Debug)]
    #[cfg_attr(feature="serde", derive(Serialize, Deserialize))]
    pub struct Gcov {
        pub ty: Type,
        pub version: Version,
        pub checksum: u32,
        pub records: Vec<Record>,
    }
}

impl Gcov {
    /// Parses the header of a file with file name, and creates a new gcov reader.
    pub fn open<P: AsRef<Path>>(p: P, interner: &mut Interner) -> Result<Gcov> {
        debug!("open gcov file {:?}", p.as_ref());
        Reader::new(BufReader::new(File::open(p)?), interner)?.parse()
    }
}

//}}}
//----------------------------------------------------------------------------------------------------------------------
//{{{ Type

/// The file type.
#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub enum Type {
    /// The "notes" file, with file extension `*.gcno`.
    Gcno,
    /// The "data" file, with file extension `*.gcda`.
    Gcda,
}

impl fmt::Display for Type {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        fmt.write_str(match *self {
            Type::Gcno => "gcno",
            Type::Gcda => "gcda",
        })
    }
}

//}}}
//----------------------------------------------------------------------------------------------------------------------
//{{{ Tag

/// The tag of a record.
#[derive(Copy, Clone, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct Tag(pub u32);

/// The tag for the end of file.
pub const EOF_TAG: Tag = Tag(0);
/// The tag for an `ANNOUNCE_FUNCTION` record.
pub const FUNCTION_TAG: Tag = Tag(0x01_00_00_00);
/// The tag for a `BASIC_BLOCK` record.
pub const BLOCKS_TAG: Tag = Tag(0x01_41_00_00);
/// The tag for an `ARCS` record.
pub const ARCS_TAG: Tag = Tag(0x01_43_00_00);
/// The tag for a `LINES` record.
pub const LINES_TAG: Tag = Tag(0x01_45_00_00);
/// The tag for an `ARC_COUNTS` record.
pub const COUNTER_BASE_TAG: Tag = Tag(0x01_a1_00_00);
/// The tag for a `SUMMARY` record.
pub const OBJECT_SUMMARY_TAG: Tag = Tag(0xa1_00_00_00);
/// The tag for a program-`SUMMARY` record, which has been deprecated and is always skipped when present.
pub const PROGRAM_SUMMARY_TAG: Tag = Tag(0xa3_00_00_00);
/// Tag of record used by AutoFDO.
pub const AFDO_FILE_NAMES_TAG: Tag = Tag(0xaa_00_00_00);
/// Tag of record used by AutoFDO.
pub const AFDO_FUNCTION_TAG: Tag = Tag(0xac_00_00_00);
/// Tag of record used by AutoFDO.
pub const AFDO_WORKING_SET_TAG: Tag = Tag(0xaf_00_00_00);

impl fmt::Display for Tag {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        write!(fmt, "0x{:08x}", self.0)
    }
}

impl fmt::Debug for Tag {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        write!(fmt, "Tag(0x{:08x})", self.0)
    }
}

impl fmt::LowerHex for Tag {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        self.0.fmt(fmt)
    }
}

impl fmt::UpperHex for Tag {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        self.0.fmt(fmt)
    }
}

//}}}
//----------------------------------------------------------------------------------------------------------------------
//{{{ Version

/// File version.
#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct Version(u32);

/// An invalid file version.
pub const INVALID_VERSION: Version = Version(0);

/// The file is targeting gcc 4.7. In this version the gcov format is modified in an incompatible way.
pub const VERSION_4_7: Version = Version(0x34_30_37_2a);

impl Version {
    /// Converts a raw version number to a `Version` structure.
    ///
    /// Returns `Err(UnsupportedVersion)` if the version is not supported by this crate.
    pub fn try_from(raw_version: u32) -> Result<Version> {
        ensure!(raw_version & 0x80_80_80_ff == 0x2a, ErrorKind::UnsupportedVersion(raw_version));
        Ok(Version(raw_version))
    }
}


impl fmt::Debug for Version {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        write!(fmt, "Tag(\"{}\")", self)
    }
}

impl fmt::Display for Version {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        write!(fmt, "{}{}{}{}",
            (self.0 >> 24 & 0xff) as u8 as char,
            (self.0 >> 16 & 0xff) as u8 as char,
            (self.0 >> 8 & 0xff) as u8 as char,
            (self.0 & 0xff) as u8 as char,
        )
    }
}

impl FromStr for Version {
    type Err = Error;
    fn from_str(s: &str) -> Result<Self> {
        ensure!(s.len() == 4, ErrorKind::UnsupportedVersion(0));
        let raw_version = BigEndian::read_u32(s.as_bytes());
        Version::try_from(raw_version)
    }
}

#[cfg(feature = "serde")]
impl Serialize for Version {
    fn serialize<S: Serializer>(&self, serializer: S) -> StdResult<S::Ok, S::Error> {
        serializer.collect_str(self)
    }
}

#[cfg(feature = "serde")]
impl<'de> Deserialize<'de> for Version {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> StdResult<Self, D::Error> {
        use serde::de::Error;
        let s = <&'de str>::deserialize(deserializer)?;
        Version::from_str(s).map_err(D::Error::custom)
    }
}

//}}}
//----------------------------------------------------------------------------------------------------------------------
//{{{ Record

/// A record in a gcov file.
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub enum Record {
    Function(Ident, Function),
    Blocks(Blocks),
    Arcs(Arcs),
    Lines(Lines),
    ArcCounts(ArcCounts),
    Summary(Summary),
}

#[cfg(feature = "serde")]
impl SerializeWithInterner for Record {
    fn serialize_with_interner<S: Serializer>(&self, serializer: S, interner: &Interner) -> StdResult<S::Ok, S::Error> {
        match *self {
            Record::Function(ref ident, ref function) => {
                use serde::ser::SerializeTupleVariant;
                let mut state = serializer.serialize_tuple_variant("Record", 0, "Function", 2)?;
                state.serialize_field(ident)?;
                state.serialize_field(&interner.with(function))?;
                state.end()
            },
            Record::Lines(ref lines) => serializer.serialize_newtype_variant("Record", 3, "Lines", lines),
            _ => self.serialize(serializer),
        }
    }
}

//}}}
//----------------------------------------------------------------------------------------------------------------------
//{{{ Function, Ident & Source

derive_serialize_with_interner! {
    /// A function.
    #[derive(Clone, PartialEq, Eq, Hash, Debug, Default)]
    #[cfg_attr(feature="serde", derive(Serialize, Deserialize))]
    pub struct Function {
        pub lineno_checksum: u32,
        #[cfg_attr(feature="serde", serde(default, skip_serializing_if="Option::is_none"))]
        pub cfg_checksum: Option<u32>,
        #[cfg_attr(feature="serde", serde(default, skip_serializing_if="Option::is_none"))]
        pub source: Option<Source>,
    }
}

/// Function identifier.
#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct Ident(pub u32);

impl fmt::Debug for Ident {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        write!(fmt, "Ident({})", self.0)
    }
}

impl fmt::Display for Ident {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        self.0.fmt(fmt)
    }
}

derive_serialize_with_interner! {
    /// Source information of a file.
    #[derive(Copy, Clone, PartialEq, Eq, Hash, Debug, Default)]
    #[cfg_attr(feature="serde", derive(Serialize, Deserialize))]
    pub struct Source {
        /// Function name
        pub name: Symbol,
        /// File name
        pub filename: Symbol,
        /// Line number
        pub line: u32,
    }
}

//}}}
//----------------------------------------------------------------------------------------------------------------------
//{{{ Blocks

macro_rules! derive_serde_for_attr {
    ($flags:path, $kind:expr, $allowed_from_gcno:expr) => {
        #[cfg(feature="serde")]
        impl Serialize for $flags {
            fn serialize<S: Serializer>(&self, serializer: S) -> StdResult<S::Ok, S::Error> {
                serializer.serialize_u16(self.bits())
            }
        }

        #[cfg(feature="serde")]
        impl<'de> Deserialize<'de> for $flags {
            fn deserialize<D: Deserializer<'de>>(deserializer: D) -> StdResult<Self, D::Error> {
                use ::serde::de::Error;
                let b = u16::deserialize(deserializer)?;
                <$flags>::from_bits(b).ok_or_else(|| D::Error::custom(ErrorKind::UnsupportedAttr($kind, b as u32)))
            }
        }

        impl $flags {
            pub fn from_gcno(flags: u32) -> Result<$flags> {
                ensure!(flags & !($allowed_from_gcno.bits() as u32) == 0, ErrorKind::UnsupportedAttr($kind, flags));
                Ok(<$flags>::from_bits_truncate(flags as u16))
            }
        }
    }
}

/// List of basic blocks.
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct Blocks {
    pub flags: Vec<BlockAttr>,
}

bitflags! {
    /// Attributes about a block.
    #[derive(Default)]
    pub struct BlockAttr: u16 {
        // This one must be consistent with GCNO.
        const BLOCK_ATTR_UNEXPECTED = 2;

        const BLOCK_ATTR_CALL_SITE = 0x1000;
        const BLOCK_ATTR_CALL_RETURN = 0x2000;
        const BLOCK_ATTR_NONLOCAL_RETURN = 0x4000;
        const BLOCK_ATTR_EXCEPTIONAL = 0x8000;
    }
}

derive_serde_for_attr! {
    BlockAttr, "block", BLOCK_ATTR_UNEXPECTED
}

/// Index to a block in the basic blocks list.
#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct BlockIndex(pub u32);

impl fmt::Debug for BlockIndex {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        write!(fmt, "BI({})", self.0)
    }
}

impl From<BlockIndex> for usize {
    fn from(i: BlockIndex) -> usize {
        i.0 as usize
    }
}

//}}}
//----------------------------------------------------------------------------------------------------------------------
//{{{ Arcs

/// List of arcs (out-going edges) from a single source.
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct Arcs {
    pub src_block: BlockIndex,
    pub arcs: Vec<Arc>,
}

bitflags! {
    /// Attributes about an arc.
    #[derive(Default)]
    pub struct ArcAttr: u16 {
        // These three must be consistent with GCNO.
        const ARC_ATTR_ON_TREE = 1;
        const ARC_ATTR_FAKE = 2;
        const ARC_ATTR_FALLTHROUGH = 4;

        const ARC_ATTR_THROW = 0x10;
        const ARC_ATTR_CALL_NON_RETURN = 0x20;
        const ARC_ATTR_NONLOCAL_RETURN = 0x40;
        const ARC_ATTR_UNCONDITIONAL = 0x80;
    }
}

derive_serde_for_attr! {
    ArcAttr, "arc", ARC_ATTR_ON_TREE | ARC_ATTR_FAKE | ARC_ATTR_FALLTHROUGH
}

/// An arc destination.
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct Arc {
    pub dest_block: BlockIndex,
    pub flags: ArcAttr,
}

//}}}
//----------------------------------------------------------------------------------------------------------------------
//{{{ Lines

derive_serialize_with_interner! {
    /// Information about source lines covered by a basic block.
    #[derive(Clone, PartialEq, Eq, Hash, Debug)]
    #[cfg_attr(feature="serde", derive(Serialize, Deserialize))]
    pub struct Lines {
        pub block_number: BlockIndex,
        pub lines: Vec<Line>,
    }
}

#[derive(Clone, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(untagged))]
pub enum Line {
    LineNumber(u32),
    FileName(Symbol),
}

impl fmt::Debug for Line {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        match *self {
            Line::LineNumber(ref n) => write!(fmt, "Line({:?})", n),
            Line::FileName(ref n) => write!(fmt, "Line({:?})", n),
        }
    }
}

#[cfg(feature = "serde")]
impl SerializeWithInterner for Line {
    fn serialize_with_interner<S: Serializer>(&self, serializer: S, interner: &Interner) -> StdResult<S::Ok, S::Error> {
        match *self {
            Line::LineNumber(number) => number.serialize(serializer),
            Line::FileName(symbol) => symbol.serialize_with_interner(serializer, interner),
        }
    }
}

//}}}
//----------------------------------------------------------------------------------------------------------------------
//{{{ ArcCounts

/// Counter of how many times an arc is hit.
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct ArcCounts {
    pub counts: Vec<u64>,
}

//}}}
//----------------------------------------------------------------------------------------------------------------------
//{{{ Summary & Histogram

/// Object summary.
#[derive(Clone, PartialEq, Eq, Hash, Debug, Default)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct Summary {
    pub checksum: u32,
    pub num: u32,
    pub runs: u32,
    pub sum: u64,
    pub max: u64,
    pub sum_max: u64,
    pub histogram: Option<Histogram>,
}

/// Histogram in the program summary.
#[derive(Clone, PartialEq, Eq, Hash, Debug, Default)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct Histogram {
    pub buckets: BTreeMap<u32, HistogramBucket>,
}

#[derive(Clone, PartialEq, Eq, Hash, Debug)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct HistogramBucket {
    pub num: u32,
    pub min: u64,
    pub sum: u64,
}

impl Default for HistogramBucket {
    fn default() -> HistogramBucket {
        HistogramBucket {
            num: 0,
            min: u64::MAX,
            sum: 0,
        }
    }
}

impl Summary {
    /// Merges another summary to here.
    pub fn merge(&mut self, other: &Summary) -> Result<()> {
        if self.checksum == 0 {
            self.checksum = other.checksum;
        } else if self.checksum != other.checksum {
            bail!(ErrorKind::ChecksumMismatch("summary"));
        }

        if self.runs == 0 {
            self.num = other.num;
        }

        self.runs += other.runs;
        self.sum += other.sum;
        self.max = max(self.max, other.max);
        self.sum_max += other.sum_max;

        if let Some(ref other_hist) = other.histogram {
            match self.histogram {
                None => {
                    self.histogram = Some(other_hist.clone());
                },
                Some(ref mut hist) => {
                    for (key, value) in &other_hist.buckets {
                        let existing = hist.buckets.entry(*key).or_default();
                        existing.num = value.num;
                        existing.min = min(existing.min, value.min);
                        existing.sum += value.sum;
                    }
                },
            }
        }

        Ok(())
    }
}

//}}}

derive_serialize_with_interner! {
    direct: Type, Tag, Version, Ident, BlockAttr, ArcAttr, Blocks, BlockIndex, Arcs, ArcCounts, Summary
}
