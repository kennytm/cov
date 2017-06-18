//! Reader of [`Gcov`] format.
//!
//! The file format of GCNO/GCDA is documented in the [GCC source code][gcov-io.h].
//!
//! [`Gcov`]: ../raw/struct.Gcov.html
//! [gcov-io.h]: https://gcc.gnu.org/git/?p=gcc.git;a=blob;f=gcc/gcov-io.h;hb=HEAD

use error::*;
use intern::{Interner, Symbol, UNKNOWN_SYMBOL};
use raw::*;

use byteorder::{BigEndian, ByteOrder, LittleEndian, ReadBytesExt};

use std::io::{self, Read, Take};
use std::iter::FromIterator;
use std::result::Result as StdResult;

/// The reader of a GCNO/GCDA file.
///
/// # Examples
///
/// ```rust
/// use cov::reader::Reader;
/// use cov::Interner;
/// # use cov::Result;
/// use std::io::Read;
/// use std::fs::File;
///
/// # fn main() { run().unwrap(); }
/// # fn run() -> Result<()> {
/// let mut interner = Interner::new();
/// let file = File::open("test-data/trivial.clang.gcno")?;
///
/// // read the header.
/// let mut reader = Reader::new(file, &mut interner)?;
/// // read the content.
/// let _gcov = reader.parse()?;
/// # Ok(()) }
/// ```
#[derive(Debug)]
pub struct Reader<'si, R> {
    reader: R,
    cursor: u64,
    ty: Type,
    version: Version,
    stamp: u32,
    is_big_endian: bool,
    interner: &'si mut Interner,
}

/// Consumes the whole reader to the end.
fn consume_to_end<R: Read>(reader: &mut R) -> Result<()> {
    loop {
        let mut buf = [0u8; 64];
        match reader.read(&mut buf) {
            Ok(0) => break,
            Ok(_) => continue,
            Err(e) => {
                if e.kind() == io::ErrorKind::Interrupted {
                    continue;
                } else {
                    bail!(e);
                }
            },
        }
    }
    Ok(())
}

#[test]
fn test_consume_to_end() {
    (|| -> Result<()> {
        let mut reader = &b"abcde123456fghijkl"[..];
        let mut top = [0u8; 5];
        reader.read_exact(&mut top)?;
        consume_to_end(&mut reader.by_ref().take(6))?;
        let mut bottom = [0u8; 7];
        reader.read_exact(&mut bottom)?;
        assert_eq!(&top, b"abcde");
        assert_eq!(&bottom, b"fghijkl");
        assert_eq!(reader, b"");
        Ok(())
    })().unwrap();
}

impl<'si, R: Read> Reader<'si, R> {
    /// Advances the reader cursor by `count` bytes. If `res` is an error, include the file position information to the
    /// error, otherwise return `res` as-is.
    fn advance_cursor<T, E: Into<Error>>(&mut self, count: u64, res: StdResult<T, E>) -> Result<T> {
        Location::Cursor(self.cursor).wrap(|| {
            self.cursor += count;
            res
        })
    }

    /// Reads a 32-bit number in gcov format.
    ///
    /// # Errors
    ///
    /// Returns [`Io`] on I/O failure, e.g. reaching end-of-file.
    ///
    /// [`Io`]: ../error/enum.ErrorKind.html#variant.Io
    fn read_32(&mut self) -> Result<u32> {
        let value = self.reader.read_u32::<LittleEndian>();
        let mut value = self.advance_cursor(4, value)?;
        if self.is_big_endian {
            value = value.swap_bytes();
        }
        Ok(value)
    }

    /// Reads a 64-bit number in gcov format.
    ///
    /// # Errors
    ///
    /// Returns [`Io`] on I/O failure, e.g. reaching end-of-file.
    ///
    /// [`Io`]: ../error/enum.ErrorKind.html#variant.Io
    fn read_64(&mut self) -> Result<u64> {
        let value = self.reader.read_u64::<LittleEndian>();
        let mut value = self.advance_cursor(8, value)?;
        if self.is_big_endian {
            value = value.rotate_left(32).swap_bytes();
        }
        Ok(value)
    }

