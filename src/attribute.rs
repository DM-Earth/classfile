use core::{fmt::Debug, num::NonZero};

use crate::{
    Error,
    util::{Buf, BufMut, Decode, Encode},
};

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
        let name_index = buf.read()?;
        let len: u32 = buf.read()?;
        Ok(Self {
            name_index,
            info: buf.read_slice(len as usize).ok_or(Error::UnexpectedEOF)?,
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
        buf.write(self.info.len() as u32)?;
        if buf.write_from_slice(self.info) != self.info.len() {
            return Err(Error::UnexpectedEOF);
        }
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
