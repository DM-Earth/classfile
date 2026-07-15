use core::{marker::PhantomData, num::NonZero, ops::Range};

use crate::{
    Attribute, Buf, BufMut, Error,
    reader::{self, phases::RawList},
    util::{BufMut as _, Decode, Encode},
    writer,
};

use bitflags::bitflags;

macro_rules! simple_rw {
    () => {
        /// Writes this attribute into given buffer.
        #[inline]
        pub fn write<B: crate::BufMut>(&self, mut buf: B) -> Result<(), Error> {
            buf.write(self)
        }

        /// Reads the attribute from given buffer.
        #[inline]
        pub fn read<'a, B: crate::Buf<'a>>(mut buf: B) -> Result<Self, Error> {
            buf.read()
        }
    };
}

/// Value of a constant expression.
#[derive(Debug, PartialEq, Eq, PartialOrd, Ord, Clone, Copy, Hash)]
pub struct ConstantValue {
    /// Index to `Utf8` constant pool for the actual constant value.
    pub constant_value_index: NonZero<u16>,
}

/// Name of [`ConstantValue`] attribute.
pub const NAME_CONSTANT_VALUE: &str = "ConstantValue";

impl ConstantValue {
    simple_rw! {}
}

impl<'de> Decode<'de> for ConstantValue {
    fn decode<B: crate::util::Buf<'de>>(mut buf: B) -> Result<Self, Error> {
        Ok(Self {
            constant_value_index: buf.read()?,
        })
    }
}

impl Encode for ConstantValue {
    fn encode<B: crate::util::BufMut>(&self, mut buf: B) -> Result<(), Error> {
        buf.write(self.constant_value_index)
    }
}

/// Name of `Code` attribute. See [`CodeReader`] and [`CodeWriter`] for usage.
pub const NAME_CODE: &str = "Code";

/// Body part of `Code` attribute, which contains JVM instructions with
/// auxiliary information about the method.
#[derive(Debug)]
pub struct CodeBody<'a> {
    /// The maximum depth of the operand stack of this method.
    pub max_stack: u16,
    /// The number of local variables in the local variable array allocated
    /// upon invocation of this method.
    pub max_locals: u16,
    /// The number of bytes in the code array for this method.
    pub code: &'a [u8],
}

impl<'a> Decode<'a> for CodeBody<'a> {
    fn decode<B: crate::util::Buf<'a>>(mut buf: B) -> Result<Self, Error> {
        Ok(Self {
            max_stack: buf.read()?,
            max_locals: buf.read()?,
            code: buf.read()?,
        })
    }
}

impl Encode for CodeBody<'_> {
    fn encode<B: crate::util::BufMut>(&self, mut buf: B) -> Result<(), Error> {
        buf.write(self.max_stack)?;
        buf.write(self.max_locals)?;
        buf.write(self.code)?;
        Ok(())
    }
}

/// Reader of `Code` attribute.
#[derive(Debug)]
pub struct CodeReader<'a, B, Phase> {
    buf: B,
    phase: Phase,
    _ghost: PhantomData<&'a ()>,
}

impl<'a, B, Phase> CodeReader<'a, B, Phase> {
    #[inline]
    fn transform<T>(self, phase: T) -> CodeReader<'a, B, T> {
        CodeReader {
            buf: self.buf,
            phase,
            _ghost: PhantomData,
        }
    }
}

impl<B> CodeReader<'_, B, reader::phases::Body> {
    /// Creates a new `Code` reader.
    pub fn new(buf: B) -> Self {
        Self {
            buf,
            phase: reader::phases::Body,
            _ghost: PhantomData,
        }
    }
}

impl<'a, B> CodeReader<'a, B, reader::phases::Body>
where
    B: Buf<'a>,
{
    /// Reads the body and jumps to next phase.
    pub fn body(
        mut self,
    ) -> Result<
        (
            CodeBody<'a>,
            CodeReader<'a, B, reader::phases::ExceptionTable>,
        ),
        Error,
    > {
        let body = self.buf.read()?;
        let list = self.buf.read()?;
        Ok((body, self.transform(reader::phases::ExceptionTable(list))))
    }
}

impl<'a, B> Iterator for CodeReader<'a, B, reader::phases::ExceptionTable>
where
    B: Buf<'a>,
{
    type Item = Result<ExceptionTableEntry, Error>;

    fn next(&mut self) -> Option<Self::Item> {
        (self.phase.0.len > self.phase.0.read).then(|| {
            self.phase.0.read += 1;
            self.buf.read()
        })
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        self.phase.0.size_hint()
    }
}

impl<'a, B> ExactSizeIterator for CodeReader<'a, B, reader::phases::ExceptionTable> where B: Buf<'a> {}

impl<'a, B> CodeReader<'a, B, reader::phases::ExceptionTable>
where
    B: Buf<'a>,
{
    /// Finishes this phase.
    pub fn finish(mut self) -> Result<CodeReader<'a, B, reader::phases::Attributes>, Error> {
        while self.next().transpose()?.is_some() {}
        let list = self.buf.read()?;
        Ok(self.transform(reader::phases::Attributes(list)))
    }
}

impl<'a, B> Iterator for CodeReader<'a, B, reader::phases::Attributes>
where
    B: Buf<'a>,
{
    type Item = Result<Attribute<'a>, Error>;

    fn next(&mut self) -> Option<Self::Item> {
        (self.phase.0.len > self.phase.0.read).then(|| {
            self.phase.0.read += 1;
            self.buf.read()
        })
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        self.phase.0.size_hint()
    }
}

impl<'a, B> ExactSizeIterator for CodeReader<'a, B, reader::phases::Attributes> where B: Buf<'a> {}

impl<'a, B> CodeReader<'a, B, reader::phases::Attributes>
where
    B: Buf<'a>,
{
    /// Finishes this phase.
    pub fn finish(mut self) -> Result<(), Error> {
        while self.next().transpose()?.is_some() {}
        Ok(())
    }
}

/// Writer of `Code` attribute.
#[derive(Debug)]
pub struct CodeWriter<B, Phase> {
    buf: B,
    phase: Phase,
}

impl<B, Phase> CodeWriter<B, Phase> {
    #[inline]
    fn transform<T>(self, phase: T) -> CodeWriter<B, T> {
        CodeWriter {
            buf: self.buf,
            phase,
        }
    }
}

impl<B> CodeWriter<B, writer::phases::Body> {
    /// Creates a new `Code` writer.
    pub fn new(buf: B) -> Self {
        Self {
            buf,
            phase: writer::phases::Body,
        }
    }
}

impl<B> CodeWriter<B, writer::phases::Body>
where
    B: BufMut,
{
    /// Writes body of the code and jumps to next phase.
    pub fn set_body(
        mut self,
        body: &CodeBody<'_>,
    ) -> Result<CodeWriter<B, writer::phases::ExceptionTable<B>>, Error> {
        self.buf.write(body)?;
        let chunk = self
            .buf
            .reserve_chunk(size_of::<u16>())
            .ok_or(Error::UnexpectedEOF)?;
        Ok(self.transform(writer::phases::ExceptionTable { count: 0, chunk }))
    }
}

impl<B> CodeWriter<B, writer::phases::ExceptionTable<B>>
where
    B: BufMut,
{
    /// Writes an entry to the buffer.
    pub fn push(&mut self, entry: &ExceptionTableEntry) -> Result<(), Error> {
        self.buf.write(entry)?;
        self.phase.count += 1;
        Ok(())
    }

    /// Finishes this phase.
    pub fn finish(mut self) -> Result<CodeWriter<B, writer::phases::Attributes<B>>, Error> {
        self.buf
            .write_chunk(self.phase.chunk, |mut buf| buf.write(self.phase.count))?;
        let chunk = self
            .buf
            .reserve_chunk(size_of::<u16>())
            .ok_or(Error::UnexpectedEOF)?;
        Ok(CodeWriter {
            buf: self.buf,
            phase: writer::phases::Attributes { count: 0, chunk },
        })
    }
}

impl<B> CodeWriter<B, writer::phases::Attributes<B>>
where
    B: BufMut,
{
    /// Writes an entry to the buffer.
    pub fn push(&mut self, entry: &Attribute<'_>) -> Result<(), Error> {
        self.buf.write(entry)?;
        self.phase.count += 1;
        Ok(())
    }

    /// Finishes this phase.
    pub fn finish(mut self) -> Result<(), Error> {
        self.buf
            .write_chunk(self.phase.chunk, |mut buf| buf.write(self.phase.count))
    }
}

/// Entry of an exception table.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ExceptionTableEntry {
    /// The ranges in the `code` array at which the exception handler is active.
    pub range_pc: Range<u16>,
    /// The start of the exception handler.
    pub handler_pc: u16,
    /// The `Class` of exceptions that this exception handler is designated
    /// to catch, if present.
    pub catch_type: Option<NonZero<u16>>,
}

impl<'a> Decode<'a> for ExceptionTableEntry {
    fn decode<B: crate::util::Buf<'a>>(mut buf: B) -> Result<Self, Error> {
        Ok(Self {
            range_pc: buf.read()?,
            handler_pc: buf.read()?,
            catch_type: buf.read()?,
        })
    }
}

impl Encode for ExceptionTableEntry {
    fn encode<B: crate::util::BufMut>(&self, mut buf: B) -> Result<(), Error> {
        buf.write(&self.range_pc)?;
        buf.write(self.handler_pc)?;
        buf.write(self.catch_type)?;
        Ok(())
    }
}

