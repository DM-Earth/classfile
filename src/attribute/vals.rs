use core::{marker::PhantomData, num::NonZero, ops::Range};

use crate::{
    Attribute, Buf, BufMut, Error,
    reader::{self, phases::RawList},
    util::{BufMut as _, Decode, Encode},
    writer,
};

macro_rules! simple_rw {
    () => {
        /// Writes this attribute into given buffer.
        #[inline]
        pub fn write<B: BufMut>(&self, mut buf: B) -> Result<(), Error> {
            buf.write(self)
        }

        /// Reads the attribute from given buffer.
        #[inline]
        pub fn read<'a, B: Buf<'a>>(mut buf: B) -> Result<Self, Error> {
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

/// Reader of an array.
#[derive(Debug)]
pub struct ArrayReader<'a, B, T> {
    buf: B,
    list: RawList,
    _ghost: PhantomData<(&'a (), &'a T)>,
}

impl<'a, B, T> ArrayReader<'a, B, T>
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

impl<'a, B, T> Iterator for ArrayReader<'a, B, T>
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

impl<'a, B, T> ExactSizeIterator for ArrayReader<'a, B, T>
where
    B: Buf<'a>,
    T: Decode<'a>,
{
}

/// Writer of an array.
#[derive(Debug)]
pub struct ArrayWriter<B, T>
where
    B: BufMut,
{
    buf: B,
    chunk: Option<B::Chunk>,
    count: u16,
    _ghost: PhantomData<T>,
}

impl<B, T> ArrayWriter<B, T>
where
    B: BufMut,
{
    /// Creates a new writer.
    pub fn new(mut buf: B) -> Result<Self, Error> {
        let chunk = buf
            .reserve_chunk(size_of::<u16>())
            .ok_or(Error::UnexpectedEOF)?;
        Ok(Self {
            buf,
            chunk: Some(chunk),
            count: 0,
            _ghost: PhantomData,
        })
    }
}

impl<B, T> ArrayWriter<B, T>
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

impl<B, T> ArrayWriter<B, T>
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

impl<B, T> Drop for ArrayWriter<B, T>
where
    B: BufMut,
{
    fn drop(&mut self) {
        if let Some(chunk) = self.chunk.take() {
            let _ = self.buf.write_chunk(chunk, |mut b| b.write(self.count));
        }
    }
}

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

use bitflags::bitflags;
#[cfg(feature = "alloc")]
pub use need_alloc::*;

#[cfg(feature = "alloc")]
mod need_alloc {
    use core::num::NonZero;

    use alloc::{borrow::Cow, vec::Vec};
    use arrayvec::ArrayVec;

    use crate::{
        Error,
        attributes::{ArrayReader, ArrayWriter},
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
    #[derive(Debug, Clone, PartialEq, Eq)]
    pub struct Annotation<'a> {
        /// The field descriptor (`Utf8`) index in constant pool.
        pub type_idx: NonZero<u16>,
        /// Element-value pairs in the annotation represented by this annotation structure.
        ///
        /// The index is for the name of element of that value (`Utf8`) in constant pool.
        pub pairs: Cow<'a, [(NonZero<u16>, ElementValue<'a>)]>,
    }

    /// Value of an element-value pair.
    #[derive(Debug, Clone, PartialEq, Eq)]
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
                pairs.push((buf.read()?, buf.read()?));
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
            for (name_idx, val) in &*self.pairs {
                buf.write(name_idx)?;
                buf.write(val)?;
            }
            Ok(())
        }
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
}
