//! Deserialization with string interner.
//!
//! This module provides the [`deserializer_with_interner()`] function, which wraps an existing deserializer such that
//! whenever a [`Symbol`] is encountered, instead of reading an integer, it will read a string and intern it.
//!
//! [`deserializer_with_interner()`]: ./fn.deserializer_with_interner.html
//! [`Symbol`]: ../intern/struct.Symbol.html

#![cfg(feature = "serde")]

use intern::Interner;

use serde::de::*;

use std::cell::RefCell;
use std::fmt;

/// Wrapper of a deserializable object together with a string interner.
///
/// The interner is ready to intern deserialized strings as symbols on demand.
#[derive(Debug)]
pub struct WithInterner<'si, T> {
    // we use `&RefCell` instead of `&mut` because there are two functions where the interner needs to be shared twice:
    // * MapAccess::next_entry_seed
    // * EnumAccess::variant_seed
    interner: &'si RefCell<Interner>,
    value: T,
}

/// Adorns a deserializer with a string interner.
///
/// The resulting deserializer will parse a string whenever a [`Symbol`] is expected.
///
/// # Examples
///
/// ```rust
/// extern crate cov;
/// extern crate serde;
/// extern crate serde_json;
/// use cov::{Interner, Symbol, deserializer_with_interner};
/// use serde::Deserialize;
/// use std::cell::RefCell;
///
/// # fn main() { run().unwrap(); }
/// # fn run() -> serde_json::Result<()> {
/// // Prepare the JSON input.
/// let input = r#"["hello", "world", "hello", "everyone"]"#;
/// let mut json_de = serde_json::de::Deserializer::from_str(input);
///
/// // Include a string interner.
/// let interner = RefCell::new(Interner::new());
/// let de = deserializer_with_interner(&mut json_de, &interner);
///
/// // Deserialize the JSON into a vector of Symbols
/// let result = Vec::<Symbol>::deserialize(de)?;
///
/// // Compare with expected output.
/// let mut interner = interner.borrow_mut();
/// let hello = interner.intern("hello");
/// let world = interner.intern("world");
/// let everyone = interner.intern("everyone");
/// assert_eq!(result, vec![hello, world, hello, everyone]);
/// # Ok(()) }
/// ```
///
/// [`Symbol`]: ../intern/struct.Symbol.html
pub fn deserializer_with_interner<'de, D: Deserializer<'de>>(deserializer: D, interner: &RefCell<Interner>) -> WithInterner<D> {
    WithInterner {
        interner,
        value: deserializer,
    }
}

macro_rules! wrap {
    ($self:ident . $f:ident($($value:ident),+ $(;$prev_args:expr)*)) => {{
        $(let $value = WithInterner {
            interner: $self.interner,
            value: $value,
        };)+
        $self.value.$f($($prev_args,)* $($value),+)
    }}
}

macro_rules! forward_deserializer {
    ($name:ident $(, $arg:ident: $ty:ty)*) => {
        fn $name<V: Visitor<'de>>(self, $($arg: $ty,)* visitor: V) -> Result<V::Value, D::Error> {
            wrap!(self.$name(visitor $(;$arg)*))
        }
    }
}