macro_rules! array_rw {
    ($r:ident,$w:ident=>$n:ty) => {
        /// Reader of an array.
        #[derive(Debug)]
        pub struct $r<'a, B, T> {
            buf: B,
            list: RawList,
            _ghost: PhantomData<(&'a (), &'a T)>,
        }

        impl<'a, B, T> $r<'a, B, T>
        where
            B: Buf<'a>,
        {
            /// Creates a new reader.
            pub fn new(mut buf: B) -> Result<Self, Error> {
                Ok(Self {
                    list: buf.read()?,
                    buf,
                    _ghost: PhantomData,
                })
            }
        }

        impl<'a, B, T> Iterator for $r<'a, B, T>
        where
            B: Buf<'a>,
            T: Decode<'a>,
        {
            type Item = Result<T, Error>;

            fn next(&mut self) -> Option<Self::Item> {
                (self.list.len > self.list.read).then(|| {
                    self.list.read += 1;
                    self.buf.read()
                })
            }

            fn size_hint(&self) -> (usize, Option<usize>) {
                self.list.size_hint()
            }
        }

        impl<'a, B, T> ExactSizeIterator for $r<'a, B, T>
        where
            B: Buf<'a>,
            T: Decode<'a>,
        {
        }

        /// Writer of an array.
        #[derive(Debug)]
        pub struct $w<B, T>
        where
            B: BufMut,
        {
            buf: B,
            chunk: Option<B::Chunk>,
            count: $n,
            _ghost: PhantomData<T>,
        }

        impl<B, T> $w<B, T>
        where
            B: BufMut,
        {
            /// Creates a new writer.
            pub fn new(mut buf: B) -> Result<Self, Error> {
                let chunk = buf
                    .reserve_chunk(size_of::<$n>())
                    .ok_or(Error::UnexpectedEOF)?;
                Ok(Self {
                    buf,
                    chunk: Some(chunk),
                    count: 0,
                    _ghost: PhantomData,
                })
            }
        }

        impl<B, T> $w<B, T>
        where
            B: BufMut,
            T: Encode,
        {
            /// Writes a new entry.
            pub fn push(&mut self, entry: &T) -> Result<(), Error> {
                self.buf.write(entry)?;
                self.count += 1;
                Ok(())
            }
        }

        impl<B, T> $w<B, T>
        where
            B: BufMut,
        {
            /// Finishes writing the array.
            #[allow(clippy::missing_panics_doc)]
            pub fn finish(mut self) -> Result<(), Error> {
                let chunk = self.chunk.take().unwrap();
                self.buf.write_chunk(chunk, |mut b| b.write(self.count))
            }
        }

        impl<B, T> Drop for $w<B, T>
        where
            B: BufMut,
        {
            fn drop(&mut self) {
                if let Some(chunk) = self.chunk.take() {
                    let _ = self.buf.write_chunk(chunk, |mut b| b.write(self.count));
                }
            }
        }
    };
}

array_rw!(ArrayReaderU8, ArrayWriterU8 => u8);
array_rw!(ArrayReaderU16, ArrayWriterU16 => u16);

/// Shorthand for [`ArrayReaderU16`].
pub type ArrayReader<'a, B, T> = ArrayReaderU16<'a, B, T>;
/// Shorthand for [`ArrayWriterU16`].
pub type ArrayWriter<B, T> = ArrayWriterU16<B, T>;

/// Name of `Exceptions` attribute. See [`ExceptionsReader`] and [`ExceptionsWriter`] for usage.
pub const NAME_EXCEPTIONS: &str = "Exceptions";
/// Reader of `Exceptions` attribute, where the entry is index to constant pool `Class` item.
pub type ExceptionsReader<'a, B> = ArrayReader<'a, B, NonZero<u16>>;
/// Writer of `Exceptions` attribute, where the entry is index to constant pool `Class` item.
pub type ExceptionsWriter<B> = ArrayWriter<B, NonZero<u16>>;

/// Name of `InnerClasses` attribute. See [`InnerClassesReader`] and [`InnerClassesWriter`] for usage.
pub const NAME_INNER_CLASSES: &str = "InnerClasses";
/// Reader of `InnerClasses` attribute.
pub type InnerClassesReader<'a, B> = ArrayReader<'a, B, InnerClass>;
/// Writer of `InnerClasses` attribute.
pub type InnerClassesWriter<B> = ArrayWriter<B, InnerClass>;

/// Represents an inner class in `InnerClasses` attribute.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct InnerClass {
    /// Index to constant pool (`Class`) representing the nested class.
    pub inner_info_idx: NonZero<u16>,
    /// Index to constant pool (`Class`) representing the parent class,
    /// if the inner class is nor a top-level class or interface or a local class
    /// or an anonymous class. Otherwise it should be `None`.
    pub outer_info_idx: Option<NonZero<u16>>,
    /// Index to constant pool (`Utf8`) represents the original simple name of nested class,
    /// if it's not anonymous.
    pub inner_name_idx: Option<NonZero<u16>>,
    /// Properties of the nested class.
    pub inner_access_flags: InnerClassAccessFlags,
}

bitflags! {
    /// Denote access permissions to and properties of a nested class or interface.
    #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
    pub struct InnerClassAccessFlags: u16 {
        /// Marked or implicitly `public` in source.
        const PUBLIC = 0x0001;
        /// Marked `private` in source.
        const PRIVATE = 0x0002;
        /// Marked `protected` in source.
        const PROTECTED = 0x0004;
        /// Marked or implicitly `static` in source.
        const STATIC = 0x0008;
        /// Marked or implicitly `final` in source.
        const FINAL = 0x0010;
        /// Was an `interface` in source.
        const INTERFACE = 0x0200;
        /// Marked or implicitly `abstract` in source.
        const ABSTRACT = 0x0400;
        /// Declared `synthetic`; not present in the source code.
        const SYNTHETIC = 0x1000;
        /// Declared as an annotation interface.
        const ANNOTATION = 0x2000;
        /// Declared as an enum class.
        const ENUM = 0x4000;
    }
}

impl<'de> Decode<'de> for InnerClass {
    fn decode<B: crate::util::Buf<'de>>(mut buf: B) -> Result<Self, Error> {
        Ok(Self {
            inner_info_idx: buf.read()?,
            outer_info_idx: buf.read()?,
            inner_name_idx: buf.read()?,
            inner_access_flags: buf.read()?,
        })
    }
}

impl Encode for InnerClass {
    fn encode<B: crate::util::BufMut>(&self, mut buf: B) -> Result<(), Error> {
        buf.write(self.inner_info_idx)?;
        buf.write(self.outer_info_idx)?;
        buf.write(self.inner_name_idx)?;
        buf.write(self.inner_access_flags)?;
        Ok(())
    }
}

impl<'de> Decode<'de> for InnerClassAccessFlags {
    fn decode<B: crate::util::Buf<'de>>(mut buf: B) -> Result<Self, Error> {
        let bits = buf.read()?;
        Ok(Self::from_bits_retain(bits))
    }
}

impl Encode for InnerClassAccessFlags {
    fn encode<B: crate::util::BufMut>(&self, mut buf: B) -> Result<(), Error> {
        buf.write(self.bits())
    }
}

/// Name of [`EnclosingMethod`] attribute.
pub const NAME_ENCLOSING_METHOD: &str = "EnclosingMethod";

/// Properties of a local class or an anonymous class.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct EnclosingMethod {
    /// The innermost class that encloses the declaration of the current class,
    /// as an index to constant pool (`Class`).
    pub class_idx: NonZero<u16>,
    /// `NameAndType` of a method in the class referenced by the `class_idx` field in constant pool,
    /// only if it's immediately enclosed by a method or constructor.
    pub method_idx: Option<NonZero<u16>>,
}

impl EnclosingMethod {
    simple_rw! {}
}

impl<'de> Decode<'de> for EnclosingMethod {
    fn decode<B: crate::util::Buf<'de>>(mut buf: B) -> Result<Self, Error> {
        Ok(Self {
            class_idx: buf.read()?,
            method_idx: buf.read()?,
        })
    }
}

impl Encode for EnclosingMethod {
    fn encode<B: crate::util::BufMut>(&self, mut buf: B) -> Result<(), Error> {
        buf.write(self.class_idx)?;
        buf.write(self.method_idx)?;
        Ok(())
    }
}

/// Name of `Synthetic` attribute. This attribute has no info.
pub const NAME_SYNTHETIC: &str = "Synthetic";

/// Index to an entry in constant pool.
#[derive(Debug, PartialEq, Eq, PartialOrd, Ord, Clone, Copy, Hash)]
pub struct ConstantPoolIndex(pub NonZero<u16>);

impl ConstantPoolIndex {
    simple_rw! {}
}

impl<'de> Decode<'de> for ConstantPoolIndex {
    #[inline]
    fn decode<B: crate::util::Buf<'de>>(mut buf: B) -> Result<Self, Error> {
        buf.read()
    }
}

impl Encode for ConstantPoolIndex {
    #[inline]
    fn encode<B: crate::util::BufMut>(&self, mut buf: B) -> Result<(), Error> {
        buf.write(self.0)
    }
}

/// Name of `Signature` attribute. Use [`ConstantPoolIndex`] for its index (`Utf8`).
pub const NAME_SIGNATURE: &str = "Signature";

/// Name of `SourceFile` attribute. Use [`ConstantPoolIndex`] for its index (`Utf8`).
pub const NAME_SOURCE_FILE: &str = "SourceFile";

/// Name of `SourceDebugExtension` attribute.
///
/// The `info` field itself in `Attribute` is the desired modified UTF-8 slice
/// of the extended debugging information.
pub const NAME_SOURCE_DEBUG_EXTENSION: &str = "SourceDebugExtension";

/// Name of `LineNumberTable` attribute. See [`LineNumberTableReader`] and [`LineNumberTableWriter`] for usage.
pub const NAME_LINE_NUMBER_TABLE: &str = "LineNumberTable";
/// Reader of `LineNumberTable` attribute.
pub type LineNumberTableReader<'a, B> = ArrayReader<'a, B, LineNumber>;
/// Writer of `LineNumberTable` attribute.
pub type LineNumberTableWriter<B> = ArrayWriter<B, LineNumber>;

