use num_traits::{Bounded, FromPrimitive, ToPrimitive};
#[cfg(feature = "serde")]
use serde::{Serialize, Serializer};
use shawshank::{self, ArenaSet};

#[cfg(feature = "serde")]
use std::collections::{BTreeMap, HashMap};
use std::fmt;
#[cfg(feature = "serde")]
use std::hash::Hash;
use std::ops::Index;

#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct Symbol(usize);

impl fmt::Debug for Symbol {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        write!(fmt, "Symbol({})", self.0)
    }
}

impl Bounded for Symbol {
    fn min_value() -> Self {
        Symbol(usize::min_value())
    }
    fn max_value() -> Self {
        Symbol(usize::max_value())
    }
}

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

impl From<usize> for Symbol {
    fn from(v: usize) -> Symbol {
        Symbol(v)
    }
}
impl From<Symbol> for usize {
    fn from(s: Symbol) -> usize {
        s.0
    }
}

/// The symbol representing the string `"<unknown>"`.
pub const UNKNOWN_SYMBOL: Symbol = Symbol(0);

pub struct Interner(ArenaSet<Box<str>, Symbol>);

impl Interner {
    pub fn new() -> Interner {
        let mut si = shawshank::Builder::<Box<str>, Symbol>::new().hash().unwrap();
        let symbol = si.intern("<unknown>").unwrap();
        debug_assert_eq!(symbol, UNKNOWN_SYMBOL);
        Interner(si)
    }

    pub fn intern(&mut self, s: Box<str>) -> Symbol {
        self.0.intern(s).unwrap()
    }

    // pub fn dump(&self) -> BTreeMap<Symbol, &str> {
    //     self.0.iter().collect()
    // }

    #[cfg(feature = "serde")]
    pub fn with<'si, 'a, T: 'a + SerializeWithInterner>(&'si self, value: &'a T) -> WithInterner<'si, 'a, T> {
        WithInterner {
            interner: self,
            value,
        }
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
        self.0.resolve(index).expect("valid symbol")
    }
}

#[cfg(feature = "serde")]
pub struct WithInterner<'si, 'a, T: 'a + SerializeWithInterner> {
    pub interner: &'si Interner,
    pub value: &'a T,
}

#[cfg(feature = "serde")]
pub trait SerializeWithInterner: Serialize {
    fn serialize_with_interner<S: Serializer>(&self, serializer: S, interner: &Interner) -> Result<S::Ok, S::Error>;
}

#[cfg(feature = "serde")]
impl<'si, 'a, T: 'a + SerializeWithInterner> Serialize for WithInterner<'si, 'a, T> {
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
        serializer.collect_map(self.iter().map(|(k, v)| (interner.with(k), interner.with(v))))
    }
}

#[cfg(feature = "serde")]
impl<K: SerializeWithInterner + Eq + Hash, V: SerializeWithInterner> SerializeWithInterner for HashMap<K, V> {
    fn serialize_with_interner<S: Serializer>(&self, serializer: S, interner: &Interner) -> Result<S::Ok, S::Error> {
        serializer.collect_map(self.iter().map(|(k, v)| (interner.with(k), interner.with(v))))
    }
}

#[cfg(feature = "serde")]
impl<T: SerializeWithInterner> SerializeWithInterner for Vec<T> {
    fn serialize_with_interner<S: Serializer>(&self, serializer: S, interner: &Interner) -> Result<S::Ok, S::Error> {
        serializer.collect_seq(self.iter().map(|value| interner.with(value)))
    }
}

#[cfg(feature = "serde")]
impl<T: SerializeWithInterner> SerializeWithInterner for Option<T> {
    fn serialize_with_interner<S: Serializer>(&self, serializer: S, interner: &Interner) -> Result<S::Ok, S::Error> {
        match *self {
            None => serializer.serialize_none(),
            Some(ref value) => serializer.serialize_some(&interner.with(value)),
        }
    }
}

#[cfg(feature = "serde")]
macro_rules! count_fields {
    () => { 0 };
    ($a:ident $($tail:ident)*) => { count_fields!($($tail)*) + 1 };
}

// poor man's workaround for `#[derive(SerializeWithInterner)]`
// to avoid needing to depend on an internal crate.
#[macro_export]
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

        #[cfg(feature="serde")]
        impl SerializeWithInterner for $struct_name {
            fn serialize_with_interner<S: Serializer>(&self, serializer: S, interner: &Interner) -> ::std::result::Result<S::Ok, S::Error> {
                use serde::ser::SerializeStruct;
                let mut state = serializer.serialize_struct(stringify!($struct_name), count_fields!($($field_name)*))?;
                $(
                    state.serialize_field(stringify!($field_name), &interner.with(&self.$field_name))?;
                )*
                state.end()
            }
        }
    };

    // directly forward to Serialize
    (direct: $($ty:ty),*) => {
        $(
            #[cfg(feature="serde")]
            impl SerializeWithInterner for $ty {
                fn serialize_with_interner<S: Serializer>(&self, serializer: S, _: &Interner) -> ::std::result::Result<S::Ok, S::Error> {
                    self.serialize(serializer)
                }
            }
        )*
    }
}

derive_serialize_with_interner! {
    direct: u32, u64, usize
}
