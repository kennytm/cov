//! String interning.
//!
//! GCNO files contain a lot of repeated strings like the filenames and function names. In order to save time and memory
//! comparing these strings, they will all be stored into the [`Interner`] class, and normal operations that does not
//! involve the actual string content are done through the proxy [`Symbol`] handles.
//!
//! ```rust
//! use cov::Interner;
//!
//! let mut interner = Interner::new();
//!
//! // The interner can intern `Box<str>`s.
//! let symbol_1 = interner.intern("hello");
//! let symbol_2 = interner.intern("hello");
//!
//! // Equal strings have equal symbols.
//! assert_eq!(symbol_1, symbol_2);
//!
//! // Get back the string by indexing.
//! assert_eq!("hello", &interner[symbol_1]);
//! ```
//!
//! ## Serialization
//!
//! A [`Symbol`] is just a plain integer, and will simply serialize to a number. To make it a write string, the
//! `Interner` must be transmitted to the serializer. This is done via the [`with_interner()`] method when serializing.
//!
//! ```rust
//! extern crate serde_json;
//! extern crate cov;
//! use cov::{Interner, SerializeWithInterner};
//!
//! # fn main() { run().unwrap(); }
//! # fn run() -> serde_json::Result<()> {
//! let mut interner = Interner::new();
//! let a = interner.intern("one");
//! let b = interner.intern("two");
//! let c = interner.intern("three");
//! let value = vec![a, b, c, a];
//!
//! // without the interner, the symbols will be serialized as numbers.
//! let serialized_without_interner = serde_json::to_string(&value)?;
//! assert_eq!(&serialized_without_interner, "[1,2,3,1]");
//!
//! // use .with_interner() to serialize them into strings.
//! let serialized = serde_json::to_string(&value.with_interner(&interner))?;
//! assert_eq!(&serialized, r#"["one","two","three","one"]"#);
//! # Ok(()) }
//! ```
//!
//! ## Deserialization
//!
//! See [`deserializer::with_interner()`] for how to deserialize a string back to a [`Symbol`].
//!
//! [`Interner`]: ./struct.Interner.html
//! [`Symbol`]: ./struct.Symbol.html
//! [`with_interner()`]: ./trait.SerializeWithInterner.html#method.with_interner
//! [`deserializer::with_interner()`]: ../deserializer/fn.with_interner.html

use num_traits::{Bounded, FromPrimitive, ToPrimitive};
#[cfg(feature = "serde")]
use serde::{Serialize, Serializer};
use shawshank::{self, ArenaSet};

use std::borrow::Borrow;
#[cfg(feature = "serde")]
use std::collections::{BTreeMap, HashMap};
use std::fmt;
#[cfg(feature = "serde")]
use std::hash::Hash;
use std::ops::Index;
#[cfg(feature = "serde")]
use std::path::PathBuf;

/// A handle to an interned string in an [`Interner`].
///
/// `Symbol` is a wrapper of `usize`, and can be cheaply copied and compared. `Symbol`s are produced by calling
/// [`Interner::intern()`].
///
/// [`Interner`]: ./struct.Interner.html
/// [`Interner::intern()`]: ./struct.Interner.html#method.intern
#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct Symbol(usize);

impl fmt::Debug for Symbol {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        write!(fmt, "Symbol({})", self.0)
    }
}

// only needed by shawshank, and it is unsafe for the user to create a Symbol not via Interner.
#[doc(hidden)]
impl Bounded for Symbol {
    fn min_value() -> Self {
        Symbol(usize::min_value())
    }
    fn max_value() -> Self {
        Symbol(usize::max_value())
    }
}

// only needed by shawshank, and it is unsafe for the user to create a Symbol not via Interner.
#[doc(hidden)]
impl FromPrimitive for Symbol {
    fn from_i64(n: i64) -> Option<Self> {
        usize::from_i64(n).map(Symbol)
    }
    fn from_u64(n: u64) -> Option<Self> {
        usize::from_u64(n).map(Symbol)
    }
    fn from_usize(n: usize) -> Option<Self> {
        Some(Symbol(n))
    }
}

impl ToPrimitive for Symbol {
    fn to_i64(&self) -> Option<i64> {
        self.0.to_i64()
    }
    fn to_u64(&self) -> Option<u64> {
        self.0.to_u64()
    }
    fn to_usize(&self) -> Option<usize> {
        Some(self.0)
    }
}

impl From<Symbol> for usize {
    fn from(s: Symbol) -> usize {
        s.0
    }
}

/// The symbol representing the string `"<unknown>"`.
pub const UNKNOWN_SYMBOL: Symbol = Symbol(0);

/// The string interner.
///
/// See the [module documentation](index.html) for detail.
#[cfg_attr(feature = "cargo-clippy", allow(stutter))]
pub struct Interner(ArenaSet<Box<str>, Symbol>);

impl Interner {
    /// Creates a new interner.
    pub fn new() -> Interner {
        let mut si = shawshank::Builder::<Box<str>, Symbol>::new().hash().expect("build ArenaSet");
        let symbol = si.intern("<unknown>").expect("intern '<unknown>'");
        debug_assert_eq!(symbol, UNKNOWN_SYMBOL);
        Interner(si)
    }

