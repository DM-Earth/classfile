//! Reader and writer for the JVM classfile format.
//!
//! Both [`Reader`] and [`Writer`] are stream-based: you walk the classfile
//! section by section and decide for each item whether to inspect it,
//! transform it, or pass it through untouched. Structured types such as
//! [`ClassfileHeader`] and [`ClassMetadata`] are layered on top, so reading
//! and writing stays ergonomic without buffering the whole classfile.
//!
//! # Example
//!
//! ```
//! use classfile::{Error, Reader};
//!
//! // A minimal, well-formed classfile header.
//! let bytes: &[u8] = &[
//!     0xCA, 0xFE, 0xBA, 0xBE, // magic
//!     0x00, 0x00, // minor version
//!     0x00, 0x41, // major version
//!     0x00, 0x01, // constant_pool_count (no entries)
//!     0x00, 0x21, // access_flags
//!     0x00, 0x01, // this_class
//!     0x00, 0x00, // super_class
//!     0x00, 0x00, // interfaces_count
//!     0x00, 0x00, // fields_count
//!     0x00, 0x00, // methods_count
//!     0x00, 0x00, // attributes_count
//! ];
//!
//! let (header, reader) = Reader::new(bytes).header()?;
//! assert_eq!(header.version.major, 0x41);
//! let reader = reader.finish()?;
//! let (metadata, _reader) = reader.metadata()?;
//! assert!(metadata.access_flags.contains(classfile::ClassAccessFlags::SUPER));
//! # Ok::<(), Error>(())
//! ```

#![no_std]

#[cfg(feature = "alloc")]
extern crate alloc;

mod util;

pub use util::{SealedBuf as Buf, SealedBufMut as BufMut};

mod attribute;
mod constant;
mod field;
mod header;
mod meta;
mod method;

use core::fmt::{Debug, Display};

pub use attribute::{Attribute, attributes};
pub use constant::{ConstantPoolEntry, ReferenceKind};
pub use field::{FieldAccessFlags, FieldHeader};
pub use header::{ClassfileHeader, Version};
pub use meta::{ClassAccessFlags, ClassMetadata};
pub use method::{MethodAccessFlags, MethodHeader};

mod reader;
mod writer;

pub use reader::*;
pub use writer::*;

/// Errors that occur during reading or writing a classfile.
#[derive(Debug, Clone)]
#[non_exhaustive]
#[allow(variant_size_differences)]
pub enum Error {
    /// Unexpected end of file while parsing or writing into a slice.
    UnexpectedEOF,
    /// Malformed magic number.
    MalformedMagic(u32),
    /// Parsed a zero value where it shouldn't be.
    UnexpectedZero,
    /// Unknown reference kind found during parsing.
    UnknownReferenceKind(u8),
    /// Unknown constant pool entry tag found during parsing.
    UnknownConstantTag(u8),
    /// Unknown verification type tag found during parsing.
    UnknownVerificationType(u8),
    /// Unknown frame type tag found during parsing.
    UnknownFrameType(u8),
    /// Unknown element value tag found during parsing.
    UnknownElementValueType(u8),
    /// Unknown annotation target tag found during parsing.
    UnknownAnnotationTargetType(u8),
    /// Unknown type path kind found during parsing.
    UnknownTypePathKind(u8),
    /// Index out of bounds.
    IndexOfBounds,
    /// Offset out of bounds.
    OffsetOutOfBounds,
}

impl Display for Error {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Error::UnexpectedEOF => write!(f, "unexpected end of file"),
            Error::UnexpectedZero => write!(f, "unexpected zero value"),
            Error::MalformedMagic(num) => write!(
                f,
                "malformed magic number: {:#04X}, expected {:#04X}",
                num,
                header::MAGIC
            ),
            Error::UnknownReferenceKind(kind) => write!(f, "unknown reference kind: {kind}"),
            Error::UnknownConstantTag(tag) => write!(f, "unknown constant pool entry tag: {tag}"),
            Error::UnknownFrameType(tag) => write!(f, "unknown frame type: {tag}"),
            Error::UnknownElementValueType(tag) => write!(f, "unknown element value type: {tag}"),
            Error::UnknownAnnotationTargetType(tag) => {
                write!(f, "unknown annotation target type: {tag}")
            }
            Error::UnknownTypePathKind(tag) => write!(f, "unknown type path kind: {tag}"),
            Error::UnknownVerificationType(tag) => {
                write!(f, "unknown verification type tag: {tag}")
            }
            Error::IndexOfBounds => write!(f, "index out of bounds"),
            Error::OffsetOutOfBounds => write!(f, "offset out of bounds"),
        }
    }
}

impl core::error::Error for Error {}

/// String literal encoded in Modified UTF-8.
///
/// The length of the byte slice should be no longer than [`u16::MAX`].
/// You may want to use `cesu8` or `simd_cesu8` crate for decoding (or vice versa).
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ModifiedUtf8<'a>(pub &'a [u8]);

impl util::Encode for ModifiedUtf8<'_> {
    fn encode<B: util::BufMut>(&self, mut buf: B) -> Result<(), Error> {
        assert!(
            self.0.len() <= u16::MAX as usize,
            "length of string literal should be smaller than u16::MAX"
        );
        buf.write(self.0.len() as u16)?;
        if buf.write_from_slice(self.0) == self.0.len() {
            Ok(())
        } else {
            Err(Error::UnexpectedEOF)
        }
    }
}

impl<'a> util::Decode<'a> for ModifiedUtf8<'a> {
    fn decode<B: util::Buf<'a>>(mut buf: B) -> Result<Self, Error> {
        let len: u16 = buf.read()?;
        buf.read_slice(len as usize)
            .map(Self)
            .ok_or(Error::UnexpectedEOF)
    }
}

impl Debug for ModifiedUtf8<'_> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "\"")?;
        for chunk in self.0.utf8_chunks() {
            write!(f, "{}", chunk.valid())?;
            for _ in 0..chunk.invalid().len() {
                write!(f, "\u{FFFD}")?;
            }
        }
        write!(f, "\"")
    }
}
