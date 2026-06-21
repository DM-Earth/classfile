use core::{fmt::Debug, num::NonZero};

use crate::{
    Error,
    util::{Buf, BufMut, Decode, Encode},
};

mod vals;

/// Parser and writer implementation for a subset of attributes.
pub mod attributes {
    pub use super::vals::*;
}

/// An attribute.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Attribute<'a> {
    /// Index of the name (`Utf8`) denoting this attribute in constant pool.
    pub name_index: NonZero<u16>,
    /// Information about the attribute.
    /// The length of the byte slice should be no longer than [`u32::MAX`].
    pub info: &'a [u8],
}

impl<'de> Decode<'de> for Attribute<'de> {
    fn decode<B: Buf<'de>>(mut buf: B) -> Result<Self, Error> {
        Ok(Self {
            name_index: buf.read()?,
            info: buf.read()?,
        })
    }
}

impl Encode for Attribute<'_> {
    fn encode<B: BufMut>(&self, mut buf: B) -> Result<(), Error> {
        assert!(
            self.info.len() <= u32::MAX as usize,
            "length of string literal should be smaller than u32::MAX"
        );
        buf.write(self.name_index)?;
        buf.write(self.info)?;
        Ok(())
    }
}

impl Debug for Attribute<'_> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("Attribute")
            .field("name_index", &self.name_index)
            .finish_non_exhaustive()
    }
}
