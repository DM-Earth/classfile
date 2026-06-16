//! Reading a classfile.

use core::num::NonZero;

use crate::{
    Attribute, ClassMetadata, ClassfileHeader, ConstantEntry, Error, FieldHeader, MethodHeader,
    reader::phases::RawList, util::Buf as _,
};

mod phases {
    use core::num::NonZero;

    use crate::{
        Error,
        util::{Buf, Decode},
    };

    #[derive(Debug)]
    pub(crate) struct RawList {
        pub(crate) len: u16,
        pub(crate) read: u16,
    }

    impl RawList {
        pub(crate) fn size_hint(&self) -> (usize, Option<usize>) {
            let len = (self.len - self.read) as usize;
            (len, Some(len))
        }
    }

    #[derive(Debug)]
    pub struct Header;

    #[derive(Debug)]
    pub struct ConstantPool {
        pub(crate) len: NonZero<u16>,
        pub(crate) read: NonZero<u16>,
        pub(crate) idx: NonZero<u16>,
    }

    #[derive(Debug)]
    pub struct Metadata;

    #[derive(Debug)]
    pub struct Interfaces(pub(crate) RawList);
    #[derive(Debug)]
    pub struct Fields(pub(crate) RawList);
    #[derive(Debug)]
    pub struct Methods(pub(crate) RawList);
    #[derive(Debug)]
    pub struct Attributes(pub(crate) RawList);

    impl<'de> Decode<'de> for RawList {
        fn decode<B: Buf<'de>>(mut buf: B) -> Result<Self, Error> {
            Ok(Self {
                len: buf.read()?,
                read: 0,
            })
        }
    }

    impl<'de> Decode<'de> for ConstantPool {
        fn decode<B: Buf<'de>>(mut buf: B) -> Result<Self, Error> {
            Ok(Self {
                len: buf.read()?,
                read: const { NonZero::new(1).unwrap() },
                idx: const { NonZero::new(1).unwrap() },
            })
        }
    }
}

/// Reader of a classfile.
#[derive(Debug)]
#[must_use]
pub struct Reader<'a, Phase> {
    haystack: &'a [u8],
    phase: Phase,
}

impl<'a, Phase> Reader<'a, Phase> {
    #[inline]
    fn transform<T>(self, phase: T) -> Reader<'a, T> {
        Reader {
            haystack: self.haystack,
            phase,
        }
    }
}

impl<'a> Reader<'a, phases::Header> {
    /// Creates a new classfile reader from given raw bytecode.
    pub const fn new(src: &'a [u8]) -> Self {
        Self {
            haystack: src,
            phase: phases::Header,
        }
    }

    /// Reads the header of the classfile.
    pub fn header(mut self) -> Result<(ClassfileHeader, Reader<'a, phases::ConstantPool>), Error> {
        let header = self.haystack.read()?;
        let phase = self.haystack.read()?;
        Ok((header, self.transform(phase)))
    }
}

impl<'a> Iterator for Reader<'a, phases::ConstantPool> {
    /// Index and entry.
    /// The indices of constant pool is fundamentally dumb so it's provided here.
    type Item = Result<(NonZero<u16>, ConstantEntry<'a>), Error>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.phase.len > self.phase.read {
            let idx = self.phase.idx;
            let entry: ConstantEntry<'a> = match self.haystack.read() {
                Ok(entry) => entry,
                Err(err) => return Some(Err(err)),
            };
            self.phase.read = self.phase.read.saturating_add(1);
            self.phase.idx = self.phase.idx.saturating_add(entry.space());
            Some(Ok((idx, entry)))
        } else {
            None
        }
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        (
            (self.phase.len.get() - self.phase.read.get()) as usize,
            None,
        )
    }
}

impl<'a> Reader<'a, phases::ConstantPool> {
    /// Finishes the constant pool phase.
    /// Unread entries will be walked over.
    pub fn finish(mut self) -> Result<Reader<'a, phases::Metadata>, Error> {
        while self.next().transpose()?.is_some() {}
        Ok(self.transform(phases::Metadata))
    }
}

impl<'a> Reader<'a, phases::Metadata> {
    /// Reads class metadata of this classfile.
    pub fn metadata(mut self) -> Result<(ClassMetadata, Reader<'a, phases::Interfaces>), Error> {
        let meta = self.haystack.read()?;
        let list = self.haystack.read()?;
        Ok((meta, self.transform(phases::Interfaces(list))))
    }
}

impl Iterator for Reader<'_, phases::Interfaces> {
    /// The constant pool entry of this interface (`Class`).
    type Item = Result<NonZero<u16>, Error>;

    fn next(&mut self) -> Option<Self::Item> {
        (self.phase.0.len > self.phase.0.read).then(|| {
            self.phase.0.read += 1;
            self.haystack.read()
        })
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        self.phase.0.size_hint()
    }
}

