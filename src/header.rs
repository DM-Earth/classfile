use core::fmt::{Debug, Display};

use crate::{
    Error,
    util::{Buf, BufMut, Decode, Encode},
};

pub(crate) const MAGIC: u32 = 0xCAFEBABE;

/// Header of a classfile.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ClassfileHeader {
    /// The version numbers of the classfile format.
    pub version: Version,
}

/// Versioning information of a classfile.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Version {
    /// Major version number of the classfile.
    pub major: u16,
    /// Minor version number of the classfile.
    pub minor: u16,
}

/// The implementation also decodes the magic number.
impl<'de> Decode<'de> for ClassfileHeader {
    fn decode<B: Buf<'de>>(mut buf: B) -> Result<Self, Error> {
        let magic: u32 = buf.read()?;
        if magic != MAGIC {
            return Err(Error::MalformedMagic(magic));
        }
        Ok(Self {
            version: buf.read()?,
        })
    }
}

impl<'de> Decode<'de> for Version {
    fn decode<B: Buf<'de>>(mut buf: B) -> Result<Self, Error> {
        let minor = buf.read()?;
        let major = buf.read()?;
        Ok(Self { major, minor })
    }
}

/// The implementation also encodes the magic number.
impl Encode for ClassfileHeader {
    fn encode<B: BufMut>(&self, mut buf: B) -> Result<(), Error> {
        buf.write(MAGIC)?;
        buf.write(self.version)?;
        Ok(())
    }
}

impl Encode for Version {
    fn encode<B: BufMut>(&self, mut buf: B) -> Result<(), Error> {
        buf.write(self.minor)?;
        buf.write(self.major)?;
        Ok(())
    }
}

impl Display for Version {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "{}.{}", self.major, self.minor)
    }
}

impl Debug for Version {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "{}", self)
    }
}