    /// Inserts a string into the interner. Returns a [`Symbol`] which can be use to extract the
    /// original string.
    ///
    /// [`Symbol`]: ./struct.Symbol.html
    pub fn intern<S>(&mut self, s: S) -> Symbol
    where
        S: Borrow<str>,
        Box<str>: From<S>,
    {
        // Our interner is not bounded, so this will never return Err.
        self.0.intern(s).expect("unbounded ArenaSet")
    }

    /// Iterates the content of the interner.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use cov::Interner;
    ///
    /// let mut interner = Interner::new();
    /// let s1 = interner.intern("one");
    /// let s2 = interner.intern("two");
    /// let s3 = interner.intern("three");
    ///
    /// let iter_res = interner.iter().collect::<Vec<_>>();
    /// assert_eq!(iter_res, vec![
    ///     (s1, "one"),
    ///     (s2, "two"),
    ///     (s3, "three"),
    /// ]);
    /// ```
    pub fn iter(&self) -> Iter {
        Iter {
            interner: self,
            current_index: 1, // don't give out UNKNOWN_SYMBOL.
        }
    }
}

impl fmt::Debug for Interner {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        write!(fmt, "Interner {{ /* {} strings */ }}", self.0.count())
    }
}

impl Default for Interner {
    fn default() -> Interner {
        Interner::new()
    }
}

impl Index<Symbol> for Interner {
    type Output = str;
    fn index(&self, index: Symbol) -> &str {
        // since we never call disintern, the interner vector will be dense and
        // the only cause of returning Err is user constructing an invalid
        // symbol via Deserialize/FromPrimitive/Bounded.
        self.0.resolve(index).expect("valid symbol")
    }
}

/// Iterator of [`Interner`].
///
/// [`Interner`]: ./struct.Interner.html
#[derive(Debug)]
pub struct Iter<'a> {
    interner: &'a Interner,
    current_index: usize,
}

impl<'a> Iterator for Iter<'a> {
    type Item = (Symbol, &'a str);
    fn next(&mut self) -> Option<Self::Item> {
        if self.current_index >= self.interner.0.capacity() {
            None
        } else {
            let symbol = Symbol(self.current_index);
            self.current_index += 1;
            Some((symbol, &self.interner[symbol]))
        }
    }
}

/// Return type of [`SerializeWithInterner::with_interner()`].
///
/// [`SerializeWithInterner::with_interner()`]: ./trait.SerializeWithInterner.html#method.with_interner
#[cfg(feature = "serde")]
#[derive(Debug)]
pub struct WithInterner<'si, T> {
    interner: &'si Interner,
    value: T,
}

/// A data structure that may contain [`Symbol`]s, which should be serialized as strings.
///
/// [`Symbol`]: ./struct.Symbol.html
#[cfg(feature = "serde")]
pub trait SerializeWithInterner {
    /// Adorns this object with a string interner.
    ///
    /// Returns a serializable object which writes out [`Symbol`]s as strings instead of numbers.
    ///
    /// [`Symbol`]: ./struct.Symbol.html
    fn with_interner<'si>(&self, interner: &'si Interner) -> WithInterner<'si, &Self> {
        WithInterner {
            interner: interner,
            value: self,
        }
    }

    /// Serializes this value with help from an [`Interner`] that writes [`Symbol`]s as strings.
    ///
    /// If any member of this type is expected to contain a `Symbol`, they should be wrapped using [`with_interner()`],
    /// so that deeply nested `Symbol`s can be recursively found and transformed.
    ///
    /// # Examples
    ///
    /// ```rust
    /// extern crate cov;
    /// extern crate serde;
    /// extern crate serde_json;
    /// use cov::{Symbol, Interner, SerializeWithInterner};
    /// use serde::ser::{Serializer, SerializeStruct};
    ///
    /// struct SymbolCounters {
    ///     symbols: Vec<Symbol>,
    ///     counters: Vec<u32>,
    /// }
    ///
    /// impl SerializeWithInterner for SymbolCounters {
    ///     fn serialize_with_interner<S: Serializer>(
    ///         &self,
    ///         serializer: S,
    ///         interner: &Interner
    ///     ) -> Result<S::Ok, S::Error> {
    ///         let mut state = serializer.serialize_struct("SymbolCounters", 2)?;
    ///         state.serialize_field("symbols", &self.symbols.with_interner(interner))?;
    ///         // ^ note the use of `with_interner()`, since a Vec<Symbol> contains Symbol.
    ///         state.serialize_field("counters", &self.counters)?;
    ///         // ^ no need to call `with_interner()` for irrelevant types.
    ///         state.end()
    ///     }
    /// }
    ///
    /// // ...
    ///
    /// # fn main() { run().unwrap(); }
    /// # fn run() -> serde_json::Result<()> {
    /// let mut interner = Interner::new();
    /// let s1 = interner.intern("one");
    /// let s2 = interner.intern("two");
    ///
    /// let symbol_counters = SymbolCounters {
    ///     symbols: vec![s1, s2],
    ///     counters: vec![45, 67],
    /// };
    /// let serialized = serde_json::to_string(&symbol_counters.with_interner(&interner))?;
    /// assert_eq!(&serialized, r#"{"symbols":["one","two"],"counters":[45,67]}"#);
    /// # Ok(()) }
    /// ```
    ///
    /// [`Symbol`]: ./struct.Symbol.html
    /// [`Interner`]: ./struct.Interner.html
    /// [`with_interner()`]: #method.with_interner
    fn serialize_with_interner<S: Serializer>(&self, serializer: S, interner: &Interner) -> Result<S::Ok, S::Error>;
}

