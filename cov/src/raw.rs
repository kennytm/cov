//! The raw structures of a GCNO/GCDA file.

use error::*;
use intern::{Interner, Symbol};
#[cfg(feature = "serde")]
use intern::SerializeWithInterner;
use reader::Reader;

use byteorder::{BigEndian, ByteOrder};
#[cfg(feature = "serde")]
use serde::{Deserialize, Deserializer, Serialize, Serializer};

use std::{fmt, u64};
use std::collections::BTreeMap;
use std::fs::File;
use std::io::BufReader;
use std::path::{Path, PathBuf};
#[cfg(feature = "serde")]
use std::result::Result as StdResult;
use std::str::FromStr;

//----------------------------------------------------------------------------------------------------------------------
//{{{ Gcov & RecordIndex

derive_serialize_with_interner! {
    /// The GCNO/GCDA file content.
    #[derive(Clone, PartialEq, Eq, Hash, Debug)]
    #[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
    pub struct Gcov {
        /// File type.
        pub ty: Type,
        /// File version.
        pub version: Version,
        /// The stamp value uniquely identifies a GCNO between consecutive compilations. The corresponding GCDA will
        /// have the same stamp.
        pub stamp: u32,
        /// Vector of records.
        pub records: Vec<Record>,
        /// Source of the gcov file
        #[serde(skip)]
        pub src: Option<PathBuf>,
    }
}

impl Gcov {
    /// Parses the file with at the given path as GCNO/GCDA format.
    ///
    /// # Errors
    ///
    /// * Returns [`UnknownFileType`] if the file is not a in GCNO/GCDA format.
    /// * Returns [`UnsupportedVersion`] if the GCNO/GCDA version is not supported by this crate.
    /// * Returns [`UnknownTag`] if the GCNO/GCDA contains an unrecognized record tag.
    /// * Returns [`Io`] on I/O failure.
    ///
    /// [`UnknownFileType`]: ../error/enum.ErrorKind.html#variant.UnknownFileType
    /// [`UnsupportedVersion`]: ../error/enum.ErrorKind.html#variant.UnsupportedVersion
    /// [`UnknownTag`]: ../error/enum.ErrorKind.html#variant.UnknownTag
    /// [`Io`]: ../error/enum.ErrorKind.html#variant.Io
    pub fn open<P: AsRef<Path>>(p: P, interner: &mut Interner) -> Result<Gcov> {
        debug!("open gcov file {:?}", p.as_ref());
        let src = p.as_ref().to_owned();
        Location::File(src.clone()).wrap(|| -> Result<Gcov> {
            let reader = BufReader::new(File::open(p)?);
            let mut gcov = Reader::new(reader, interner)?.parse()?;
            gcov.src = Some(src);
            Ok(gcov)
        })
    }
}

//}}}
//----------------------------------------------------------------------------------------------------------------------
//{{{ Type

/// File type.
#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub enum Type {
    /// The GCNO (**gc**ov **no**tes) file, with file extension `*.gcno`.
    Gcno,
    /// The GCDA (**gc**ov **da**ta) file, with file extension `*.gcda`.
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

/// Tag of a record.
#[derive(Copy, Clone, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct Tag(pub u32);

/// The tag for the end of file.
pub const EOF_TAG: Tag = Tag(0);
/// The tag for an [`ANNOUNCE_FUNCTION` record](./struct.Function.html).
pub const FUNCTION_TAG: Tag = Tag(0x01_00_00_00);
/// The tag for a [`BASIC_BLOCK` record](./struct.Blocks.html).
pub const BLOCKS_TAG: Tag = Tag(0x01_41_00_00);
/// The tag for an [`ARCS` record](./struct.Arcs.html).
pub const ARCS_TAG: Tag = Tag(0x01_43_00_00);
/// The tag for a [`LINES` record](./struct.Lines.html).
pub const LINES_TAG: Tag = Tag(0x01_45_00_00);
/// The tag for a [`COUNTS` record](./struct.ArcCounts.html).
pub const COUNTER_BASE_TAG: Tag = Tag(0x01_a1_00_00);
/// The tag for a [`SUMMARY` record](./struct.Summary.html).
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

