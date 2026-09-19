//! Writing a classfile.

use core::num::NonZero;

use crate::{
    Attribute, ClassMetadata, ClassfileHeader, ConstantPoolEntry, Error, FieldHeader, MethodHeader,
    util::BufMut as _,
};

pub(crate) mod phases {
    #[derive(Debug)]
    pub struct Header;

    #[derive(Debug)]
    pub struct Body;

    #[derive(Debug)]
    pub struct Attributes<W>
    where
        W: crate::BufMut,
    {
        pub(crate) count: u16,
        pub(crate) chunk: W::Chunk,
    }

    #[derive(Debug)]
    pub struct ConstantPool<W>
    where
        W: crate::BufMut,
    {
        pub(crate) count: u16,
        pub(crate) chunk: W::Chunk,
    }

    #[derive(Debug)]
    pub struct Metadata;

    #[derive(Debug)]
    pub struct Interfaces<W>
    where
        W: crate::BufMut,
    {
        pub(crate) count: u16,
        pub(crate) chunk: W::Chunk,
    }

    #[derive(Debug)]
    pub struct ExceptionTable<W>
    where
        W: crate::BufMut,
    {
        pub(crate) count: u16,
        pub(crate) chunk: W::Chunk,
    }

    #[derive(Debug)]
    pub struct Fields<W, Phase>
    where
        W: crate::BufMut,
    {
        pub(crate) count: u16,
        pub(crate) chunk: W::Chunk,
        pub(crate) phase: Phase,
    }

    #[derive(Debug)]
    pub struct Methods<W, Phase>
    where
        W: crate::BufMut,
    {
        pub(crate) count: u16,
        pub(crate) chunk: W::Chunk,
        pub(crate) phase: Phase,
    }
}

/// A classfile writer.
#[derive(Debug)]
#[must_use]
pub struct Writer<W, Phase> {
    buf: W,
    phase: Phase,
}

impl<W, Phase> Writer<W, Phase> {
    #[inline]
    fn transform<T>(self, phase: T) -> Writer<W, T> {
        Writer {
            buf: self.buf,
            phase,
        }
    }

    /// Peeks at the internal buffer.
    #[inline]
    pub fn buf(&self) -> &W {
        &self.buf
    }
}

impl<W> Writer<W, phases::Header> {
    /// Creates a new classfile writer.
    pub const fn new(buf: W) -> Self {
        Self {
            buf,
            phase: phases::Header,
        }
    }
}

impl<W> Writer<W, phases::Header>
where
    W: crate::BufMut,
{
    /// Writes the classfile header.
    pub fn set_header(
        mut self,
        header: &ClassfileHeader,
    ) -> Result<Writer<W, phases::ConstantPool<W>>, Error> {
        self.buf.write(header)?;
        let chunk = self
            .buf
            .reserve_chunk(size_of::<u16>())
            .ok_or(Error::UnexpectedEOF)?;
        // plus one on the write side
        Ok(self.transform(phases::ConstantPool { count: 0, chunk }))
    }
}

impl<W> Writer<W, phases::ConstantPool<W>>
where
    W: crate::BufMut,
{
    /// Writes a new constant entry.
    pub fn push(&mut self, entry: &ConstantPoolEntry<'_>) -> Result<(), Error> {
        self.buf.write(entry)?;
        self.phase.count += entry.space();
        Ok(())
    }

    /// Finishes up the constant pool phase.
    pub fn finish(mut self) -> Result<Writer<W, phases::Metadata>, Error> {
        self.buf
            .write_chunk(self.phase.chunk, |mut buf| buf.write(self.phase.count + 1))?;
        Ok(Writer {
            buf: self.buf,
            phase: phases::Metadata,
        })
    }
}

impl<W> Writer<W, phases::Metadata>
where
    W: crate::BufMut,
{
    /// Writes the class metadata.
    pub fn set_metadata(
        mut self,
        metadata: &ClassMetadata,
    ) -> Result<Writer<W, phases::Interfaces<W>>, Error> {
        self.buf.write(metadata)?;
        let chunk = self
            .buf
            .reserve_chunk(size_of::<u16>())
            .ok_or(Error::UnexpectedEOF)?;
        Ok(self.transform(phases::Interfaces { count: 0, chunk }))
    }
}

impl<W> Writer<W, phases::Interfaces<W>>
where
    W: crate::BufMut,
{
    /// Writes a new interface index.
    pub fn push(&mut self, index: NonZero<u16>) -> Result<(), Error> {
        self.buf.write(index)?;
        self.phase.count += 1;
        Ok(())
    }

    /// Finishes up the interfaces phase.
    pub fn finish(mut self) -> Result<Writer<W, phases::Fields<W, phases::Header>>, Error> {
        self.buf
            .write_chunk(self.phase.chunk, |mut buf| buf.write(self.phase.count))?;
        let chunk = self
            .buf
            .reserve_chunk(size_of::<u16>())
            .ok_or(Error::UnexpectedEOF)?;
        Ok(Writer {
            buf: self.buf,
            phase: phases::Fields {
                count: 0,
                chunk,
                phase: phases::Header,
            },
        })
    }
}