impl<'si, 'de, D: Deserializer<'de>> Deserializer<'de> for WithInterner<'si, D> {
    type Error = D::Error;

    forward_deserializer!(deserialize_any);
    forward_deserializer!(deserialize_bool);
    forward_deserializer!(deserialize_u8);
    forward_deserializer!(deserialize_u16);
    forward_deserializer!(deserialize_u32);
    forward_deserializer!(deserialize_u64);
    forward_deserializer!(deserialize_i8);
    forward_deserializer!(deserialize_i16);
    forward_deserializer!(deserialize_i32);
    forward_deserializer!(deserialize_i64);
    forward_deserializer!(deserialize_f32);
    forward_deserializer!(deserialize_f64);
    forward_deserializer!(deserialize_char);
    forward_deserializer!(deserialize_str);
    forward_deserializer!(deserialize_string);
    forward_deserializer!(deserialize_unit);
    forward_deserializer!(deserialize_option);
    forward_deserializer!(deserialize_seq);
    forward_deserializer!(deserialize_bytes);
    forward_deserializer!(deserialize_byte_buf);
    forward_deserializer!(deserialize_map);
    forward_deserializer!(deserialize_unit_struct, name: &'static str);
    forward_deserializer!(deserialize_tuple_struct, name: &'static str, len: usize);
    forward_deserializer!(deserialize_struct, name: &'static str, fields: &'static [&'static str]);
    forward_deserializer!(deserialize_identifier);
    forward_deserializer!(deserialize_tuple, len: usize);
    forward_deserializer!(deserialize_enum, name: &'static str, variants: &'static [&'static str]);
    forward_deserializer!(deserialize_ignored_any);

    fn deserialize_newtype_struct<V: Visitor<'de>>(self, name: &'static str, visitor: V) -> Result<V::Value, Self::Error> {
        if name == "Symbol" {
            let v = ToSymbol {
                interner: self.interner,
                value: visitor,
            };
            self.value.deserialize_newtype_struct(name, v)
        } else {
            wrap!(self.deserialize_newtype_struct(visitor; name))
        }
    }
}



macro_rules! forward_visitor {
    ($name:ident($($arg:ident: $ty:ty),*)) => {
        fn $name<E: Error>(self $(, $arg: $ty)*) -> Result<V::Value, E> {
            self.value.$name($($arg),*)
        }
    };
    ($name:ident <$n:ident: $ty:ident>) => {
        fn $name<$n: $ty<'de>>(self, value: $n) -> Result<V::Value, $n::Error> {
            wrap!(self.$name(value))
        }
    };
}

impl<'si, 'de, V: Visitor<'de>> Visitor<'de> for WithInterner<'si, V> {
    type Value = V::Value;

    fn expecting(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        self.value.expecting(fmt)
    }

    forward_visitor!(visit_bool(v: bool));
    forward_visitor!(visit_i8(v: i8));
    forward_visitor!(visit_i16(v: i16));
    forward_visitor!(visit_i32(v: i32));
    forward_visitor!(visit_i64(v: i64));
    forward_visitor!(visit_u8(v: u8));
    forward_visitor!(visit_u16(v: u16));
    forward_visitor!(visit_u32(v: u32));
    forward_visitor!(visit_u64(v: u64));
    forward_visitor!(visit_f32(v: f32));
    forward_visitor!(visit_f64(v: f64));
    forward_visitor!(visit_char(v: char));
    forward_visitor!(visit_str(v: &str));
    forward_visitor!(visit_string(v: String));
    forward_visitor!(visit_borrowed_str(v: &'de str));
    forward_visitor!(visit_bytes(v: &[u8]));
    forward_visitor!(visit_borrowed_bytes(v: &'de [u8]));
    forward_visitor!(visit_byte_buf(v: Vec<u8>));
    forward_visitor!(visit_none());
    forward_visitor!(visit_some<D: Deserializer>);
    forward_visitor!(visit_unit());
    forward_visitor!(visit_newtype_struct<D: Deserializer>);
    forward_visitor!(visit_seq<A: SeqAccess>);
    forward_visitor!(visit_map<A: MapAccess>);
    forward_visitor!(visit_enum<A: EnumAccess>);
}

impl<'si, 'de, A: SeqAccess<'de>> SeqAccess<'de> for WithInterner<'si, A> {
    type Error = A::Error;

    fn next_element_seed<T: DeserializeSeed<'de>>(&mut self, seed: T) -> Result<Option<T::Value>, A::Error> {
        wrap!(self.next_element_seed(seed))
    }

    fn size_hint(&self) -> Option<usize> {
        self.value.size_hint()
    }
}

impl<'si, 'de, A: MapAccess<'de>> MapAccess<'de> for WithInterner<'si, A> {
    type Error = A::Error;

    fn next_key_seed<K: DeserializeSeed<'de>>(&mut self, seed: K) -> Result<Option<K::Value>, A::Error> {
        wrap!(self.next_key_seed(seed))
    }

    fn next_value_seed<V: DeserializeSeed<'de>>(&mut self, seed: V) -> Result<V::Value, A::Error> {
        wrap!(self.next_value_seed(seed))
    }

    fn next_entry_seed<K: DeserializeSeed<'de>, V: DeserializeSeed<'de>>(&mut self, key_seed: K, value_seed: V) -> Result<Option<(K::Value, V::Value)>, A::Error> {
        wrap!(self.next_entry_seed(key_seed, value_seed))
    }
}

impl<'si, 'de, A: EnumAccess<'de>> EnumAccess<'de> for WithInterner<'si, A> {
    type Error = A::Error;
    type Variant = WithInterner<'si, A::Variant>;

    fn variant_seed<V: DeserializeSeed<'de>>(self, seed: V) -> Result<(V::Value, Self::Variant), A::Error> {
        let (value, variant) = wrap!(self.variant_seed(seed))?;
        let variant = WithInterner {
            interner: self.interner,
            value: variant,
        };
        Ok((value, variant))
    }
}

