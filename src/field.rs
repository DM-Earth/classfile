use core::num::NonZero;

use bitflags::bitflags;

use crate::{
    Error,
    util::{Buf, BufMut, Decode, Encode},
};

/// Header of a field.
///
/// Don't forget that there're attributes followed!
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct FieldHeader {
    /// Access permission to and properties of this field.
    pub access_flags: FieldAccessFlags,
    /// Index of the name (`Utf8`) denoting this field in constant pool.
    pub name_index: NonZero<u16>,
    /// Index of the descriptor (`Utf8`) denoting this field in constant pool.
    pub descriptor_index: NonZero<u16>,
}

bitflags! {
    /// Denote access permissions to and properties of a field.
    #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
    pub struct FieldAccessFlags: u16 {
        /// Declared `public`; may be accessed from outside its package.
        const PUBLIC = 0x0001;
        /// Declared `private`;
        /// accessible only within the defining class and other classes belonging to the same nest.
        const PRIVATE = 0x0002;
        /// Declared `protected`; may be accessed within subclasses.
        const PROTECTED = 0x0004;
        /// Declared `static`.
        const STATIC = 0x0008;
        /// Declared `final`; never directly assigned to after object construction.
        const FINAL = 0x0010;
        /// Declared `volatile`; cannot be cached.
        const VIOLATE = 0x0040;
        /// Declared `transient`; not written or read by a persistent object manager.
        const TRANSIENT = 0x0080;
        /// Declared `synthetic`; not present in the source code.
        const SYNTHETIC = 0x1000;
        /// Declared as an element of an enum class.
        const ENUM = 0x4000;
    }
}

impl<'de> Decode<'de> for FieldHeader {
    fn decode<B: Buf<'de>>(mut buf: B) -> Result<Self, Error> {
        Ok(Self {
            access_flags: buf.read()?,
            name_index: buf.read()?,
            descriptor_index: buf.read()?,
        })
    }
}

impl Encode for FieldHeader {
    fn encode<B: BufMut>(&self, mut buf: B) -> Result<(), Error> {
        buf.write(self.access_flags)?;
        buf.write(self.name_index)?;
        buf.write(self.descriptor_index)?;
        Ok(())
    }
}

impl<'de> Decode<'de> for FieldAccessFlags {
    fn decode<B: Buf<'de>>(mut buf: B) -> Result<Self, Error> {
        let bits = buf.read()?;
        Self::from_bits(bits).ok_or(Error::UnknownAccessFlags(bits))
    }
}

impl Encode for FieldAccessFlags {
    fn encode<B: BufMut>(&self, mut buf: B) -> Result<(), Error> {
        buf.write(self.bits())
    }
}
