use core::{
    fmt::{Debug, Display},
    num::NonZero,
};

use crate::{
    Error, ModifiedUtf8,
    util::{Buf, BufMut, Decode, Encode},
};

/// An entry in the constant pool.
#[derive(Debug, Clone, Copy, PartialEq, PartialOrd)]
#[allow(clippy::exhaustive_enums)]
pub enum ConstantEntry<'a> {
    /// String literal.
    Utf8(ModifiedUtf8<'a>),

    /// Four-byte numeric constant for type `int`.
    Integer(i32),
    /// Four-byte numeric constant for type `float`.
    Float(f32), // same repr in Rust and classfile
    /// Eight-byte numeric constant for type `long`.
    Long(i64),
    /// Eight-byte numeric constant for type `double`.
    Double(f64), // same repr in Rust and classfile

    /// A class or an interface.
    /// The containing number is for index to the name (`Utf8`) in constant pool.
    Class(NonZero<u16>),
    /// A string object.
    /// The containing number is for index to the `Utf8` value in constant pool.
    String(NonZero<u16>),

    /// A field.
    FieldRef {
        /// The index of class or interface (`Class`) containing this field in constant pool.
        class_index: NonZero<u16>,
        /// The index of name and descriptor (`NameAndType`) of this field in constant pool.
        name_and_type_index: NonZero<u16>,
    },
    /// A method.
    MethodRef {
        /// The index of class or interface (`Class`) containing this method in constant pool.
        class_index: NonZero<u16>,
        /// The index of name and descriptor (`NameAndType`) of this method in constant pool.
        name_and_type_index: NonZero<u16>,
    },
    /// An interface method.
    InterfaceMethodRef {
        /// The index of interface (`Class`) containing this method in constant pool.
        class_index: NonZero<u16>,
        /// The index of name and descriptor (`NameAndType`) of this method in constant pool.
        name_and_type_index: NonZero<u16>,
    },

    /// Detailed information of a method or type.
    NameAndType {
        /// The index of name (`Utf8`) in constant pool.
        name_index: NonZero<u16>,
        /// The index of field or method descriptor (`Utf8`) in constant pool.
        descriptor_index: NonZero<u16>,
    },

    /// A method handle.
    MethodHandle {
        /// Denotes the kind of this method handle.
        ref_kind: ReferenceKind,
        /// The index of referenced item. See
        /// [`JVMS 4.4.8`](https://docs.oracle.com/javase/specs/jvms/se26/html/jvms-4.html#jvms-4.4.8)
        /// for information about the use of it.
        ref_index: NonZero<u16>,
    },
    /// A method type.
    /// The containing number is for index to the descriptor (`Utf8`) in constant pool.
    MethodType(NonZero<u16>),

    /// A dynamically-computed constant at runtime.
    Dynamic {
        /// The index of the `bootstrap_methods` array in the bootstrap method table.
        bootstrap_method_attr_index: u16,
        /// The index of name and field descriptor (`NameAndType`) in constant pool.
        name_and_type_index: NonZero<u16>,
    },
    /// A dynamically-computed call site at runtime.
    InvokeDynamic {
        /// The index of the `bootstrap_methods` array in the bootstrap method table.
        bootstrap_method_attr_index: u16,
        /// The index of name and method descriptor (`NameAndType`) in constant pool.
        name_and_type_index: NonZero<u16>,
    },

    /// A module.
    /// The containing number is for index to the name (`Utf8`) in constant pool.
    Module(NonZero<u16>),
    /// A package.
    /// The containing number is for index to the name (`Utf8`) in constant pool.
    Package(NonZero<u16>),
}

impl ConstantEntry<'_> {
    /// The amount of index space this entry takes.
    ///
    /// If the index of this entry is `i` and it took `n` space then
    /// the next available index is `i + n`.
    ///
    /// All entries are `1` except for `Long` and `Double` that are `2`.
    pub fn space(&self) -> u16 {
        if matches!(self, Self::Long(_) | Self::Double(_)) {
            2
        } else {
            1
        }
    }
}

/// Kind of a method handle, charactizing its bytecode behavior.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(u8)]
#[allow(clippy::exhaustive_enums)]
pub enum ReferenceKind {
    /// Get an instance field (`REF_getField`).
    GetField = 1,
    /// Get a static field (`REF_getStatic`).
    GetStatic,
    /// Set an instance field (`REF_putField`).
    PutField,
    /// Set a static field (`REF_putStatic`).
    PutStatic,
    /// Invoke a virtual method (`REF_invokeVirtual`).
    InvokeVirtual,
    /// Invoke a static method (`REF_invokeStatic`).
    InvokeStatic,
    /// Invoke a special/private method or constructor (`REF_invokeSpecial`).
    InvokeSpecial,
    /// Invoke a constructor via `new` + `<init>` dispatch (`REF_newInvokeSpecial`).
    NewInvokeSpecial,
    /// Invoke an interface method (`REF_invokeInterface`).
    InvokeInterface,
}

