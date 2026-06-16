use core::num::NonZero;

use bitflags::bitflags;

use crate::{
    Error,
    util::{Buf, BufMut, Decode, Encode},
};

/// Metadata of the class.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ClassMetadata {
    /// Access permissions to and properties of this class or interface.
    pub access_flags: ClassAccessFlags,
    /// Index of the class or interface (`Class`) defined by this classfile in constant pool.
    pub this_class: NonZero<u16>,
    /// Index of the direct superclass (`Class`) of this class in constant pool.
    pub super_class: Option<NonZero<u16>>,
}

bitflags! {
    /// Denote access permissions to and properties of a class or interface.
    #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
    pub struct ClassAccessFlags: u16 {
        /// Declared `public`; may be accessed from outside its package.
        const PUBLIC = 0x0001;
        /// Declared `final`; no subclasses allowed.
        const FINAL = 0x0010;
        /// Treat superclass methods specially when invoked by the *invokespecial* instruction.
        const SUPER = 0x0020;
        /// Is an interface, not a class.
        const INTERFACE = 0x0200;
        /// Declared `abstract`; must not be instantiated.
        const ABSTRACT = 0x0400;
        /// Declared `synthetic`; not present in the source code.
        const SYNTHETIC = 0x1000;
        /// Declared as an annotation interface.
        const ANNOTATION = 0x2000;
        /// Declared as an enum class.
        const ENUM = 0x4000;
        /// Is a module, not a class or interface.
        const MODULE = 0x8000;
    }
}

impl<'de> Decode<'de> for ClassMetadata {
    fn decode<B: Buf<'de>>(mut buf: B) -> Result<Self, Error> {
        Ok(Self {
            access_flags: buf.read()?,
            this_class: buf.read()?,
            super_class: buf.read()?,
        })
    }
}

impl Encode for ClassMetadata {
    fn encode<B: BufMut>(&self, mut buf: B) -> Result<(), Error> {
        buf.write(self.access_flags)?;
        buf.write(self.this_class)?;
        buf.write(self.super_class)?;
        Ok(())
    }
}

impl<'de> Decode<'de> for ClassAccessFlags {
    fn decode<B: Buf<'de>>(mut buf: B) -> Result<Self, Error> {
        let bits = buf.read()?;
        Self::from_bits(bits).ok_or(Error::UnknownAccessFlags(bits))
    }
}

impl Encode for ClassAccessFlags {
    fn encode<B: BufMut>(&self, mut buf: B) -> Result<(), Error> {
        buf.write(self.bits())
    }
}