/// The line number in the original source file changes at a given point in the `code` array.
#[derive(Debug, PartialEq, Eq, PartialOrd, Ord, Clone, Copy, Hash)]
pub struct LineNumber {
    /// Index into the `code` array of this `Code` attribute.
    ///
    /// The item indicates the index into the `code` array at which the
    /// code for a new line in the original source file begins.
    pub start_pc: u16,
    /// The corresponding line number in the original source file.
    pub line_number: u16,
}

impl<'de> Decode<'de> for LineNumber {
    fn decode<B: crate::util::Buf<'de>>(mut buf: B) -> Result<Self, Error> {
        Ok(Self {
            start_pc: buf.read()?,
            line_number: buf.read()?,
        })
    }
}

impl Encode for LineNumber {
    fn encode<B: crate::util::BufMut>(&self, mut buf: B) -> Result<(), Error> {
        buf.write(self.start_pc)?;
        buf.write(self.line_number)?;
        Ok(())
    }
}

/// Name of `LocalVariableTable` attribute. See [`LocalVariableTableReader`] and [`LocalVariableTableWriter`] for usage.
pub const NAME_LOCAL_VARIABLE_TABLE: &str = "LocalVariableTable";
/// Reader of `LocalVariableTable` attribute.
pub type LocalVariableTableReader<'a, B> = ArrayReader<'a, B, LocalVariable>;
/// Writer of `LocalVariableTable` attribute.
pub type LocalVariableTableWriter<B> = ArrayWriter<B, LocalVariable>;

/// A range of `code` array offsets within which a local variable has a value,
/// and indicates the index into the local variable array of the current frame
/// at which that local variable can be found.
#[derive(Debug, PartialEq, Eq, Clone, Hash)]
pub struct LocalVariable {
    /// The interval where the given local variable has a value at indices in the `code` array.
    pub indices: Range<u16>,
    /// The unqualified name denoting a local variable, as an index into constant pool (`Utf8`).
    pub name_idx: NonZero<u16>,
    /// Field descriptor which encodes the type of a local variable in the source program,
    /// as an index into constant pool (`Utf8`).
    pub desc_idx: NonZero<u16>,
    /// Index into the local variable array of the current frame.
    /// The given local variable is at `index` in the local variable array of the current frame.
    ///
    /// If the given local variable is of type `double` or `long`, it occupies both `index` and `index + 1`.
    pub index: u16,
}

impl<'de> Decode<'de> for LocalVariable {
    fn decode<B: crate::util::Buf<'de>>(mut buf: B) -> Result<Self, Error> {
        Ok(Self {
            indices: {
                let start: u16 = buf.read()?;
                let len: u16 = buf.read()?;
                start..(start + len)
            },
            name_idx: buf.read()?,
            desc_idx: buf.read()?,
            index: buf.read()?,
        })
    }
}

impl Encode for LocalVariable {
    fn encode<B: crate::util::BufMut>(&self, mut buf: B) -> Result<(), Error> {
        buf.write(self.indices.start)?;
        buf.write(self.indices.end - self.indices.start)?;
        buf.write(self.name_idx)?;
        buf.write(self.desc_idx)?;
        buf.write(self.index)?;
        Ok(())
    }
}

/// Name of `LocalVariableTypeTable` attribute. See [`LocalVariableTypeTableReader`] and [`LocalVariableTypeTableWriter`] for usage.
pub const NAME_LOCAL_VARIABLE_TYPE_TABLE: &str = "LocalVariableTypeTable";
/// Reader of `LocalVariableTypeTable` attribute.
pub type LocalVariableTypeTableReader<'a, B> = ArrayReader<'a, B, TypedLocalVariable>;
/// Writer of `LocalVariableTypeTable` attribute.
pub type LocalVariableTypeTableWriter<B> = ArrayWriter<B, TypedLocalVariable>;

/// [`LocalVariable`] but with signature instead of descriptor.
#[derive(Debug, PartialEq, Eq, Clone, Hash)]
pub struct TypedLocalVariable {
    /// The interval where the given local variable has a value at indices in the `code` array.
    pub indices: Range<u16>,
    /// The unqualified name denoting a local variable, as an index into constant pool (`Utf8`).
    pub name_idx: NonZero<u16>,
    /// Field signature which encodes the type of a local variable in the source program,
    /// as an index into constant pool (`Utf8`).
    pub signature_idx: NonZero<u16>,
    /// Index into the local variable array of the current frame.
    /// The given local variable is at `index` in the local variable array of the current frame.
    ///
    /// If the given local variable is of type `double` or `long`, it occupies both `index` and `index + 1`.
    pub index: u16,
}

impl<'de> Decode<'de> for TypedLocalVariable {
    fn decode<B: crate::util::Buf<'de>>(mut buf: B) -> Result<Self, Error> {
        Ok(Self {
            indices: {
                let start: u16 = buf.read()?;
                let len: u16 = buf.read()?;
                start..(start + len)
            },
            name_idx: buf.read()?,
            signature_idx: buf.read()?,
            index: buf.read()?,
        })
    }
}

impl Encode for TypedLocalVariable {
    fn encode<B: crate::util::BufMut>(&self, mut buf: B) -> Result<(), Error> {
        buf.write(self.indices.start)?;
        buf.write(self.indices.end - self.indices.start)?;
        buf.write(self.name_idx)?;
        buf.write(self.signature_idx)?;
        buf.write(self.index)?;
        Ok(())
    }
}

/// Name of `Deprecated` attribute. This attribute has no info.
pub const NAME_DEPRECATED: &str = "Deprecated";

/// Formal parameter of a method.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct MethodParam {
    /// Name of this parameter in constant pool (`Utf8`), if present.
    pub name_idx: Option<NonZero<u16>>,
    /// Access properties of a method parameter.
    pub access_flags: MethodParamAccessFlags,
}

bitflags! {
    /// Denote access properties of a method parameter.
    #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
    pub struct MethodParamAccessFlags: u16 {
        /// Indicates that the formal parameter was declared `final`.
        const FINAL = 0x0010;
        /// Indicates that the formal parameter was not explicitly or implicitly declared in source code,
        /// according to the specification of the language in which the source code was written.
        const SYNTHETIC = 0x1000;
        /// Indicates that the formal parameter was implicitly declared in source code,
        /// according to the specification of the language in which the source code was written.
        const MANDATED = 0x8000;
    }
}

impl<'de> Decode<'de> for MethodParamAccessFlags {
    fn decode<B: crate::util::Buf<'de>>(mut buf: B) -> Result<Self, Error> {
        let bits = buf.read()?;
        Ok(Self::from_bits_retain(bits))
    }
}

impl Encode for MethodParamAccessFlags {
    fn encode<B: crate::util::BufMut>(&self, mut buf: B) -> Result<(), Error> {
        buf.write(self.bits())
    }
}

impl<'de> Decode<'de> for MethodParam {
    fn decode<B: crate::util::Buf<'de>>(mut buf: B) -> Result<Self, Error> {
        Ok(Self {
            name_idx: buf.read()?,
            access_flags: buf.read()?,
        })
    }
}

impl Encode for MethodParam {
    fn encode<B: crate::util::BufMut>(&self, mut buf: B) -> Result<(), Error> {
        buf.write(self.name_idx)?;
        buf.write(self.access_flags)?;
        Ok(())
    }
}

/// Name of `MethodParameters` attribute. See [`MethodParamsReader`] and [`MethodParamsWriter`] for usage.
pub const NAME_METHOD_PARAMS: &str = "MethodParameters";
/// Reader of `MethodParameters` attribute.
pub type MethodParamsReader<'a, B> = ArrayReaderU8<'a, B, MethodParam>;
/// Writer of `MethodParameters` attribute.
pub type MethodParamsWriter<B> = ArrayWriterU8<B, MethodParam>;

/// Name of `ModulePackages` attribute.
/// See [`ModulePackagesReader`] and [`ModulePackagesWriter`] for usage.
pub const NAME_MODULE_PACKAGES: &str = "ModulePackages";
/// Reader of `ModulePackages` attribute.
pub type ModulePackagesReader<'a, B> = ArrayReader<'a, B, NonZero<u16>>;
/// Writer of `ModulePackages` attribute.
pub type ModulePackagesWriter<B> = ArrayWriter<B, NonZero<u16>>;

/// Name of `ModuleMainClass` attribute. Use [`ConstantPoolIndex`] for its index (`Class`).
pub const NAME_MODULE_MAIN_CLASS: &str = "ModuleMainClass";
/// Name of `NestHost` attribute. Use [`ConstantPoolIndex`] for its index (`Class`).
pub const NAME_NEST_HOST: &str = "NestHost";

/// Name of `NestMembers` attribute.
/// See [`NestMembersReader`] and [`NestMembersWriter`] for usage.
pub const NAME_NEST_MEMBERS: &str = "NestMembers";
/// Reader of `NestMembers` attribute.
pub type NestMembersReader<'a, B> = ArrayReader<'a, B, NonZero<u16>>;
/// Writer of `NestMembers` attribute.
pub type NestMembersWriter<B> = ArrayWriter<B, NonZero<u16>>;

/// Name of `PermittedSubclasses` attribute.
/// See [`PermittedSubclassesReader`] and [`PermittedSubclassesWriter`] for usage.
pub const NAME_PERMITTED_SUBCLASSES: &str = "PermittedSubclasses";
/// Reader of `PermittedSubclasses` attribute.
pub type PermittedSubclassesReader<'a, B> = ArrayReader<'a, B, NonZero<u16>>;
/// Writer of `PermittedSubclasses` attribute.
pub type PermittedSubclassesWriter<B> = ArrayWriter<B, NonZero<u16>>;