impl ExactSizeIterator for Reader<'_, phases::Interfaces> {}

impl<'a> Reader<'a, phases::Interfaces> {
    /// Finishes the interfaces phase.
    /// Unread entries will be walked over.
    pub fn finish(mut self) -> Result<Reader<'a, phases::Fields>, Error> {
        while self.next().transpose()?.is_some() {}
        let list = self.haystack.read()?;
        Ok(self.transform(phases::Fields(list)))
    }
}

/// Reader of a field.
#[derive(Debug)]
pub struct FieldReader<'a, 'env> {
    header: FieldHeader,
    haystack: &'env mut &'a [u8],
    attrs: RawList,
}

impl FieldReader<'_, '_> {
    /// Returns the header of this field.
    #[inline]
    pub fn header(&self) -> &FieldHeader {
        &self.header
    }

    /// Finishes up this field read.
    pub fn finish(mut self) -> Result<(), Error> {
        while self.next().transpose()?.is_some() {}
        Ok(())
    }
}

impl<'a> Iterator for FieldReader<'a, '_> {
    type Item = Result<Attribute<'a>, Error>;

    fn next(&mut self) -> Option<Self::Item> {
        (self.attrs.len > self.attrs.read).then(|| {
            self.attrs.read += 1;
            self.haystack.read()
        })
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        self.attrs.size_hint()
    }
}

impl ExactSizeIterator for FieldReader<'_, '_> {}

impl Drop for FieldReader<'_, '_> {
    fn drop(&mut self) {
        self.fold((), |_, err| {
            let _ = err.expect("error occurred when skipping fields");
        })
    }
}

impl<'a> Reader<'a, phases::Fields> {
    /// Pulls for next field.
    pub fn next<'env>(&'env mut self) -> Option<Result<FieldReader<'a, 'env>, Error>> {
        (self.phase.0.len > self.phase.0.read).then(|| {
            self.phase.0.read += 1;
            Ok(FieldReader {
                header: self.haystack.read()?,
                attrs: self.haystack.read()?,
                haystack: &mut self.haystack,
            })
        })
    }

    /// Finishes the fields phase.
    /// Unread entries will be walked over.
    pub fn finish(mut self) -> Result<Reader<'a, phases::Methods>, Error> {
        while self.next().transpose()?.is_some() {}
        let list = self.haystack.read()?;
        Ok(self.transform(phases::Methods(list)))
    }
}
/// Reader of a method.
#[derive(Debug)]
pub struct MethodReader<'a, 'env> {
    header: MethodHeader,
    haystack: &'env mut &'a [u8],
    attrs: RawList,
}

impl MethodReader<'_, '_> {
    /// Returns the header of this method.
    #[inline]
    pub fn header(&self) -> &MethodHeader {
        &self.header
    }

    /// Finishes up this method read.
    pub fn finish(mut self) -> Result<(), Error> {
        while self.next().transpose()?.is_some() {}
        Ok(())
    }
}

impl<'a> Iterator for MethodReader<'a, '_> {
    type Item = Result<Attribute<'a>, Error>;

    fn next(&mut self) -> Option<Self::Item> {
        (self.attrs.len > self.attrs.read).then(|| {
            self.attrs.read += 1;
            self.haystack.read()
        })
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        self.attrs.size_hint()
    }
}

impl ExactSizeIterator for MethodReader<'_, '_> {}

impl Drop for MethodReader<'_, '_> {
    fn drop(&mut self) {
        self.fold((), |_, err| {
            let _ = err.expect("error occurred when skipping methods");
        })
    }
}

impl<'a> Reader<'a, phases::Methods> {
    /// Pulls for next method.
    pub fn next<'env>(&'env mut self) -> Option<Result<MethodReader<'a, 'env>, Error>> {
        (self.phase.0.len > self.phase.0.read).then(|| {
            self.phase.0.read += 1;
            Ok(MethodReader {
                header: self.haystack.read()?,
                attrs: self.haystack.read()?,
                haystack: &mut self.haystack,
            })
        })
    }

    /// Finishes the methods phase.
    /// Unread entries will be walked over.
    pub fn finish(mut self) -> Result<Reader<'a, phases::Attributes>, Error> {
        while self.next().transpose()?.is_some() {}
        let list = self.haystack.read()?;
        Ok(self.transform(phases::Attributes(list)))
    }
}

impl<'a> Iterator for Reader<'a, phases::Attributes> {
    /// The constant pool entry of this interface (`Class`).
    type Item = Result<Attribute<'a>, Error>;

    fn next(&mut self) -> Option<Self::Item> {
        (self.phase.0.len > self.phase.0.read).then(|| {
            self.phase.0.read += 1;
            self.haystack.read()
        })
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        self.phase.0.size_hint()
    }
}

impl ExactSizeIterator for Reader<'_, phases::Attributes> {}