impl<'si, 'de, A: VariantAccess<'de>> VariantAccess<'de> for WithInterner<'si, A> {
    type Error = A::Error;

    fn unit_variant(self) -> Result<(), Self::Error> {
        self.value.unit_variant()
    }

    fn newtype_variant_seed<T: DeserializeSeed<'de>>(self, seed: T) -> Result<T::Value, A::Error> {
        wrap!(self.newtype_variant_seed(seed))
    }

    fn tuple_variant<V: Visitor<'de>>(self, len: usize, visitor: V) -> Result<V::Value, A::Error> {
        wrap!(self.tuple_variant(visitor; len))
    }

    fn struct_variant<V: Visitor<'de>>(self, fields: &'static [&'static str], visitor: V) -> Result<V::Value, A::Error> {
        wrap!(self.struct_variant(visitor; fields))
    }
}

impl<'si, 'de, T: DeserializeSeed<'de>> DeserializeSeed<'de> for WithInterner<'si, T> {
    type Value = T::Value;

    fn deserialize<D: Deserializer<'de>>(self, deserializer: D) -> Result<T::Value, D::Error> {
        wrap!(self.deserialize(deserializer))
    }
}

/// A serde deserialization object which interns all encountered strings and emit a [`Symbol`].
///
/// [`Symbol`]: ./struct.Symbol.html
struct ToSymbol<'si, T> {
    interner: &'si RefCell<Interner>,
    value: T,
}

impl<'si, 'de, V: Visitor<'de>> Visitor<'de> for ToSymbol<'si, V> {
    type Value = V::Value;

    fn expecting(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        fmt.write_str("string")
    }

    fn visit_str<E: Error>(self, v: &str) -> Result<V::Value, E> {
        let symbol: usize = self.interner.borrow_mut().intern(v).into();
        self.value.visit_u64(symbol as u64)
    }

    fn visit_string<E: Error>(self, v: String) -> Result<V::Value, E> {
        let symbol: usize = self.interner.borrow_mut().intern(v).into();
        self.value.visit_u64(symbol as u64)
    }

    fn visit_newtype_struct<D: Deserializer<'de>>(self, deserializer: D) -> Result<V::Value, D::Error> {
        let deserializer = ToSymbol {
            interner: self.interner,
            value: deserializer,
        };
        self.value.visit_newtype_struct(deserializer)
    }
}

impl<'si, 'de, D: Deserializer<'de>> Deserializer<'de> for ToSymbol<'si, D> {
    type Error = D::Error;

    fn deserialize_any<V: Visitor<'de>>(self, _: V) -> Result<V::Value, D::Error> {
        unreachable!()
    }

    fn deserialize_u64<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, D::Error> {
        let visitor = ToSymbol {
            interner: self.interner,
            value: visitor,
        };
        self.value.deserialize_u64(visitor)
    }

    forward_to_deserialize_any! {
        bool
        i8 i16 i32 i64
        u8 u16 u32
        f32 f64
        char
        bytes
        byte_buf
        str
        string
        option
        unit
        unit_struct
        newtype_struct
        seq
        tuple
        tuple_struct
        map
        struct
        enum
        identifier
        ignored_any
    }
}

#[test]
fn test_deserialize_symbol() {
    use intern::Symbol;
    use serde_json::de::Deserializer;

    #[derive(Deserialize, Debug, PartialEq, Eq)]
    struct Foo {
        a: String,
        b: Option<Symbol>,
        c: Vec<Symbol>,
        d: (String, Symbol),
    }

    let mut interner = Interner::new();
    let s2 = interner.intern("s2");
    let s3 = interner.intern("s3");
    let s4 = interner.intern("s4");

    let expected = Foo {
        a: "s1".to_owned(),
        b: Some(s2),
        c: vec![s3, s4],
        d: ("s3".to_owned(), s4),
    };

    let mut deserializer = Deserializer::from_str(
        r#"{
            "a": "s1",
            "b": "s2",
            "c": ["s3", "s4"],
            "d": ["s3", "s4"]
        }"#,
    );
    let interner = RefCell::new(interner);
    let deserializer = deserializer_with_interner(&mut deserializer, &interner);
    let actual = Foo::deserialize(deserializer).expect("deserialized");

    assert_eq!(expected, actual);
}