/// GCNO/GCDA version targeting gcc 4.7.
///
/// Starting from this version the gcov format is modified in an incompatible way.
pub const VERSION_4_7: Version = Version(0x34_30_37_2a);

impl Version {
    /// Converts a raw version number to a `Version` structure.
    ///
    /// # Errors
    ///
    /// Returns [`UnsupportedVersion`] if the version is not supported by this crate.
    ///
    /// [`UnsupportedVersion`]: ../error/enum.ErrorKind.html#variant.UnsupportedVersion
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
    /// An `ANNOUNCE_FUNCTION` record in GCNO and GCDA formats.
    Function(Ident, Function),
    /// A `BASIC_BLOCK` record in GCNO format.
    Blocks(Blocks),
    /// An `ARCS` record in GCNO format.
    Arcs(Arcs),
    /// A `LINES` record in GCNO format.
    Lines(Lines),
    /// A `COUNTS` record in GCDA format.
    ArcCounts(ArcCounts),
    /// A `SUMMARY` record in GCDA format.
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
                state.serialize_field(&function.with_interner(interner))?;
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
    /// Information of a function.
    #[derive(Copy, Clone, PartialEq, Eq, Hash, Debug, Default)]
    #[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
    pub struct Function {
        /// The line-number checksum of this function.
        pub lineno_checksum: u32,
        /// The configuration checksum of this function. On versions before 4.7, this value is always 0.
        pub cfg_checksum: u32,
        /// The source location of this function. This field is `None` in a GCDA.
        #[cfg_attr(feature = "serde", serde(default, skip_serializing_if = "Option::is_none"))]
        pub source: Option<Source>,
    }
}

/// Function identifier. The identifier is used to match between two `ANNOUNCE_FUNCTION` records between the GCNO and
/// GCDA. The identifier is not necessarily sequential.
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
    /// Source location of a function.
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
            /// Converts an integer read from GCNO to this attribute.
            ///
            /// # Errors
            ///
            /// Returns [`UnsupportedAttr`] if the GCNO flag is unrecognized.
            ///
            /// [`UnsupportedAttr`]: ../error/enum.ErrorKind.html#variant.UnsupportedAttr
            pub fn from_gcno(flags: u32) -> Result<$flags> {
                ensure!(flags & !($allowed_from_gcno.bits() as u32) == 0, ErrorKind::UnsupportedAttr($kind, flags));
                Ok(<$flags>::from_bits_truncate(flags as u16))
            }
        }
    }
}

/// List of [basic blocks](https://en.wikipedia.org/wiki/Basic_block).
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct Blocks {
    /// The attributes for each block, in sequence.
    pub flags: Vec<BlockAttr>,
}

bitflags! {
    /// Attributes about a [basic block](https://en.wikipedia.org/wiki/Basic_block).
    #[derive(Default)]
    pub struct BlockAttr: u16 {
        /// The block is unexpected.
        ///
        /// Equivalent to the `GCOV_BLOCK_UNEXPECTED` flag.
        const BLOCK_ATTR_UNEXPECTED = 2;

        /// The block ends with a function call which may throw an exception.
        const BLOCK_ATTR_CALL_SITE = 0x1000;

        /// The block starts with the return from a function call.
        const BLOCK_ATTR_CALL_RETURN = 0x2000;

        /// The block is the landing pad for `longjmp`.
        const BLOCK_ATTR_NONLOCAL_RETURN = 0x4000;

        /// The block starts as a catch block.
        const BLOCK_ATTR_EXCEPTIONAL = 0x8000;
    }
}

derive_serde_for_attr! {
    BlockAttr, "block", BLOCK_ATTR_UNEXPECTED
}

/// Index to a block the [`Blocks`].
///
/// [`Blocks`]: ./struct.Blocks.html
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

/// List of arcs (out-going edges) from a single basic block.
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct Arcs {
    /// The predecessor basic block of this collection of arcs.
    pub src_block: BlockIndex,
    /// The arcs going out from the `src_block`.
    pub arcs: Vec<Arc>,
}