    /// Reads eight 32-bit numbers in gcov format.
    ///
    /// # Errors
    ///
    /// Returns [`Io`] on I/O failure, e.g. reaching end-of-file.
    ///
    /// [`Io`]: ../error/enum.ErrorKind.html#variant.Io
    fn read_histogram_bitvector(&mut self) -> Result<[u32; 8]> {
        let mut buf = [0; 32];
        let res = self.reader.read_exact(&mut buf);
        self.advance_cursor(32, res)?;

        let mut decoded = [0; 8];
        let decoder = if self.is_big_endian {
            BigEndian::read_u32
        } else {
            LittleEndian::read_u32
        };

        for (i, slot) in decoded.iter_mut().enumerate() {
            *slot = decoder(&buf[(i * 4)..])
        }

        Ok(decoded)
    }

    /// Reads a string in gcov format.
    ///
    /// # Errors
    ///
    /// * Returns [`Io`] on I/O failure, e.g. reaching end-of-file.
    /// * Returns [`FromUtf8`] if the string is not encoded in UTF-8.
    ///
    /// [`Io`]: ../error/enum.ErrorKind.html#variant.Io
    /// [`FromUtf8`]: ../error/enum.ErrorKind.html#variant.FromUtf8
    fn read_string(&mut self) -> Result<Symbol> {
        let length = (self.read_32()? as u64) * 4;
        let mut buf = Vec::with_capacity(length as usize);
        let cursor = self.cursor;
        let value = self.reader.by_ref().take(length).read_to_end(&mut buf);
        let _ = self.advance_cursor(length, value)?;
        let actual_length = buf.iter().rposition(|b| *b != 0).unwrap_or(!0).wrapping_add(1);
        buf.truncate(actual_length);
        let string = Location::Cursor(cursor).wrap(|| String::from_utf8(buf))?;
        Ok(self.interner.intern(string.into_boxed_str()))
    }

    /// Reads something from this reader using the provided function `f`, until end-of-file is encountered.
    ///
    /// The result is a collection of returned values of `f`.
    fn until_eof<C, T, F>(&mut self, f: F) -> Result<C>
    where
        F: FnMut(&mut Self) -> Result<T>,
        C: FromIterator<T>,
    {
        UntilEof(self, f).collect()
    }