impl<W> Writer<W, phases::Fields<W, phases::Header>>
where
    W: crate::BufMut,
{
    /// Writes a new field.
    pub fn set_header(
        mut self,
        header: &FieldHeader,
    ) -> Result<Writer<W, phases::Fields<W, phases::Attributes<W>>>, Error> {
        self.buf.write(header)?;
        let chunk = self
            .buf
            .reserve_chunk(size_of::<u16>())
            .ok_or(Error::UnexpectedEOF)?;
        Ok(Writer {
            buf: self.buf,
            phase: phases::Fields {
                phase: phases::Attributes { count: 0, chunk },
                count: self.phase.count,
                chunk: self.phase.chunk,
            },
        })
    }

    /// Finishes up the fields phase.
    pub fn finish(mut self) -> Result<Writer<W, phases::Methods<W, phases::Header>>, Error> {
        self.buf
            .write_chunk(self.phase.chunk, |mut buf| buf.write(self.phase.count))?;
        let chunk = self
            .buf
            .reserve_chunk(size_of::<u16>())
            .ok_or(Error::UnexpectedEOF)?;
        Ok(Writer {
            buf: self.buf,
            phase: phases::Methods {
                count: 0,
                chunk,
                phase: phases::Header,
            },
        })
    }
}

impl<W> Writer<W, phases::Fields<W, phases::Attributes<W>>>
where
    W: crate::BufMut,
{
    /// Writes a new attribute for the field.
    pub fn push(&mut self, attr: &Attribute<'_>) -> Result<(), Error> {
        self.buf.write(attr)?;
        self.phase.phase.count += 1;
        Ok(())
    }

    /// Finishes up current field, ready to write next or quit current phase.
    pub fn finish(mut self) -> Result<Writer<W, phases::Fields<W, phases::Header>>, Error> {
        self.buf.write_chunk(self.phase.phase.chunk, |mut buf| {
            buf.write(self.phase.phase.count)
        })?;
        self.phase.count += 1;
        Ok(Writer {
            buf: self.buf,
            phase: phases::Fields {
                count: self.phase.count,
                chunk: self.phase.chunk,
                phase: phases::Header,
            },
        })
    }
}

impl<W> Writer<W, phases::Methods<W, phases::Header>>
where
    W: crate::BufMut,
{
    /// Writes a new method.
    pub fn set_header(
        mut self,
        header: &MethodHeader,
    ) -> Result<Writer<W, phases::Methods<W, phases::Attributes<W>>>, Error> {
        self.buf.write(header)?;
        let chunk = self
            .buf
            .reserve_chunk(size_of::<u16>())
            .ok_or(Error::UnexpectedEOF)?;
        Ok(Writer {
            buf: self.buf,
            phase: phases::Methods {
                phase: phases::Attributes { count: 0, chunk },
                count: self.phase.count,
                chunk: self.phase.chunk,
            },
        })
    }

    /// Finishes up the methods phase.
    pub fn finish(mut self) -> Result<Writer<W, phases::Attributes<W>>, Error> {
        self.buf
            .write_chunk(self.phase.chunk, |mut buf| buf.write(self.phase.count))?;
        let chunk = self
            .buf
            .reserve_chunk(size_of::<u16>())
            .ok_or(Error::UnexpectedEOF)?;
        Ok(Writer {
            buf: self.buf,
            phase: phases::Attributes { count: 0, chunk },
        })
    }
}

impl<W> Writer<W, phases::Methods<W, phases::Attributes<W>>>
where
    W: crate::BufMut,
{
    /// Writes a new attribute for the method.
    pub fn push(&mut self, attr: &Attribute<'_>) -> Result<(), Error> {
        self.buf.write(attr)?;
        self.phase.phase.count += 1;
        Ok(())
    }

    /// Finishes up current method, ready to write next or quit current phase.
    pub fn finish(mut self) -> Result<Writer<W, phases::Methods<W, phases::Header>>, Error> {
        self.buf.write_chunk(self.phase.phase.chunk, |mut buf| {
            buf.write(self.phase.phase.count)
        })?;
        self.phase.count += 1;
        Ok(Writer {
            buf: self.buf,
            phase: phases::Methods {
                count: self.phase.count,
                chunk: self.phase.chunk,
                phase: phases::Header,
            },
        })
    }
}

impl<W> Writer<W, phases::Attributes<W>>
where
    W: crate::BufMut,
{
    /// Writes a new attribute.
    pub fn push(&mut self, attr: &Attribute<'_>) -> Result<(), Error> {
        self.buf.write(attr)?;
        self.phase.count += 1;
        Ok(())
    }

    /// Finishes up the attribute phase, writing terminated.
    pub fn finish(mut self) -> Result<W, Error> {
        self.buf
            .write_chunk(self.phase.chunk, |mut buf| buf.write(self.phase.count))?;
        Ok(self.buf)
    }
}