bitflags! {
    /// Attributes about an [`Arc`].
    ///
    /// [`Arc`]: ./struct.Arc.html
    #[derive(Default)]
    pub struct ArcAttr: u16 {
        /// The arc is a non-instrumentable edge on the spanning tree. This arc will not appear in the corresponding
        /// GCDA file.
        ///
        /// Equivalent to the `GCOV_ARC_ON_TREE` flag.
        const ARC_ATTR_ON_TREE = 1;

        /// The arc is fake. Such arcs connect no-return blocks (e.g. infinite loop and `-> !` functions) to the exit
        /// block, i.e. in reality this arc should never be taken.
        ///
        /// Equivalent to the `GCOV_ARC_FAKE` flag.
        const ARC_ATTR_FAKE = 2;

        /// The arc is fall-through.
        ///
        /// Equivalent to the `GCOV_ARC_FALLTHROUGH` flag.
        const ARC_ATTR_FALLTHROUGH = 4;

        /// The arc is taken to a `catch` handler.
        const ARC_ATTR_THROW = 0x10;

        /// The arc is for a function that abnormally returns.
        const ARC_ATTR_CALL_NON_RETURN = 0x20;

        /// The arc is for `setjmp`.
        const ARC_ATTR_NONLOCAL_RETURN = 0x40;

        /// The arc is an unconditional branch.
        const ARC_ATTR_UNCONDITIONAL = 0x80;
    }
}

derive_serde_for_attr! {
    ArcAttr, "arc", ARC_ATTR_ON_TREE | ARC_ATTR_FAKE | ARC_ATTR_FALLTHROUGH
}

/// An arc.
#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct Arc {
    /// The destination basic block of the arc. The source is in the [`Arcs`] structure.
    ///
    /// [`Arcs`]: ./struct.Arcs.html
    pub dest_block: BlockIndex,

    /// The attribute of this arc.
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
        /// The basic block which contains these source information.
        pub block_number: BlockIndex,
        /// The line numebers. The vector typically starts with a [`FileName`], followed by many [`LineNumber`]s.
        ///
        /// [`FileName`]: ./enum.Line.html#variant.FileName
        /// [`LineNumber`]: ./enum.Line.html#variant.LineNumber
        pub lines: Vec<Line>,
    }
}

/// A source line entry of a basic block.
#[derive(Copy, Clone, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(untagged))]
pub enum Line {
    /// A line number inside the basic block.
    LineNumber(u32),
    /// The file name containing the basic block.
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

/// Counter of how many times an arc is taken. Only arcs without the [`ARC_ATTR_ON_TREE`] flag will be recorded.
///
/// [`ARC_ATTR_ON_TREE`]: ./constant.ARC_ATTR_ON_TREE.html
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct ArcCounts {
    /// How many times an arc is taken. The vector lists the counts for each arc.
    pub counts: Vec<u64>,
}

//}}}
//----------------------------------------------------------------------------------------------------------------------
//{{{ Summary & Histogram

/// Object summary.
#[derive(Clone, PartialEq, Eq, Hash, Debug, Default)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct Summary {
    /// Checksum of the object.
    pub checksum: u32,
    /// Number of counters.
    pub num: u32,
    /// Number of program runs.
    pub runs: u32,
    /// Sum of all counters accumulated.
    pub sum: u64,
    /// Maximum count of a single run.
    pub max: u64,
    /// Sum of individual maximum counts.
    pub sum_max: u64,
    /// Histogram of counter values.
    pub histogram: Option<Histogram>,
}

/// Histogram in the [`Summary`].
///
/// [`Summary`]: ./struct.Summary.html
#[derive(Clone, PartialEq, Eq, Hash, Debug, Default)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct Histogram {
    /// Buckets in the histogram.
    ///
    /// The key gives the scale-index.
    pub buckets: BTreeMap<u32, HistogramBucket>,
}

/// A bucket in the [`Histogram`].
///
/// [`Histogram`]: ./struct.Histogram.html
#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct HistogramBucket {
    /// Number of counters whose profile count falls within the bucket.
    pub num: u32,
    /// Smallest profile count included in this bucket.
    pub min: u64,
    /// Cumulative value of the profile counts in this bucket.
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

//}}}

derive_serialize_with_interner! {
    direct: Type, Tag, Version, Ident, BlockAttr, ArcAttr, Blocks, BlockIndex, Arcs, ArcCounts, Summary
}
