//! Additional methods for libstd and external crates.

use fixedbitset::FixedBitSet;

use std::collections::{btree_map, hash_map};
use std::ffi::OsString;
use std::ops::Add;
use std::path::PathBuf;

/// Adds the `or_default` method to entry API.
pub trait EntryExt<'a> {
    /// Type of "Value" of a key-value pair in the Entry.
    type Value: 'a;
    /// Equivalent to `self.or_insert_with(Self::Value::default)`.
    fn or_default(self) -> &'a mut Self::Value;
}

impl<'a, K: Ord + 'a, V: Default + 'a> EntryExt<'a> for btree_map::Entry<'a, K, V> {
    type Value = V;
    fn or_default(self) -> &'a mut V {
        self.or_insert_with(V::default)
    }
}

impl<'a, K: 'a, V: Default + 'a> EntryExt<'a> for hash_map::Entry<'a, K, V> {
    type Value = V;
    fn or_default(self) -> &'a mut V {
        self.or_insert_with(V::default)
    }
}

/// Sets the whole `FixedBitSet` to ones.
pub fn fill_fixedbitset_with_ones(bitset: &mut FixedBitSet) {
    let excess_bits = bitset.len() % 32;
    let slice = bitset.as_mut_slice();
    for b in slice.iter_mut() {
        *b = !0;
    }
    if excess_bits != 0 {
        let last_item = slice.last_mut().expect("non-empty bitset");
        *last_item = 0x7fff_ffff >> (31 - excess_bits);
    }
}

/// Computes the sum of two 3-tuples.
pub fn tuple_3_add<A: Add<X>, B: Add<Y>, C: Add<Z>, X, Y, Z>(left: (A, B, C), right: (X, Y, Z)) -> (A::Output, B::Output, C::Output) {
    (left.0 + right.0, left.1 + right.1, left.2 + right.2)
}

/// Computes the sum of two 4-tuples.
pub fn tuple_4_add<A: Add<X>, B: Add<Y>, C: Add<Z>, D: Add<W>, X, Y, Z, W>(left: (A, B, C, D), right: (X, Y, Z, W)) -> (A::Output, B::Output, C::Output, D::Output) {
    (left.0 + right.0, left.1 + right.1, left.2 + right.2, left.3 + right.3)
}

/// Adds the `into_string_lossy` method to `OsString` and `Vec<u8>`.
pub trait IntoStringLossy {
    /// Consumes the ownership and converts the string-like object into a real string. Unconvertible characters are
    /// replaced by U+FFFD.
    fn into_string_lossy(self) -> String;
}

impl IntoStringLossy for OsString {
    fn into_string_lossy(self) -> String {
        self.into_string().unwrap_or_else(|s| s.to_string_lossy().into_owned())
    }
}

impl IntoStringLossy for Vec<u8> {
    fn into_string_lossy(self) -> String {
        String::from_utf8(self).unwrap_or_else(|e| String::from_utf8_lossy(&e.into_bytes()).into_owned())
    }
}

impl IntoStringLossy for PathBuf {
    fn into_string_lossy(self) -> String {
        self.into_os_string().into_string_lossy()
    }
}