    /// Parses the header of the file, and creates a new gcov reader.
    ///
    /// # Errors
    ///
    /// * Returns [`UnknownFileType`] if the reader is not a in GCNO/GCDA format.
    /// * Returns [`UnsupportedVersion`] if the GCNO/GCDA version is not supported by this crate.
    /// * Returns [`Io`] on I/O failure.
    ///
    /// [`UnknownFileType`]: ../error/enum.ErrorKind.html#variant.UnknownFileType
    /// [`UnsupportedVersion`]: ../error/enum.ErrorKind.html#variant.UnsupportedVersion
    /// [`Io`]: ../error/enum.ErrorKind.html#variant.Io
    pub fn new(mut reader: R, interner: &'si mut Interner) -> Result<Reader<'si, R>> {
        trace!("gcov-magic");
        let (ty, is_big_endian) = match reader.read_u32::<LittleEndian>()? {
            0x67_63_6e_6f => (Type::Gcno, false),
            0x6f_6e_63_67 => (Type::Gcno, true),
            0x67_63_64_61 => (Type::Gcda, false),
            0x61_64_63_67 => (Type::Gcda, true),
            magic => bail!(ErrorKind::UnknownFileType(magic)),
        };
        let mut result = Reader {
            reader,
            ty,
            is_big_endian,
            interner,
            cursor: 4,
            version: INVALID_VERSION,
            stamp: 0,
        };
        trace!("gcov-version @ 0x{:x}", result.cursor);
        let version = result.read_32()?;
        let version = Location::Cursor(result.cursor - 4).wrap(|| Version::try_from(version))?;
        result.version = version;
        trace!("gcov-stamp @ 0x{:x}", result.cursor);
        result.stamp = result.read_32()?;
        Ok(result)
    }

    /// Parses the content of the reader, to produce a [`Gcov`] structure.
    ///
    /// # Errors
    ///
    /// * Returns [`UnknownTag`] if the GCNO/GCDA contains an unrecognized record tag.
    /// * Returns [`FromUtf8`] if any string in the file is not UTF-8 encoded.
    /// * Returns [`Io`] on I/O failure.
    ///
    /// [`Gcov`]: ../raw/struct.Gcov.html
    /// [`UnknownTag`]: ../error/enum.ErrorKind.html#variant.UnknownTag
    /// [`FromUtf8`]: ../error/enum.ErrorKind.html#variant.FromUtf8
    /// [`Io`]: ../error/enum.ErrorKind.html#variant.Io
    pub fn parse(&mut self) -> Result<Gcov> {
        let records = self.until_eof(|s| {
            let cursor = s.cursor;
            let (tag, mut subreader) = s.read_record_header()?;
            trace!("parse-record @ 0x{:x}; tag = 0x{:08x}", cursor, tag);
            Ok(match tag {
                FUNCTION_TAG => {
                    let (ident, function) = subreader.parse_function()?;
                    Record::Function(ident, function)
                },
                BLOCKS_TAG => Record::Blocks(subreader.parse_blocks()?),
                ARCS_TAG => Record::Arcs(subreader.parse_arcs()?),
                LINES_TAG => Record::Lines(subreader.parse_lines()?),
                COUNTER_BASE_TAG => Record::ArcCounts(subreader.parse_arc_counts()?),
                OBJECT_SUMMARY_TAG |
                PROGRAM_SUMMARY_TAG => Record::Summary(subreader.parse_summary()?),
                EOF_TAG => bail!(ErrorKind::Eof),
                tag => bail!(Location::Cursor(cursor).wrap_error(ErrorKind::UnknownTag(tag.0))),
            })
        })?;
        Ok(Gcov {
            ty: self.ty,
            version: self.version,
            stamp: self.stamp,
            records,
            src: None,
        })
    }

    /// Reads the header of a record. Returns the record type, and a reader that is specialized for
    /// reading this record.
    ///
    /// # Errors
    ///
    /// * Returns [`Io`] on I/O failure.
    ///
    /// [`Io`]: ../error/enum.ErrorKind.html#variant.Io
    fn read_record_header(&mut self) -> Result<(Tag, Reader<Take<&mut R>>)> {
        trace!("record-tag @ 0x{:x}", self.cursor);
        let tag = Tag(self.read_32()?);
        trace!("record-length @ 0x{:x}", self.cursor);
        let length = (self.read_32()? as u64) * 4;
        let subreader = Reader {
            reader: self.reader.by_ref().take(length),
            cursor: self.cursor,
            ty: self.ty,
            version: self.version,
            stamp: self.stamp,
            is_big_endian: self.is_big_endian,
            interner: self.interner,
        };
        debug!("record-header: tag = {0:08x}, length = {1} (0x{1:x}), range = 0x{2:x} .. 0x{3:x}", tag, length, self.cursor, self.cursor + length);
        self.cursor += length;
        Ok((tag, subreader))
    }

    /// Parses the `ANNOUNCE_FUNCTION` record.
    ///
    /// # Errors
    ///
    /// * Returns [`FromUtf8`] if the file name or function name is not UTF-8 encoded.
    /// * Returns [`Io`] on I/O failure.
    ///
    /// [`FromUtf8`]: ../error/enum.ErrorKind.html#variant.FromUtf8
    /// [`Io`]: ../error/enum.ErrorKind.html#variant.Io
    fn parse_function(&mut self) -> Result<(Ident, Function)> {
        trace!("function-ident @ 0x{:x}", self.cursor);
        let ident = Ident(self.read_32()?);
        trace!("function-lineno-checksum @ 0x{:x}", self.cursor);
        let lineno_checksum = self.read_32()?;
        let cfg_checksum = if self.version >= VERSION_4_7 {
            trace!("function-cfg-checksum @ 0x{:x}", self.cursor);
            self.read_32()?
        } else {
            0
        };
        let source = if self.ty == Type::Gcno {
            trace!("function-source @ 0x{:x}", self.cursor);
            Some(self.read_source()?)
        } else if self.version < VERSION_4_7 {
            trace!("function-source-name @ 0x{:x}", self.cursor);
            let name = self.read_string()?;
            Some(Source {
                name,
                filename: UNKNOWN_SYMBOL,
                line: 0,
            })
        } else {
            None
        };

        consume_to_end(&mut self.reader)?;

        Ok((
            ident,
            Function {
                lineno_checksum,
                cfg_checksum,
                source,
            },
        ))
    }

    /// Reads the source of a function.
    ///
    /// # Errors
    ///
    /// * Returns [`FromUtf8`] if the file name or function name is not UTF-8 encoded.
    /// * Returns [`Io`] on I/O failure.
    ///
    /// [`FromUtf8`]: ../error/enum.ErrorKind.html#variant.FromUtf8
    /// [`Io`]: ../error/enum.ErrorKind.html#variant.Io
    fn read_source(&mut self) -> Result<Source> {
        trace!("source-name @ 0x{:x}", self.cursor);
        let name = self.read_string()?;
        trace!("source-filename @ 0x{:x}", self.cursor);
        let filename = self.read_string()?;
        trace!("source-line @ 0x{:x}", self.cursor);
        let line = self.read_32()?;
        Ok(Source {
            name,
            filename,
            line,
        })
    }

    /// Parses the `BASIC_BLOCK` record.
    ///
    /// # Errors
    ///
    /// * Returns [`Io`] on I/O failure.
    ///
    /// [`Io`]: ../error/enum.ErrorKind.html#variant.Io
    fn parse_blocks(&mut self) -> Result<Blocks> {
        trace!("blocks-flags @ 0x{:x}", self.cursor);
        let flags = self.until_eof(|s| {
            let raw_flag = s.read_32()?;
            Location::Cursor(s.cursor - 4).wrap(|| BlockAttr::from_gcno(raw_flag))
        })?;
        Ok(Blocks { flags })
    }

    /// Parses the `ARCS` record.
    ///
    /// # Errors
    ///
    /// * Returns [`Io`] on I/O failure.
    ///
    /// [`Io`]: ../error/enum.ErrorKind.html#variant.Io
    fn parse_arcs(&mut self) -> Result<Arcs> {
        trace!("arcs-block-no @ 0x{:x}", self.cursor);
        let src_block = BlockIndex(self.read_32()?);
        trace!("arcs-arcs @ 0x{:x}", self.cursor);
        let arcs = self.until_eof(|s| {
            trace!("arc-dest-block @ 0x{:x}", s.cursor);
            let dest_block = BlockIndex(s.read_32()?);
            trace!("arc-flags @ 0x{:x}", s.cursor);
            let raw_flags = s.read_32()?;
            let flags = Location::Cursor(s.cursor - 4).wrap(|| ArcAttr::from_gcno(raw_flags))?;
            Ok(Arc { dest_block, flags })
        })?;
        Ok(Arcs { src_block, arcs })
    }

    /// Parses the `LINES` record.
    ///
    /// # Errors
    ///
    /// * Returns [`Io`] on I/O failure.
    ///
    /// [`Io`]: ../error/enum.ErrorKind.html#variant.Io
    fn parse_lines(&mut self) -> Result<Lines> {
        trace!("lines-block-no @ 0x{:x}", self.cursor);
        let block_number = BlockIndex(self.read_32()?);
        trace!("lines-lines @ 0x{:x}", self.cursor);
        let mut lines: Vec<_> = self.until_eof(|s| {
            trace!("line-line-no @ 0x{:x}", s.cursor);
            let line_number = s.read_32()?;
            let line = if line_number != 0 {
                Line::LineNumber(line_number)
            } else {
                trace!("line-filename @ 0x{:x}", s.cursor);
                let filename = s.read_string()?;
                Line::FileName(filename)
            };
            Ok(line)
        })?;
        let _ = lines.pop(); // the last entry must be a null string which is useless.
        Ok(Lines {
            block_number,
            lines,
        })
    }

    /// Parses the `ARC_COUNTS` record.
    ///
    /// # Errors
    ///
    /// * Returns [`Io`] on I/O failure.
    ///
    /// [`Io`]: ../error/enum.ErrorKind.html#variant.Io
    fn parse_arc_counts(&mut self) -> Result<ArcCounts> {
        trace!("arc-counts-counts @ 0x{:x}", self.cursor);
        let counts = self.until_eof(Self::read_64)?;
        Ok(ArcCounts { counts })
    }

    /// Parses the `SUMMARY` record.
    ///
    /// # Errors
    ///
    /// * Returns [`Io`] on I/O failure.
    ///
    /// [`Io`]: ../error/enum.ErrorKind.html#variant.Io
    fn parse_summary(&mut self) -> Result<Summary> {
        trace!("summary-checksum @ 0x{:x}", self.cursor);
        let checksum = self.read_32()?;
        trace!("summary-num @ 0x{:x}", self.cursor);
        let num = self.read_32()?;
        trace!("summary-runs @ 0x{:x}", self.cursor);
        let runs = self.read_32()?;
        trace!("summary-sum @ 0x{:x}", self.cursor);
        let sum = self.read_64()?;
        trace!("summary-max @ 0x{:x}", self.cursor);
        let max = self.read_64()?;
        trace!("summary-sum-max @ 0x{:x}", self.cursor);
        let sum_max = self.read_64()?;

        trace!("summary-histogram @ 0x{:x}", self.cursor);
        let histogram = match self.read_histogram_bitvector() {
            Ok(bitvector) => {
                let mut bitpos = bitvector // @rustfmt-force-break
                    .iter()
                    .flat_map(|num| (0..32).map(move |i| num & 1 << i))
                    .enumerate()
                    .filter_map(|(i, b)| if b != 0 { Some(i as u32) } else { None });
                trace!("summary-histogram-buckets @ 0x{:x}", self.cursor);
                let buckets = self.until_eof(|s| {
                    let index = bitpos.next().unwrap_or(256);
                    trace!("histogram-bucket-num @ 0x{:x}", s.cursor);
                    let num = s.read_32()?;
                    trace!("histogram-bucket-min @ 0x{:x}", s.cursor);
                    let min = s.read_64()?;
                    trace!("histogram-bucket-sum @ 0x{:x}", s.cursor);
                    let sum = s.read_64()?;
                    Ok((index, HistogramBucket { num, min, sum }))
                })?;
                Some(Histogram { buckets })
            },
            Err(ref e) if e.is_eof() => None,
            Err(e) => bail!(e),
        };

        Ok(Summary {
            checksum,
            num,
            runs,
            sum,
            max,
            sum_max,
            histogram,
        })
    }
}

/// An iterator which reads from a reader until it produces an end-of-file error.
struct UntilEof<'a, S: 'a, T, F>(&'a mut S, F)
where
    F: FnMut(&mut S) -> Result<T>;

impl<'a, S: 'a, T, F> Iterator for UntilEof<'a, S, T, F>
where
    F: FnMut(&mut S) -> Result<T>,
{
    type Item = Result<T>;
    fn next(&mut self) -> Option<Result<T>> {
        match (self.1)(self.0) {
            Err(ref e) if e.is_eof() => {
                trace!("**** reached eof");
                None
            },
            x => Some(x),
        }
    }
}
