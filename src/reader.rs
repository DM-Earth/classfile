//! Reading a classfile.

use core::{marker::PhantomData, num::NonZero};

use crate::{
    Attribute, ClassMetadata, ClassfileHeader, ConstantPoolEntry, Error, FieldHeader, MethodHeader,
    reader::phases::RawList,
};

pub(crate) mod phases {
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
    pub struct Body;

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
    #[derive(Debug)]
    pub struct ExceptionTable(pub(crate) RawList);

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
pub struct Reader<'a, R, Phase> {
    haystack: R,
    phase: Phase,
    _ghost: PhantomData<&'a ()>,
}

impl<'a, R, Phase> Reader<'a, R, Phase> {
    #[inline]
    fn transform<T>(self, phase: T) -> Reader<'a, R, T> {
        Reader {
            haystack: self.haystack,
            phase,
            _ghost: PhantomData,
        }
    }

    /// Peeks at the internal haystack.
    #[inline]
    pub fn haystack(&self) -> &R {
        &self.haystack
    }
}

impl<R> Reader<'_, R, phases::Header> {
    /// Creates a new classfile reader from given raw bytecode.
    pub const fn new(src: R) -> Self {
        Self {
            haystack: src,
            phase: phases::Header,
            _ghost: PhantomData,
        }
    }
}

impl<'a, R> Reader<'a, R, phases::Header>
where
    R: crate::Buf<'a>,
{
    /// Reads the header of the classfile.
    pub fn header(
        mut self,
    ) -> Result<(ClassfileHeader, Reader<'a, R, phases::ConstantPool>), Error> {
        let header = self.haystack.read()?;
        let phase = self.haystack.read()?;
        Ok((header, self.transform(phase)))
    }
}

impl<'a, R> Iterator for Reader<'a, R, phases::ConstantPool>
where
    R: crate::Buf<'a>,
{
    /// Index and entry.
    /// The indices of constant pool is fundamentally dumb so it's provided here.
    type Item = Result<(NonZero<u16>, ConstantPoolEntry<'a>), Error>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.phase.len > self.phase.read {
            let idx = self.phase.idx;
            let entry: ConstantPoolEntry<'a> = match self.haystack.read() {
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

impl<'a, R> Reader<'a, R, phases::ConstantPool>
where
    R: crate::Buf<'a>,
{
    /// Finishes the constant pool phase.
    /// Unread entries will be walked over.
    pub fn finish(mut self) -> Result<Reader<'a, R, phases::Metadata>, Error> {
        while self.next().transpose()?.is_some() {}
        Ok(self.transform(phases::Metadata))
    }
}

impl<'a, R> Reader<'a, R, phases::Metadata>
where
    R: crate::Buf<'a>,
{
    /// Reads class metadata of this classfile.
    pub fn metadata(mut self) -> Result<(ClassMetadata, Reader<'a, R, phases::Interfaces>), Error> {
        let meta = self.haystack.read()?;
        let list = self.haystack.read()?;
        Ok((meta, self.transform(phases::Interfaces(list))))
    }
}

impl<'a, R> Iterator for Reader<'a, R, phases::Interfaces>
where
    R: crate::Buf<'a>,
{
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

impl<'a, R> ExactSizeIterator for Reader<'a, R, phases::Interfaces> where R: crate::Buf<'a> {}

impl<'a, R> Reader<'a, R, phases::Interfaces>
where
    R: crate::Buf<'a>,
{
    /// Finishes the interfaces phase.
    /// Unread entries will be walked over.
    pub fn finish(mut self) -> Result<Reader<'a, R, phases::Fields>, Error> {
        while self.next().transpose()?.is_some() {}
        let list = self.haystack.read()?;
        Ok(self.transform(phases::Fields(list)))
    }
}

/// Reader of a field.
#[derive(Debug)]
pub struct FieldReader<'a, 'env, R>
where
    R: crate::Buf<'a>,
{
    header: FieldHeader,
    haystack: &'env mut R,
    attrs: RawList,
    _ghost: PhantomData<&'a ()>,
}

impl<'a, R> FieldReader<'a, '_, R>
where
    R: crate::Buf<'a>,
{
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

impl<'a, R> Iterator for FieldReader<'a, '_, R>
where
    R: crate::Buf<'a>,
{
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

impl<'a, R> ExactSizeIterator for FieldReader<'a, '_, R> where R: crate::Buf<'a> {}

impl<'a, R> Drop for FieldReader<'a, '_, R>
where
    R: crate::Buf<'a>,
{
    fn drop(&mut self) {
        self.fold((), |_, err| {
            let _ = err.expect("error occurred when skipping fields");
        })
    }
}

impl<'a, R> Reader<'a, R, phases::Fields>
where
    R: crate::Buf<'a>,
{
    /// Pulls for next field.
    pub fn next<'env>(&'env mut self) -> Option<Result<FieldReader<'a, 'env, R>, Error>> {
        (self.phase.0.len > self.phase.0.read).then(|| {
            self.phase.0.read += 1;
            Ok(FieldReader {
                header: self.haystack.read()?,
                attrs: self.haystack.read()?,
                haystack: &mut self.haystack,
                _ghost: PhantomData,
            })
        })
    }

    /// Finishes the fields phase.
    /// Unread entries will be walked over.
    pub fn finish(mut self) -> Result<Reader<'a, R, phases::Methods>, Error> {
        while self.next().transpose()?.is_some() {}
        let list = self.haystack.read()?;
        Ok(self.transform(phases::Methods(list)))
    }
}

/// Reader of a method.
#[derive(Debug)]
pub struct MethodReader<'a, 'env, R>
where
    R: crate::Buf<'a>,
{
    header: MethodHeader,
    haystack: &'env mut R,
    attrs: RawList,
    _ghost: PhantomData<&'a ()>,
}

impl<'a, R> MethodReader<'a, '_, R>
where
    R: crate::Buf<'a>,
{
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

impl<'a, R> Iterator for MethodReader<'a, '_, R>
where
    R: crate::Buf<'a>,
{
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

impl<'a, R> ExactSizeIterator for MethodReader<'a, '_, R> where R: crate::Buf<'a> {}

impl<'a, R> Drop for MethodReader<'a, '_, R>
where
    R: crate::Buf<'a>,
{
    fn drop(&mut self) {
        self.fold((), |_, err| {
            let _ = err.expect("error occurred when skipping methods");
        })
    }
}

impl<'a, R> Reader<'a, R, phases::Methods>
where
    R: crate::Buf<'a>,
{
    /// Pulls for next method.
    pub fn next<'env>(&'env mut self) -> Option<Result<MethodReader<'a, 'env, R>, Error>> {
        (self.phase.0.len > self.phase.0.read).then(|| {
            self.phase.0.read += 1;
            Ok(MethodReader {
                header: self.haystack.read()?,
                attrs: self.haystack.read()?,
                haystack: &mut self.haystack,
                _ghost: PhantomData,
            })
        })
    }

    /// Finishes the methods phase.
    /// Unread entries will be walked over.
    pub fn finish(mut self) -> Result<Reader<'a, R, phases::Attributes>, Error> {
        while self.next().transpose()?.is_some() {}
        let list = self.haystack.read()?;
        Ok(self.transform(phases::Attributes(list)))
    }
}

impl<'a, R> Iterator for Reader<'a, R, phases::Attributes>
where
    R: crate::Buf<'a>,
{
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

impl<'a, R> ExactSizeIterator for Reader<'a, R, phases::Attributes> where R: crate::Buf<'a> {}