impl<'de> Decode<'de> for ConstantEntry<'de> {
    fn decode<B: Buf<'de>>(mut buf: B) -> Result<Self, Error> {
        let tag: u8 = buf.read()?;
        match tag {
            1 => Ok(Self::Utf8(buf.read()?)),
            3 => Ok(Self::Integer(buf.read()?)),
            4 => Ok(Self::Float(buf.read()?)),
            5 => Ok(Self::Long(buf.read()?)),
            6 => Ok(Self::Double(buf.read()?)),
            7 => Ok(Self::Class(buf.read()?)),
            8 => Ok(Self::String(buf.read()?)),
            9 => Ok(Self::FieldRef {
                class_index: buf.read()?,
                name_and_type_index: buf.read()?,
            }),
            10 => Ok(Self::MethodRef {
                class_index: buf.read()?,
                name_and_type_index: buf.read()?,
            }),
            11 => Ok(Self::InterfaceMethodRef {
                class_index: buf.read()?,
                name_and_type_index: buf.read()?,
            }),
            12 => Ok(Self::NameAndType {
                name_index: buf.read()?,
                descriptor_index: buf.read()?,
            }),
            15 => Ok(Self::MethodHandle {
                ref_kind: buf.read()?,
                ref_index: buf.read()?,
            }),
            16 => Ok(Self::MethodType(buf.read()?)),
            17 => Ok(Self::Dynamic {
                bootstrap_method_attr_index: buf.read()?,
                name_and_type_index: buf.read()?,
            }),
            18 => Ok(Self::InvokeDynamic {
                bootstrap_method_attr_index: buf.read()?,
                name_and_type_index: buf.read()?,
            }),
            19 => Ok(Self::Module(buf.read()?)),
            20 => Ok(Self::Package(buf.read()?)),
            myth => Err(Error::UnknownConstantTag(myth)),
        }
    }
}

impl Encode for ConstantEntry<'_> {
    fn encode<B: BufMut>(&self, mut buf: B) -> Result<(), Error> {
        match self {
            ConstantEntry::Utf8(slice) => {
                buf.write(1u8)?;
                buf.write(slice)?;
            }
            ConstantEntry::Integer(val) => {
                buf.write(3u8)?;
                buf.write(val)?;
            }
            ConstantEntry::Float(val) => {
                buf.write(4u8)?;
                buf.write(val)?;
            }
            ConstantEntry::Long(val) => {
                buf.write(5u8)?;
                buf.write(val)?;
            }
            ConstantEntry::Double(val) => {
                buf.write(6u8)?;
                buf.write(val)?;
            }
            ConstantEntry::Class(idx) => {
                buf.write(7u8)?;
                buf.write(idx)?;
            }
            ConstantEntry::String(idx) => {
                buf.write(8u8)?;
                buf.write(idx)?;
            }
            ConstantEntry::FieldRef {
                class_index,
                name_and_type_index,
            } => {
                buf.write(9u8)?;
                buf.write(class_index)?;
                buf.write(name_and_type_index)?;
            }
            ConstantEntry::MethodRef {
                class_index,
                name_and_type_index,
            } => {
                buf.write(10u8)?;
                buf.write(class_index)?;
                buf.write(name_and_type_index)?;
            }
            ConstantEntry::InterfaceMethodRef {
                class_index,
                name_and_type_index,
            } => {
                buf.write(11u8)?;
                buf.write(class_index)?;
                buf.write(name_and_type_index)?;
            }
            ConstantEntry::NameAndType {
                name_index,
                descriptor_index,
            } => {
                buf.write(12u8)?;
                buf.write(name_index)?;
                buf.write(descriptor_index)?;
            }
            ConstantEntry::MethodHandle {
                ref_kind,
                ref_index,
            } => {
                buf.write(15u8)?;
                buf.write(ref_kind)?;
                buf.write(ref_index)?;
            }
            ConstantEntry::MethodType(idx) => {
                buf.write(16u8)?;
                buf.write(idx)?;
            }
            ConstantEntry::Dynamic {
                bootstrap_method_attr_index,
                name_and_type_index,
            } => {
                buf.write(17u8)?;
                buf.write(bootstrap_method_attr_index)?;
                buf.write(name_and_type_index)?;
            }
            ConstantEntry::InvokeDynamic {
                bootstrap_method_attr_index,
                name_and_type_index,
            } => {
                buf.write(18u8)?;
                buf.write(bootstrap_method_attr_index)?;
                buf.write(name_and_type_index)?;
            }
            ConstantEntry::Module(idx) => {
                buf.write(19u8)?;
                buf.write(idx)?;
            }
            ConstantEntry::Package(idx) => {
                buf.write(20u8)?;
                buf.write(idx)?;
            }
        }
        Ok(())
    }
}

impl<'de> Decode<'de> for ReferenceKind {
    fn decode<B: Buf<'de>>(mut buf: B) -> Result<Self, Error> {
        let kind: u8 = buf.read()?;
        match kind {
            1 => Ok(Self::GetField),
            2 => Ok(Self::GetStatic),
            3 => Ok(Self::PutField),
            4 => Ok(Self::PutStatic),
            5 => Ok(Self::InvokeVirtual),
            6 => Ok(Self::InvokeStatic),
            7 => Ok(Self::InvokeSpecial),
            8 => Ok(Self::NewInvokeSpecial),
            9 => Ok(Self::InvokeInterface),
            myth => Err(Error::UnknownReferenceKind(myth)),
        }
    }
}

impl Encode for ReferenceKind {
    fn encode<B: BufMut>(&self, mut buf: B) -> Result<(), Error> {
        buf.write(*self as u8)
    }
}

impl Display for ReferenceKind {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            ReferenceKind::GetField => write!(f, "REF_getField"),
            ReferenceKind::GetStatic => write!(f, "REF_getStatic"),
            ReferenceKind::PutField => write!(f, "REF_putField"),
            ReferenceKind::PutStatic => write!(f, "REF_putStatic"),
            ReferenceKind::InvokeVirtual => write!(f, "REF_invokeVirtual"),
            ReferenceKind::InvokeStatic => write!(f, "REF_invokeStatic"),
            ReferenceKind::InvokeSpecial => write!(f, "REF_invokeSpecial"),
            ReferenceKind::NewInvokeSpecial => write!(f, "REF_newInvokeSpecial"),
            ReferenceKind::InvokeInterface => write!(f, "REF_invokeInterface"),
        }
    }
}

impl Debug for ReferenceKind {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "{}", self)
    }
}