#[cfg(feature = "alloc")]
pub use need_alloc::*;

#[cfg(feature = "alloc")]
mod need_alloc {
    use core::{num::NonZero, ops::Range};

    use alloc::{borrow::Cow, vec::Vec};
    use arrayvec::ArrayVec;
    use bitflags::bitflags;

    use crate::{
        Attribute, Error,
        attributes::{ArrayReader, ArrayReaderU8, ArrayWriter, ArrayWriterU8},
        util::{Decode, Encode},
    };

    /// Name of `StackMapTable` attribute. See [`StackMapTableReader`] and [`StackMapTableWriter`] for usage.
    pub const NAME_STACK_MAP_TABLE: &str = "StackMapTable";
    /// Reader of `StackMapTable` attribute.
    pub type StackMapTableReader<'env, 'a, B> = ArrayReader<'a, B, StackMapFrame<'env>>;
    /// Writer of `StackMapTable` attribute.
    pub type StackMapTableWriter<'env, B> = ArrayWriter<B, StackMapFrame<'env>>;

    /// A stack map frame of a method.
    ///
    /// Tuple variants are used for `offset_delta` and `stack`.
    #[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
    pub enum StackMapFrame<'a> {
        /// The frame has exactly the same local variables as the previous frame
        /// and that the operand stack is empty.
        SameFrame(u16),
        /// The frame has exactly the same local variables as the previous frame
        /// and that the operand stack has one entry.
        SameLocals1StackItemFrame(u16, VerificationType),
        /// The frame has exactly the same local variables as the previous frame
        /// and that the operand stack has one entry.
        SameLocals1StackItemFrameExtended(u16, VerificationType),
        /// The frame has the same local variables as the previous frame except that
        /// the last k local variables are absent, and that the operand stack is empty.
        ChopFrame {
            /// The offset delta.
            offset_delta: u16,
            /// `k` value, indicating that last `k` local variables are absent.
            absent: NonZero<u16>,
        },
        /// The frame has exactly the same local variables as the previous frame
        /// and that the operand stack is empty.
        SameFrameExtended(u16),
        /// The frame has the same locals as the previous frame except that k additional locals are defined,
        /// and that the operand stack is empty.
        AppendFrame {
            /// The offset delta.
            offset_delta: u16,
            /// The additional locals.
            locals: ArrayVec<VerificationType, 3>,
        },
        /// A full frame.
        FullFrame {
            /// The offset delta.
            offset_delta: u16,
            /// The locals.
            locals: Cow<'a, [VerificationType]>,
            /// The stack.
            stack: Cow<'a, [VerificationType]>,
        },
    }

    fn validate_val(val: u16, max: u16) -> Result<(), Error> {
        if val <= max {
            Ok(())
        } else {
            Err(Error::OffsetOutOfBounds)
        }
    }

    impl<'de> Decode<'de> for StackMapFrame<'_> {
        fn decode<B: crate::util::Buf<'de>>(mut buf: B) -> Result<Self, Error> {
            let tag: u8 = buf.read()?;
            match tag {
                0..=63 => Ok(Self::SameFrame(tag as u16)),
                64..=127 => Ok(Self::SameLocals1StackItemFrame(
                    tag as u16 - 64,
                    buf.read()?,
                )),
                247 => Ok(Self::SameLocals1StackItemFrameExtended(
                    buf.read()?,
                    buf.read()?,
                )),
                248..=250 => Ok(Self::ChopFrame {
                    offset_delta: buf.read()?,
                    absent: NonZero::new(251 - tag as u16).unwrap(),
                }),
                251 => Ok(Self::SameFrameExtended(buf.read()?)),
                252..=254 => {
                    let offset_delta = buf.read()?;
                    let k = tag as usize - 251;
                    let mut locals = ArrayVec::new();
                    for _ in 0..k {
                        locals.push(buf.read()?);
                    }
                    Ok(Self::AppendFrame {
                        offset_delta,
                        locals,
                    })
                }
                255 => {
                    let offset_delta = buf.read()?;
                    let locals_len: u16 = buf.read()?;
                    let mut locals = Vec::with_capacity(locals_len as usize);
                    for _ in 0..locals_len {
                        locals.push(buf.read()?);
                    }
                    let stack_len: u16 = buf.read()?;
                    let mut stack = Vec::with_capacity(stack_len as usize);
                    for _ in 0..stack_len {
                        stack.push(buf.read()?);
                    }
                    Ok(Self::FullFrame {
                        offset_delta,
                        locals: Cow::Owned(locals),
                        stack: Cow::Owned(stack),
                    })
                }
                myth => Err(Error::UnknownFrameType(myth)),
            }
        }
    }

    impl Encode for StackMapFrame<'_> {
        fn encode<B: crate::util::BufMut>(&self, mut buf: B) -> Result<(), Error> {
            match self {
                StackMapFrame::SameFrame(delta) => {
                    validate_val(*delta, 63)?;
                    buf.write(*delta as u8)
                }
                StackMapFrame::SameLocals1StackItemFrame(delta, stack) => {
                    let frame_ty = *delta + 64;
                    validate_val(frame_ty, 127)?;
                    buf.write(frame_ty as u8)?;
                    buf.write(stack)
                }
                StackMapFrame::SameLocals1StackItemFrameExtended(delta, stack) => {
                    buf.write(247u8)?;
                    buf.write(delta)?;
                    buf.write(stack)
                }
                StackMapFrame::ChopFrame {
                    offset_delta,
                    absent,
                } => {
                    validate_val(absent.get(), 3)?;
                    buf.write(251 - absent.get() as u8)?;
                    buf.write(offset_delta)
                }
                StackMapFrame::SameFrameExtended(delta) => {
                    buf.write(251u8)?;
                    buf.write(delta)
                }
                StackMapFrame::AppendFrame {
                    offset_delta,
                    locals,
                } => {
                    let k = locals.len();
                    buf.write(251 + k as u8)?;
                    buf.write(offset_delta)?;
                    for local in locals {
                        buf.write(local)?;
                    }
                    Ok(())
                }
                StackMapFrame::FullFrame {
                    offset_delta,
                    locals,
                    stack,
                } => {
                    buf.write(255u8)?;
                    buf.write(offset_delta)?;
                    buf.write(locals.len() as u16)?;
                    for local in &**locals {
                        buf.write(local)?;
                    }
                    buf.write(stack.len() as u16)?;
                    for ty in &**stack {
                        buf.write(ty)?;
                    }
                    Ok(())
                }
            }
        }
    }

    /// The type of either one or two locations, where a location is either a single local variable
    /// or a single operand stack entry.
    #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
    pub enum VerificationType {
        /// The local variable has the verification type `top`.
        Top,
        /// The local variable has the verification type `integer`.
        Integer,
        /// The local variable has the verification type `float`.
        Float,
        /// The local variable has the verification type `double`.
        Double,
        /// The local variable has the verification type `long`.
        Long,
        /// The local variable has the verification type `null`.
        Null,
        /// The local variable has the verification type `uninitializedThis`.
        UninitializedThis,
        /// The local variable has the verification type which is the class
        /// represented by the index of the class in constant pool.
        Object(NonZero<u16>),
        /// The local variable has the verification type `uninitialized(Offset)`.
        ///
        /// The `Offset` item indicates the offset, in the `code` array of the `Code` attribute
        /// that contains this `StackMapTable` attribute, of the new instruction that created
        /// the object being stored in the location.
        Uninitialized(u16),
    }

    impl<'de> Decode<'de> for VerificationType {
        fn decode<B: crate::util::Buf<'de>>(mut buf: B) -> Result<Self, Error> {
            let tag: u8 = buf.read()?;
            match tag {
                0 => Ok(Self::Top),
                1 => Ok(Self::Integer),
                2 => Ok(Self::Float),
                3 => Ok(Self::Double),
                4 => Ok(Self::Long),
                5 => Ok(Self::Null),
                6 => Ok(Self::UninitializedThis),
                7 => Ok(Self::Object(buf.read()?)),
                8 => Ok(Self::Uninitialized(buf.read()?)),
                _ => Err(Error::UnknownVerificationType(tag)),
            }
        }
    }

    impl Encode for VerificationType {
        fn encode<B: crate::util::BufMut>(&self, mut buf: B) -> Result<(), Error> {
            match self {
                VerificationType::Top => buf.write(0),
                VerificationType::Integer => buf.write(1),
                VerificationType::Float => buf.write(2),
                VerificationType::Double => buf.write(3),
                VerificationType::Long => buf.write(4),
                VerificationType::Null => buf.write(5),
                VerificationType::UninitializedThis => buf.write(6),
                VerificationType::Object(index) => {
                    buf.write(7)?;
                    buf.write(index)
                }
                VerificationType::Uninitialized(index) => {
                    buf.write(8)?;
                    buf.write(index)
                }
            }
        }
    }

    /// A single runtime visible annotation on a declaration.
    #[derive(Debug, Clone, PartialEq, Eq, Hash)]
    pub struct Annotation<'a> {
        /// The field descriptor (`Utf8`) index in constant pool.
        pub type_idx: NonZero<u16>,
        /// Element-value pairs in the annotation represented by this annotation structure.
        ///
        /// The index is for the name of element of that value (`Utf8`) in constant pool.
        pub pairs: Cow<'a, [(NonZero<u16>, ElementValue<'a>)]>,
    }

    /// A single runtime visible annotation on a type used in a declaration or expression.
    #[derive(Debug, Clone, PartialEq, Eq, Hash)]
    pub struct TypedAnnotation<'a> {
        /// Which type in a declaration or expression is annotated.
        pub target: AnnotationTarget<'a>,
        /// Which part of the type is annotated.
        pub type_path: Cow<'a, [TypePathEntry]>,
        /// The field descriptor (`Utf8`) index in constant pool.
        pub type_idx: NonZero<u16>,
        /// Element-value pairs in the annotation represented by this annotation structure.
        ///
        /// The index is for the name of element of that value (`Utf8`) in constant pool.
        pub pairs: Cow<'a, [(NonZero<u16>, ElementValue<'a>)]>,
    }

    /// Specifies precisely which type in a declaration or expression is annotated.
    #[derive(Debug, Clone, PartialEq, Eq, Hash)]
    pub enum AnnotationTarget<'a> {
        /// An annotation appears on the declaration of the `i`'th type parameter of a generic class,
        /// generic interface, generic method, or generic constructor.
        TypeParameter(TypeParameterKind, u8),
        /// An annotation appears on a type in the `extends` or `implements` clause of
        /// a class or interface declaration.
        ///
        /// `None` specifies that the annotation appears on the superclass in an
        /// `extends` clause of a class declaration.
        SuperType(Option<u16>),
        /// An annotation appears on the `i`'th bound of the `j`'th type parameter declaration of
        /// a generic class, interface, method, or constructor.
        TypeParameterBound {
            /// Kind of target.
            kind: TypeParameterKind,
            /// Which type parameter declaration has an annotated bound.
            type_param_idx: u8,
            /// Which bound of the type parameter declaration indicated by `type_parameter_idx` is annotated.
            bound_idx: u8,
        },
        /// An annotation appears on either the type in a:
        ///
        /// - Field declaration
        /// - Type in a record component declaration
        /// - Return type of a method
        /// - Type of a newly constructed object
        /// - Receiver type of a method or constructor.
        Empty(EmptyAnnotationKind),
        /// An annotation appears on the type in a formal parameter declaration of a method,
        /// constructor, or lambda expression.
        FormalParameter(u8),
        /// An annotation appears on the `i`'th type in the throws clause of a method
        /// or constructor declaration.
        Throws(u16),
        /// An annotation appears on the type in a local variable declaration,
        /// including a variable declared as a resource in a `try`-with-resources statement.
        LocalVar(LocalVarKind, Cow<'a, [LocalVarTableEntry]>),
        /// An annotation appears on the `i`'th type in an exception parameter declaration.
        Catch(u16),
        /// An annotation appears on either the type in an `instanceof` expression or a new expression,
        /// or the type before the `::` in a method reference expression.
        Offset(OffsetAnnotationKind, u16),
        /// An annotation appears either on the `i`'th type in a cast expression, or on the `i`'th type argument
        /// in the explicit type argument list for any of the following:
        ///
        /// - New expression
        /// - Explicit constructor invocation statement
        /// - Method invocation expression
        /// - Method reference expression.
        TypeArgument {
            /// Kind of target.
            kind: TypeArgumentKind,
            /// The `code` array offset.
            offset: u16,
            /// Which type argument is annotated.
            type_arg_idx: u8,
        },
    }

    /// Kind of [`AnnotationTarget::TypeParameter`].
    #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
    pub enum TypeParameterKind {
        /// Type parameter declaration of generic class or interface.
        Class,
        /// Type parameter declaration of generic method or constructor.
        Method,
    }

    /// Kind of [`AnnotationTarget::Empty`].
    #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
    pub enum EmptyAnnotationKind {
        /// Type in field or record component declaration.
        Field,
        /// Return type of method, or type of newly constructed object.
        Return,
        /// Receiver type of method or constructor.
        Receiver,
    }

    /// Kind of [`AnnotationTarget::LocalVar`].
    #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
    pub enum LocalVarKind {
        /// Type in local variable declaration.
        Local,
        /// Type in resource variable declaration.
        Resource,
    }

    /// Kind of [`AnnotationTarget::Offset`].
    #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
    pub enum OffsetAnnotationKind {
        /// Type in `instanceof` expression.
        InstanceOf,
        /// Type in `new` expression.
        New,
        /// Type in method reference expression using `::new`.
        MethodRefNew,
        /// Type in method reference expression using `::Identifier`.
        MethodRefIdent,
    }

    /// Kind of [`AnnotationTarget::TypeArgument`].
    #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
    pub enum TypeArgumentKind {
        /// Type in *cast* expression.
        Cast,
        /// Type argument for generic constructor in `new` expression or
        /// explicit constructor invocation statement.
        New,
        /// Type argument for generic method in method invocation expression.
        MethodInvocation,
        /// Type argument for generic constructor in method reference expression using `::new`.
        MethodRefNew,
        /// Type argument for generic constructor in method reference expression using `::Identifier`.
        MethodRefIdent,
    }

    /// Specifies a local variable whose type is annotated.
    #[derive(Debug, PartialEq, Eq, Clone, Hash)]
    pub struct LocalVarTableEntry {
        /// The interval where the given local variable has a value at indices in the `code` array.
        pub indices: Range<u16>,
        /// Index into the local variable array of the current frame.
        /// The given local variable is at `index` in the local variable array of the current frame.
        ///
        /// If the given local variable is of type `double` or `long`, it occupies both `index` and `index + 1`.
        pub index: u16,
    }

    /// Entry of `type_path` in a [`TypedAnnotation`].
    #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
    pub enum TypePathEntry {
        /// Annotation is deeper in an array type.
        Array,
        /// Annotation is deeper in a nested type.
        Nested,
        /// Annotation is on the bound of a wildcard type argument of a parameterized type.
        Wildcard,
        /// Annotation is on a type argument of a parameterized type.
        Argument(u8),
    }

    /// Value of an element-value pair.
    #[derive(Debug, Clone, PartialEq, Eq, Hash)]
    pub enum ElementValue<'a> {
        /// A constant of either a primitive type or the type `String`.
        Const {
            /// Exact type of the value.
            tag: ElementConstValueType,
            /// Corresponding entry in constant pool.
            /// See documentation for each variant of [`ElementConstValueType`] for its desired type.
            index: NonZero<u16>,
        },
        /// An enum constant.
        Enum {
            /// The field descriptor index (`Utf8`) in the constant pool.
            type_name_idx: NonZero<u16>,
            /// The simple name index (`Utf8`) in the constant pool.
            const_name_idx: NonZero<u16>,
        },
        /// A class literal, denoted by the `Class` index in constant pool.
        Class(NonZero<u16>),
        /// A "nested" annotation.
        Annotation(Annotation<'a>),
        /// An array.
        Array(Cow<'a, [Self]>),
    }

    /// Type of a constant element value.
    #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
    pub enum ElementConstValueType {
        /// Tag item `B` with type `Integer`.
        Byte,
        /// Tag item `C` with type `Integer`.
        Char,
        /// Tag item `D` with type `Double`.
        Double,
        /// Tag item `F` with type `Float`.
        Float,
        /// Tag item `I` with type `Integer`.
        Int,
        /// Tag item `J` with type `Long`.
        Long,
        /// Tag item `S` with type `Integer`.
        Short,
        /// Tag item `Z` with type `Integer`.
        Boolean,
        /// Tag item `s` with type `Utf8`.
        String,
    }

    impl<'de> Decode<'de> for Annotation<'_> {
        fn decode<B: crate::util::Buf<'de>>(mut buf: B) -> Result<Self, Error> {
            let type_idx = buf.read()?;
            let len: u16 = buf.read()?;
            let mut pairs = Vec::with_capacity(len as usize);
            for _ in 0..len {
                pairs.push(buf.read()?);
            }
            Ok(Self {
                type_idx,
                pairs: Cow::Owned(pairs),
            })
        }
    }

    impl Encode for Annotation<'_> {
        fn encode<B: crate::util::BufMut>(&self, mut buf: B) -> Result<(), Error> {
            buf.write(self.type_idx)?;
            buf.write(self.pairs.len() as u16)?;
            for pair in &*self.pairs {
                buf.write(pair)?;
            }
            Ok(())
        }
    }

    impl ElementValue<'_> {
        simple_rw! {}
    }

    impl<'de> Decode<'de> for ElementValue<'_> {
        fn decode<B: crate::util::Buf<'de>>(mut buf: B) -> Result<Self, Error> {
            let tag: u8 = buf.read()?;
            match tag {
                b'B' | b'C' | b'D' | b'F' | b'I' | b'J' | b'S' | b'Z' | b's' => {
                    let ty = match tag {
                        b'B' => ElementConstValueType::Byte,
                        b'C' => ElementConstValueType::Char,
                        b'D' => ElementConstValueType::Double,
                        b'F' => ElementConstValueType::Float,
                        b'I' => ElementConstValueType::Int,
                        b'J' => ElementConstValueType::Long,
                        b'S' => ElementConstValueType::Short,
                        b'Z' => ElementConstValueType::Boolean,
                        b's' => ElementConstValueType::String,
                        _ => unreachable!(),
                    };
                    Ok(Self::Const {
                        tag: ty,
                        index: buf.read()?,
                    })
                }
                b'e' => Ok(Self::Enum {
                    type_name_idx: buf.read()?,
                    const_name_idx: buf.read()?,
                }),
                b'c' => Ok(Self::Class(buf.read()?)),
                b'@' => Ok(Self::Annotation(buf.read()?)),
                b'[' => {
                    let len: u16 = buf.read()?;
                    let mut vals = Vec::with_capacity(len as usize);
                    for _ in 0..len {
                        vals.push(buf.read()?);
                    }
                    Ok(Self::Array(Cow::Owned(vals)))
                }
                myth => Err(Error::UnknownElementValueType(myth)),
            }
        }
    }

    impl Encode for ElementValue<'_> {
        fn encode<B: crate::util::BufMut>(&self, mut buf: B) -> Result<(), Error> {
            match self {
                ElementValue::Const { tag, index } => {
                    match tag {
                        ElementConstValueType::Byte => buf.write(b'B')?,
                        ElementConstValueType::Char => buf.write(b'C')?,
                        ElementConstValueType::Double => buf.write(b'D')?,
                        ElementConstValueType::Float => buf.write(b'F')?,
                        ElementConstValueType::Int => buf.write(b'I')?,
                        ElementConstValueType::Long => buf.write(b'J')?,
                        ElementConstValueType::Short => buf.write(b'S')?,
                        ElementConstValueType::Boolean => buf.write(b'Z')?,
                        ElementConstValueType::String => buf.write(b's')?,
                    }
                    buf.write(index)?;
                }
                ElementValue::Enum {
                    type_name_idx,
                    const_name_idx,
                } => {
                    buf.write(b'e')?;
                    buf.write(type_name_idx)?;
                    buf.write(const_name_idx)?;
                }
                ElementValue::Class(index) => {
                    buf.write(b'c')?;
                    buf.write(index)?;
                }
                ElementValue::Annotation(val) => {
                    buf.write(b'@')?;
                    buf.write(val)?;
                }
                ElementValue::Array(cow) => {
                    buf.write(b'[')?;
                    buf.write(cow.len() as u16)?;
                    for val in &**cow {
                        buf.write(val)?;
                    }
                }
            }
            Ok(())
        }
    }

    impl<'de> Decode<'de> for LocalVarTableEntry {
        fn decode<B: crate::util::Buf<'de>>(mut buf: B) -> Result<Self, Error> {
            let start: u16 = buf.read()?;
            let len: u16 = buf.read()?;
            Ok(Self {
                indices: start..start + len,
                index: buf.read()?,
            })
        }
    }

    impl Encode for LocalVarTableEntry {
        fn encode<B: crate::util::BufMut>(&self, mut buf: B) -> Result<(), Error> {
            buf.write(self.indices.start)?;
            buf.write(self.indices.end - self.indices.start)?;
            buf.write(self.index)?;
            Ok(())
        }
    }

    impl<'de> Decode<'de> for TypePathEntry {
        fn decode<B: crate::util::Buf<'de>>(mut buf: B) -> Result<Self, Error> {
            let tag: u8 = buf.read()?;
            let idx = buf.read()?;
            match tag {
                0 => Ok(Self::Array),
                1 => Ok(Self::Nested),
                2 => Ok(Self::Wildcard),
                3 => Ok(Self::Argument(idx)),
                myth => Err(Error::UnknownTypePathKind(myth)),
            }
        }
    }

    impl Encode for TypePathEntry {
        fn encode<B: crate::util::BufMut>(&self, mut buf: B) -> Result<(), Error> {
            let mut idx = 0;
            match self {
                TypePathEntry::Array => buf.write(0u8)?,
                TypePathEntry::Nested => buf.write(1u8)?,
                TypePathEntry::Wildcard => buf.write(2u8)?,
                TypePathEntry::Argument(i) => {
                    buf.write(3u8)?;
                    idx = *i
                }
            }
            buf.write(idx)?;
            Ok(())
        }
    }

    impl<'de> Decode<'de> for AnnotationTarget<'_> {
        fn decode<B: crate::util::Buf<'de>>(mut buf: B) -> Result<Self, Error> {
            let tag: u8 = buf.read()?;
            match tag {
                0x00 | 0x01 => Ok(Self::TypeParameter(
                    match tag {
                        0x00 => TypeParameterKind::Class,
                        0x01 => TypeParameterKind::Method,
                        _ => unreachable!(),
                    },
                    buf.read()?,
                )),
                0x10 => {
                    let idx: u16 = buf.read()?;
                    Ok(Self::SuperType(Some(idx).filter(|i| *i != u16::MAX)))
                }
                0x11 | 0x12 => Ok(Self::TypeParameterBound {
                    kind: match tag {
                        0x11 => TypeParameterKind::Class,
                        0x12 => TypeParameterKind::Method,
                        _ => unreachable!(),
                    },
                    type_param_idx: buf.read()?,
                    bound_idx: buf.read()?,
                }),
                0x13..=0x15 => Ok(Self::Empty(match tag {
                    0x13 => EmptyAnnotationKind::Field,
                    0x14 => EmptyAnnotationKind::Return,
                    0x15 => EmptyAnnotationKind::Receiver,
                    _ => unreachable!(),
                })),
                0x16 => Ok(Self::FormalParameter(buf.read()?)),
                0x17 => Ok(Self::Throws(buf.read()?)),
                0x40 | 0x41 => {
                    let len: u16 = buf.read()?;
                    let mut vec = Vec::with_capacity(len as usize);
                    for _ in 0..len {
                        vec.push(buf.read()?);
                    }
                    Ok(Self::LocalVar(
                        match tag {
                            0x40 => LocalVarKind::Local,
                            0x41 => LocalVarKind::Resource,
                            _ => unreachable!(),
                        },
                        Cow::Owned(vec),
                    ))
                }
                0x42 => Ok(Self::Catch(buf.read()?)),
                0x43..=0x46 => Ok(Self::Offset(
                    match tag {
                        0x43 => OffsetAnnotationKind::InstanceOf,
                        0x44 => OffsetAnnotationKind::New,
                        0x45 => OffsetAnnotationKind::MethodRefNew,
                        0x46 => OffsetAnnotationKind::MethodRefIdent,
                        _ => unreachable!(),
                    },
                    buf.read()?,
                )),
                0x47..=0x4B => Ok(Self::TypeArgument {
                    kind: match tag {
                        0x47 => TypeArgumentKind::Cast,
                        0x48 => TypeArgumentKind::New,
                        0x49 => TypeArgumentKind::MethodInvocation,
                        0x4A => TypeArgumentKind::MethodRefNew,
                        0x4B => TypeArgumentKind::MethodRefIdent,
                        _ => unreachable!(),
                    },
                    offset: buf.read()?,
                    type_arg_idx: buf.read()?,
                }),
                myth => Err(Error::UnknownAnnotationTargetType(myth)),
            }
        }
    }

    impl Encode for AnnotationTarget<'_> {
        fn encode<B: crate::util::BufMut>(&self, mut buf: B) -> Result<(), Error> {
            match self {
                AnnotationTarget::TypeParameter(kind, idx) => {
                    let tag: u8 = match kind {
                        TypeParameterKind::Class => 0x00,
                        TypeParameterKind::Method => 0x01,
                    };
                    buf.write(tag)?;
                    buf.write(idx)?;
                }
                AnnotationTarget::SuperType(idx) => {
                    buf.write(0x10u8)?;
                    buf.write(idx.unwrap_or(u16::MAX))?;
                }
                AnnotationTarget::TypeParameterBound {
                    kind,
                    type_param_idx,
                    bound_idx,
                } => {
                    let tag: u8 = match kind {
                        TypeParameterKind::Class => 0x11,
                        TypeParameterKind::Method => 0x12,
                    };
                    buf.write(tag)?;
                    buf.write(type_param_idx)?;
                    buf.write(bound_idx)?;
                }
                AnnotationTarget::Empty(kind) => {
                    let tag: u8 = match kind {
                        EmptyAnnotationKind::Field => 0x13,
                        EmptyAnnotationKind::Return => 0x14,
                        EmptyAnnotationKind::Receiver => 0x15,
                    };
                    buf.write(tag)?;
                }
                AnnotationTarget::FormalParameter(idx) => {
                    buf.write(0x16u8)?;
                    buf.write(idx)?;
                }
                AnnotationTarget::Throws(idx) => {
                    buf.write(0x17u8)?;
                    buf.write(idx)?;
                }
                AnnotationTarget::LocalVar(kind, list) => {
                    let tag: u8 = match kind {
                        LocalVarKind::Local => 0x40,
                        LocalVarKind::Resource => 0x41,
                    };
                    buf.write(tag)?;
                    buf.write(list.len() as u16)?;
                    for entry in &**list {
                        buf.write(entry)?;
                    }
                }
                AnnotationTarget::Catch(idx) => {
                    buf.write(0x42u8)?;
                    buf.write(idx)?;
                }
                AnnotationTarget::Offset(kind, offset) => {
                    let tag: u8 = match kind {
                        OffsetAnnotationKind::InstanceOf => 0x43,
                        OffsetAnnotationKind::New => 0x44,
                        OffsetAnnotationKind::MethodRefNew => 0x45,
                        OffsetAnnotationKind::MethodRefIdent => 0x46,
                    };
                    buf.write(tag)?;
                    buf.write(offset)?;
                }
                AnnotationTarget::TypeArgument {
                    kind,
                    offset,
                    type_arg_idx,
                } => {
                    let tag: u8 = match kind {
                        TypeArgumentKind::Cast => 0x47,
                        TypeArgumentKind::New => 0x48,
                        TypeArgumentKind::MethodInvocation => 0x49,
                        TypeArgumentKind::MethodRefNew => 0x4A,
                        TypeArgumentKind::MethodRefIdent => 0x4B,
                    };
                    buf.write(tag)?;
                    buf.write(offset)?;
                    buf.write(type_arg_idx)?;
                }
            }
            Ok(())
        }
    }

    impl<'de> Decode<'de> for TypedAnnotation<'_> {
        fn decode<B: crate::util::Buf<'de>>(mut buf: B) -> Result<Self, Error> {
            Ok(Self {
                target: buf.read()?,
                type_path: {
                    let len: u8 = buf.read()?;
                    let mut vec = Vec::with_capacity(len as usize);
                    for _ in 0..len {
                        vec.push(buf.read()?)
                    }
                    Cow::Owned(vec)
                },
                type_idx: buf.read()?,
                pairs: {
                    let len: u16 = buf.read()?;
                    let mut vec = Vec::with_capacity(len as usize);
                    for _ in 0..len {
                        vec.push(buf.read()?)
                    }
                    Cow::Owned(vec)
                },
            })
        }
    }

    impl Encode for TypedAnnotation<'_> {
        fn encode<B: crate::util::BufMut>(&self, mut buf: B) -> Result<(), Error> {
            buf.write(&self.target)?;
            buf.write(self.type_path.len() as u8)?;
            for entry in &*self.type_path {
                buf.write(entry)?;
            }
            buf.write(self.type_idx)?;
            buf.write(self.pairs.len() as u16)?;
            for entry in &*self.pairs {
                buf.write(entry)?;
            }
            Ok(())
        }
    }

    /// Name of `RuntimeInvisibleAnnotations` attribute. See [`RuntimeInvisibleAnnotationsReader`] and [`RuntimeInvisibleAnnotationsWriter`] for usage.
    pub const NAME_RUNTIME_INVISIBLE_ANNOTATIONS: &str = "RuntimeInvisibleAnnotations";
    /// Reader of `RuntimeInvisibleAnnotations` attribute.
    pub type RuntimeInvisibleAnnotationsReader<'env, 'a, B> = ArrayReader<'a, B, Annotation<'env>>;
    /// Writer of `RuntimeInvisibleAnnotations` attribute.
    pub type RuntimeInvisibleAnnotationsWriter<'env, B> = ArrayWriter<B, Annotation<'env>>;

    /// All of the runtime visible annotations on the declaration of a single formal parameter.
    #[derive(Debug)]
    pub struct ParamAnnotations<'a>(pub Cow<'a, [Annotation<'a>]>);

    impl<'de> Decode<'de> for ParamAnnotations<'_> {
        fn decode<B: crate::util::Buf<'de>>(mut buf: B) -> Result<Self, Error> {
            let len: u16 = buf.read()?;
            let mut vec = Vec::with_capacity(len as usize);
            for _ in 0..len {
                vec.push(buf.read()?);
            }
            Ok(Self(Cow::Owned(vec)))
        }
    }

    impl Encode for ParamAnnotations<'_> {
        fn encode<B: crate::util::BufMut>(&self, mut buf: B) -> Result<(), Error> {
            buf.write(self.0.len() as u16)?;
            for val in &*self.0 {
                buf.write(val)?;
            }
            Ok(())
        }
    }

    /// Name of `RuntimeVisibleParameterAnnotations` attribute.
    /// See [`RuntimeVisibleParamAnnotationsReader`] and [`RuntimeVisibleParamAnnotationsWriter`] for usage.
    pub const NAME_RUNTIME_VISIBLE_PARAM_ANNOTATIONS: &str = "RuntimeVisibleParameterAnnotations";
    /// Reader of `RuntimeVisibleParameterAnnotations` attribute.
    pub type RuntimeVisibleParamAnnotationsReader<'env, 'a, B> =
        ArrayReaderU8<'a, B, ParamAnnotations<'env>>;
    /// Writer of `RuntimeVisibleParameterAnnotations` attribute.
    pub type RuntimeVisibleParamAnnotationsWriter<'env, B> =
        ArrayWriterU8<B, ParamAnnotations<'env>>;

    /// Name of `RuntimeInvisibleParameterAnnotations` attribute.
    /// See [`RuntimeInvisibleParamAnnotationsReader`] and [`RuntimeInvisibleParamAnnotationsWriter`] for usage.
    pub const NAME_RUNTIME_INVISIBLE_PARAM_ANNOTATIONS: &str =
        "RuntimeInvisibleParameterAnnotations";
    /// Reader of `RuntimeInvisibleParameterAnnotations` attribute.
    pub type RuntimeInvisibleParamAnnotationsReader<'env, 'a, B> =
        ArrayReaderU8<'a, B, ParamAnnotations<'env>>;
    /// Writer of `RuntimeInvisibleParameterAnnotations` attribute.
    pub type RuntimeInvisibleParamAnnotationsWriter<'env, B> =
        ArrayWriterU8<B, ParamAnnotations<'env>>;

    /// Name of `RuntimeVisibleTypeAnnotations` attribute.
    /// See [`RuntimeVisibleTypeAnnotationsReader`] and [`RuntimeVisibleTypeAnnotationsWriter`] for usage.
    pub const NAME_RUNTIME_VISIBLE_TYPE_ANNOTATIONS: &str = "RuntimeVisibleTypeAnnotations";
    /// Reader of `RuntimeVisibleTypeAnnotations` attribute.
    pub type RuntimeVisibleTypeAnnotationsReader<'env, 'a, B> =
        ArrayReader<'a, B, TypedAnnotation<'env>>;
    /// Writer of `RuntimeVisibleTypeAnnotations` attribute.
    pub type RuntimeVisibleTypeAnnotationsWriter<'env, B> = ArrayWriter<B, TypedAnnotation<'env>>;

    /// Name of `RuntimeInvisibleTypeAnnotations` attribute.
    /// See [`RuntimeInvisibleTypeAnnotationsReader`] and [`RuntimeInvisibleTypeAnnotationsWriter`] for usage.
    pub const NAME_RUNTIME_INVISIBLE_TYPE_ANNOTATIONS: &str = "RuntimeInvisibleTypeAnnotations";
    /// Reader of `RuntimeInvisibleTypeAnnotations` attribute.
    pub type RuntimeInvisibleTypeAnnotationsReader<'env, 'a, B> =
        ArrayReader<'a, B, TypedAnnotation<'env>>;
    /// Writer of `RuntimeInvisibleTypeAnnotations` attribute.
    pub type RuntimeInvisibleTypeAnnotationsWriter<'env, B> = ArrayWriter<B, TypedAnnotation<'env>>;

    /// Name of `AnnotationDefault` attribute. It's represented by [`ElementValue`].
    pub const NAME_ANNOTATION_DEFAULT: &str = "AnnotationDefault";

    /// A bootstrap method entry in `BootstrapMethods`.
    #[derive(Debug, Clone, PartialEq, Eq, Hash)]
    pub struct BootstrapMethod<'a> {
        /// Index to `MethodHandle` in constant pool.
        pub method_ref: NonZero<u16>,
        /// Indexes to *static arguments* for the bootstrap method in the constant pool.
        pub args: Cow<'a, [NonZero<u16>]>,
    }

    impl<'de> Decode<'de> for BootstrapMethod<'_> {
        fn decode<B: crate::util::Buf<'de>>(mut buf: B) -> Result<Self, Error> {
            Ok(Self {
                method_ref: buf.read()?,
                args: {
                    let len: u16 = buf.read()?;
                    let mut vec = Vec::with_capacity(len as usize);
                    for _ in 0..len {
                        vec.push(buf.read()?);
                    }
                    Cow::Owned(vec)
                },
            })
        }
    }

    impl Encode for BootstrapMethod<'_> {
        fn encode<B: crate::util::BufMut>(&self, mut buf: B) -> Result<(), Error> {
            buf.write(self.method_ref)?;
            buf.write(self.args.len() as u16)?;
            for entry in &*self.args {
                buf.write(entry)?;
            }
            Ok(())
        }
    }

    /// Name of `BootstrapMethods` attribute.
    /// See [`BootstrapMethodsReader`] and [`BootstrapMethodsWriter`] for usage.
    pub const NAME_BOOTSTRAP_METHODS: &str = "BootstrapMethods";
    /// Reader of `BootstrapMethods` attribute.
    pub type BootstrapMethodsReader<'env, 'a, B> = ArrayReader<'a, B, BootstrapMethod<'env>>;
    /// Writer of `BootstrapMethods` attribute.
    pub type BootstrapMethodsWriter<'env, B> = ArrayWriter<B, BootstrapMethod<'env>>;

    /// Name of [`Module`] attribute.
    pub const NAME_MODULE: &str = "Module";

    /// Information about a module, consists of following properties:
    ///
    /// - The modules required by a module
    /// - The packages exported and opened by a module
    /// - The services used and provided by a module.
    #[derive(Debug, Clone, PartialEq, Eq)]
    pub struct Module<'a> {
        /// Name of this module, as index to constant table (`Utf8`).
        pub name_idx: NonZero<u16>,
        /// Properties of this module.
        pub flags: ModuleFlags,
        /// Version information about the module as index to constant table (`Utf8`), if present.
        pub version_idx: Option<NonZero<u16>>,

        /// Dependencies of this module.
        pub requires: Cow<'a, [ModuleRequired]>,
        /// Packages exported by this module.
        pub exports: Cow<'a, [PackageRelation<'a>]>,
        /// Packages opened by this module.
        pub opens: Cow<'a, [PackageRelation<'a>]>,

        /// `ServiceLoader`s used by this module, by the form of constant pool indices. (`Class`)
        pub uses: Cow<'a, [NonZero<u16>]>,
        /// Serivce implementations of interfaces.
        pub provides: Cow<'a, [ModuleProvided<'a>]>,
    }

    bitflags! {
        /// Denote properties of a module.
        #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
        pub struct ModuleFlags: u16 {
            /// Indicates that this module is open.
            const OPEN = 0x0020;
            /// Indicates that this module was not explicitly or implicitly declared.
            const SYNTHETIC = 0x1000;
            /// Indicates that this module was implicitly declared.
            const MANDATED = 0x8000;
        }
    }

    /// A dependency of a module.
    #[derive(Debug, PartialEq, Eq, Clone, Copy, Hash)]
    pub struct ModuleRequired {
        /// The depended module, as an index in constant pool (`Module`).
        pub index: NonZero<u16>,
        /// Properties of the dependency.
        pub flags: ModuleRequiredFlags,
        /// Version information about the module as index to constant table (`Utf8`), if present.
        pub version_idx: Option<NonZero<u16>>,
    }

    bitflags! {
        /// Denote properties of a module dependency.
        #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
        pub struct ModuleRequiredFlags: u16 {
            /// Indicates that any module which depends on the current module,
            /// implicitly declares a dependence on the module indicated by this entry.
            const TRANSITIVE = 0x0020;
            /// Indicates that this dependence is mandatory in the static phase.
            const STATIC_PHASE = 0x0040;
            /// Indicates that this dependence was not explicitly or implicitly declared
            /// in the source of the module declaration.
            const SYNTHETIC = 0x1000;
            /// Indicates that this dependence was implicitly declared in the source of the module declaration.
            const MANDATED = 0x8000;
        }
    }

    /// Package related to a module.
    #[derive(Debug, Clone, PartialEq, Eq)]
    pub struct PackageRelation<'a> {
        /// The package index to constant pool. (`Package`)
        pub index: NonZero<u16>,
        /// Properties of the package relation.
        pub flags: PackageFlags,
        /// The destination module indices to constant pool. (`Module`)
        pub dst_indices: Cow<'a, [NonZero<u16>]>,
    }

    bitflags! {
        /// Denote properties of a module dependency.
        #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
        pub struct PackageFlags: u16 {
            /// Indicates that this export was not explicitly or implicitly declared in the source of the module declaration.
            const STATIC_PHASE = 0x0040;
            /// Indicates that this package was implicitly declared in the source of the module declaration.
            const MANDATED = 0x8000;
        }
    }

    /// A service implementation for a given service interface in a module.
    #[derive(Debug, Clone, PartialEq, Eq)]
    pub struct ModuleProvided<'a> {
        /// The service interface index to constant pool. (`Class`)
        pub index: NonZero<u16>,
        /// The destination service implementation indices to constant pool. (`Class`)
        pub dst_indices: Cow<'a, [NonZero<u16>]>,
    }

    impl<'de> Decode<'de> for ModuleFlags {
        fn decode<B: crate::util::Buf<'de>>(mut buf: B) -> Result<Self, Error> {
            let bits = buf.read()?;
            Ok(Self::from_bits_retain(bits))
        }
    }

    impl Encode for ModuleFlags {
        fn encode<B: crate::util::BufMut>(&self, mut buf: B) -> Result<(), Error> {
            buf.write(self.bits())
        }
    }

    impl<'de> Decode<'de> for ModuleRequiredFlags {
        fn decode<B: crate::util::Buf<'de>>(mut buf: B) -> Result<Self, Error> {
            let bits = buf.read()?;
            Ok(Self::from_bits_retain(bits))
        }
    }

    impl Encode for ModuleRequiredFlags {
        fn encode<B: crate::util::BufMut>(&self, mut buf: B) -> Result<(), Error> {
            buf.write(self.bits())
        }
    }

    impl<'de> Decode<'de> for ModuleRequired {
        fn decode<B: crate::util::Buf<'de>>(mut buf: B) -> Result<Self, Error> {
            Ok(Self {
                index: buf.read()?,
                flags: buf.read()?,
                version_idx: buf.read()?,
            })
        }
    }

    impl Encode for ModuleRequired {
        fn encode<B: crate::util::BufMut>(&self, mut buf: B) -> Result<(), Error> {
            buf.write(self.index)?;
            buf.write(self.flags)?;
            buf.write(self.version_idx)?;
            Ok(())
        }
    }

    impl<'de> Decode<'de> for PackageFlags {
        fn decode<B: crate::util::Buf<'de>>(mut buf: B) -> Result<Self, Error> {
            let bits = buf.read()?;
            Ok(Self::from_bits_retain(bits))
        }
    }

    impl Encode for PackageFlags {
        fn encode<B: crate::util::BufMut>(&self, mut buf: B) -> Result<(), Error> {
            buf.write(self.bits())
        }
    }

    impl<'de> Decode<'de> for PackageRelation<'_> {
        fn decode<B: crate::util::Buf<'de>>(mut buf: B) -> Result<Self, Error> {
            Ok(Self {
                index: buf.read()?,
                flags: buf.read()?,
                dst_indices: {
                    let len: u16 = buf.read()?;
                    let mut vec = Vec::with_capacity(len as usize);
                    for _ in 0..len {
                        vec.push(buf.read()?);
                    }
                    Cow::Owned(vec)
                },
            })
        }
    }

    impl Encode for PackageRelation<'_> {
        fn encode<B: crate::util::BufMut>(&self, mut buf: B) -> Result<(), Error> {
            buf.write(self.index)?;
            buf.write(self.flags)?;
            buf.write(self.dst_indices.len() as u16)?;
            for &val in &*self.dst_indices {
                buf.write(val)?;
            }
            Ok(())
        }
    }

    impl<'de> Decode<'de> for ModuleProvided<'_> {
        fn decode<B: crate::util::Buf<'de>>(mut buf: B) -> Result<Self, Error> {
            Ok(Self {
                index: buf.read()?,
                dst_indices: {
                    let len: u16 = buf.read()?;
                    let mut vec = Vec::with_capacity(len as usize);
                    for _ in 0..len {
                        vec.push(buf.read()?);
                    }
                    Cow::Owned(vec)
                },
            })
        }
    }

    impl Encode for ModuleProvided<'_> {
        fn encode<B: crate::util::BufMut>(&self, mut buf: B) -> Result<(), Error> {
            buf.write(self.index)?;
            buf.write(self.dst_indices.len() as u16)?;
            for &val in &*self.dst_indices {
                buf.write(val)?;
            }
            Ok(())
        }
    }

    impl<'de> Decode<'de> for Module<'_> {
        fn decode<B: crate::util::Buf<'de>>(mut buf: B) -> Result<Self, Error> {
            Ok(Self {
                name_idx: buf.read()?,
                flags: buf.read()?,
                version_idx: buf.read()?,
                requires: {
                    let len: u16 = buf.read()?;
                    let mut vec = Vec::with_capacity(len as usize);
                    for _ in 0..len {
                        vec.push(buf.read()?);
                    }
                    Cow::Owned(vec)
                },
                exports: {
                    let len: u16 = buf.read()?;
                    let mut vec = Vec::with_capacity(len as usize);
                    for _ in 0..len {
                        vec.push(buf.read()?);
                    }
                    Cow::Owned(vec)
                },
                opens: {
                    let len: u16 = buf.read()?;
                    let mut vec = Vec::with_capacity(len as usize);
                    for _ in 0..len {
                        vec.push(buf.read()?);
                    }
                    Cow::Owned(vec)
                },
                uses: {
                    let len: u16 = buf.read()?;
                    let mut vec = Vec::with_capacity(len as usize);
                    for _ in 0..len {
                        vec.push(buf.read()?);
                    }
                    Cow::Owned(vec)
                },
                provides: {
                    let len: u16 = buf.read()?;
                    let mut vec = Vec::with_capacity(len as usize);
                    for _ in 0..len {
                        vec.push(buf.read()?);
                    }
                    Cow::Owned(vec)
                },
            })
        }
    }

    impl Encode for Module<'_> {
        fn encode<B: crate::util::BufMut>(&self, mut buf: B) -> Result<(), Error> {
            buf.write(self.name_idx)?;
            buf.write(self.flags)?;
            buf.write(self.version_idx)?;

            buf.write(self.requires.len() as u16)?;
            for val in &*self.requires {
                buf.write(val)?;
            }
            buf.write(self.exports.len() as u16)?;
            for val in &*self.exports {
                buf.write(val)?;
            }
            buf.write(self.opens.len() as u16)?;
            for val in &*self.opens {
                buf.write(val)?;
            }
            buf.write(self.uses.len() as u16)?;
            for val in &*self.uses {
                buf.write(val)?;
            }
            buf.write(self.provides.len() as u16)?;
            for val in &*self.provides {
                buf.write(val)?;
            }
            Ok(())
        }
    }

    /// A record component of a class.
    #[derive(Debug, Clone, PartialEq, Eq)]
    pub struct ComponentInfo<'a> {
        /// Index of name in constant pool. (`Utf8`)
        pub name_idx: NonZero<u16>,
        /// Index of descriptor in constant pool. (`Utf8`)
        pub desc_idx: NonZero<u16>,
        /// Additional attributes.
        pub attributes: Cow<'a, [Attribute<'a>]>,
    }

    impl<'de> Decode<'de> for ComponentInfo<'de> {
        fn decode<B: crate::util::Buf<'de>>(mut buf: B) -> Result<Self, Error> {
            Ok(Self {
                name_idx: buf.read()?,
                desc_idx: buf.read()?,
                attributes: {
                    let len: u16 = buf.read()?;
                    let mut vec = Vec::with_capacity(len as usize);
                    for _ in 0..len {
                        vec.push(buf.read()?);
                    }
                    Cow::Owned(vec)
                },
            })
        }
    }

    impl Encode for ComponentInfo<'_> {
        fn encode<B: crate::util::BufMut>(&self, mut buf: B) -> Result<(), Error> {
            buf.write(self.name_idx)?;
            buf.write(self.desc_idx)?;
            buf.write(self.attributes.len() as u16)?;
            for val in &*self.attributes {
                buf.write(val)?;
            }
            Ok(())
        }
    }

    /// Name of `Record` attribute.
    /// See [`RecordReader`] and [`RecordWriter`] for usage.
    pub const NAME_RECORD: &str = "Record";
    /// Reader of `Record` attribute.
    pub type RecordReader<'a, B> = ArrayReader<'a, B, ComponentInfo<'a>>;
    /// Writer of `Record` attribute.
    pub type RecordWriter<'env, B> = ArrayWriter<B, ComponentInfo<'env>>;
}