#[cfg(feature = "serde")]
impl<'si, T: SerializeWithInterner> Serialize for WithInterner<'si, T> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        self.value.serialize_with_interner(serializer, self.interner)
    }
}

#[cfg(feature = "serde")]
impl SerializeWithInterner for Symbol {
    fn serialize_with_interner<S: Serializer>(&self, serializer: S, interner: &Interner) -> Result<S::Ok, S::Error> {
        interner[*self].serialize(serializer)
    }
}

#[cfg(feature = "serde")]
impl<K: SerializeWithInterner + Ord, V: SerializeWithInterner> SerializeWithInterner for BTreeMap<K, V> {
    fn serialize_with_interner<S: Serializer>(&self, serializer: S, interner: &Interner) -> Result<S::Ok, S::Error> {
        serializer.collect_map(self.iter().map(|(k, v)| (k.with_interner(interner), v.with_interner(interner))))
    }
}

#[cfg(feature = "serde")]
impl<K: SerializeWithInterner + Eq + Hash, V: SerializeWithInterner> SerializeWithInterner for HashMap<K, V> {
    fn serialize_with_interner<S: Serializer>(&self, serializer: S, interner: &Interner) -> Result<S::Ok, S::Error> {
        serializer.collect_map(self.iter().map(|(k, v)| (k.with_interner(interner), v.with_interner(interner))))
    }
}

#[cfg(feature = "serde")]
impl<T: SerializeWithInterner> SerializeWithInterner for Vec<T> {
    fn serialize_with_interner<S: Serializer>(&self, serializer: S, interner: &Interner) -> Result<S::Ok, S::Error> {
        serializer.collect_seq(self.iter().map(|value| value.with_interner(interner)))
    }
}

#[cfg(feature = "serde")]
impl<T: SerializeWithInterner> SerializeWithInterner for Option<T> {
    fn serialize_with_interner<S: Serializer>(&self, serializer: S, interner: &Interner) -> Result<S::Ok, S::Error> {
        match *self {
            None => serializer.serialize_none(),
            Some(ref value) => serializer.serialize_some(&value.with_interner(interner)),
        }
    }
}

#[cfg(feature = "serde")]
impl<'a, T: 'a + SerializeWithInterner + ?Sized> SerializeWithInterner for &'a T {
    fn serialize_with_interner<S: Serializer>(&self, serializer: S, interner: &Interner) -> Result<S::Ok, S::Error> {
        (**self).serialize_with_interner(serializer, interner)
    }
}

#[cfg(feature = "serde")]
macro_rules! count_fields {
    () => { 0 };
    ($a:ident $($tail:ident)*) => { count_fields!($($tail)*) + 1 };
}

// poor man's workaround for `#[derive(SerializeWithInterner)]` to avoid needing to depend on an internal crate.
macro_rules! derive_serialize_with_interner {
    // derive for struct
    (
        $(#[$struct_attr:meta])*
        pub struct $struct_name:ident {
            $(
                $(#[$field_attr:meta])*
                pub $field_name:ident: $field_ty:ty,
            )+
        }
    ) => {
        $(#[$struct_attr])*
        pub struct $struct_name {
            $(
                $(#[$field_attr])*
                pub $field_name: $field_ty,
            )+
        }

        #[cfg(feature = "serde")]
        impl SerializeWithInterner for $struct_name {
            fn serialize_with_interner<S: Serializer>(&self, serializer: S, interner: &Interner) -> ::std::result::Result<S::Ok, S::Error> {
                use serde::ser::SerializeStruct;
                let mut state = serializer.serialize_struct(stringify!($struct_name), count_fields!($($field_name)*))?;
                $(
                    state.serialize_field(stringify!($field_name), &self.$field_name.with_interner(interner))?;
                )*
                state.end()
            }
        }
    };

    // directly forward to Serialize for types that do not contain Symbol.
    (direct: $($ty:ty),*) => {
        $(
            #[cfg(feature = "serde")]
            impl SerializeWithInterner for $ty {
                fn serialize_with_interner<S: Serializer>(&self, serializer: S, _: &Interner) -> ::std::result::Result<S::Ok, S::Error> {
                    self.serialize(serializer)
                }
            }
        )*
    }
}

derive_serialize_with_interner! {
    direct: u32, u64, usize, PathBuf
}
