use core::num::NonZero;

use crate::Error;

pub trait Decode<'de>: Sized {
    fn decode<B: Buf<'de>>(buf: B) -> Result<Self, Error>;
}

pub trait Encode {
    fn encode<B: BufMut>(&self, buf: B) -> Result<(), Error>;
}

pub trait Buf<'de> {
    fn read_to_slice(&mut self, dst: &mut [u8]) -> usize;

    fn read_slice(&mut self, len: usize) -> Option<&'de [u8]>;

    #[inline]
    fn read_bytes<const N: usize>(&mut self) -> Option<[u8; N]> {
        let mut buf = [0; N];
        let n = self.read_to_slice(&mut buf);
        (n == N).then_some(buf)
    }

    #[inline]
    fn read<T: Decode<'de>>(&mut self) -> Result<T, Error> {
        T::decode(self)
    }
}

pub trait BufMut {
    type Chunk;
    type ChunkBuf<'a>: BufMut
    where
        Self: 'a;

    fn write_from_slice(&mut self, src: &[u8]) -> usize;

    #[inline]
    fn write_bytes<const N: usize>(&mut self, bytes: [u8; N]) -> bool {
        self.write_from_slice(&bytes) == N
    }

    #[inline]
    fn write<T: Encode>(&mut self, value: T) -> Result<(), Error> {
        value.encode(self)
    }

    fn reserve_chunk(&mut self, len: usize) -> Option<Self::Chunk>;

    fn write_chunk<'env, F, U>(&'env mut self, chunk: Self::Chunk, f: F) -> Result<U, Error>
    where
        F: FnOnce(Self::ChunkBuf<'env>) -> Result<U, Error>;
}

impl<'de, B: Buf<'de> + ?Sized> Buf<'de> for &mut B {
    #[inline]
    fn read<T: Decode<'de>>(&mut self) -> Result<T, Error> {
        B::read(self)
    }

    #[inline]
    fn read_bytes<const N: usize>(&mut self) -> Option<[u8; N]> {
        B::read_bytes(self)
    }

    #[inline]
    fn read_to_slice(&mut self, dst: &mut [u8]) -> usize {
        B::read_to_slice(self, dst)
    }

    #[inline]
    fn read_slice(&mut self, len: usize) -> Option<&'de [u8]> {
        B::read_slice(self, len)
    }
}

impl<T: Encode + ?Sized> Encode for &T {
    #[inline]
    fn encode<B: BufMut>(&self, buf: B) -> Result<(), Error> {
        T::encode(self, buf)
    }
}

impl<B: BufMut + ?Sized> BufMut for &mut B {
    #[inline]
    fn write_from_slice(&mut self, src: &[u8]) -> usize {
        B::write_from_slice(self, src)
    }

    #[inline]
    fn write_bytes<const N: usize>(&mut self, bytes: [u8; N]) -> bool {
        B::write_bytes(self, bytes)
    }

    #[inline]
    fn write<T: Encode>(&mut self, value: T) -> Result<(), Error> {
        B::write(self, value)
    }

    type Chunk = B::Chunk;

    type ChunkBuf<'a>
        = B::ChunkBuf<'a>
    where
        Self: 'a;

    #[inline]
    fn reserve_chunk(&mut self, len: usize) -> Option<Self::Chunk> {
        B::reserve_chunk(self, len)
    }

    #[inline]
    fn write_chunk<'env, F, U>(&'env mut self, chunk: Self::Chunk, f: F) -> Result<U, Error>
    where
        F: FnOnce(Self::ChunkBuf<'env>) -> Result<U, Error>,
    {
        B::write_chunk(self, chunk, f)
    }
}

impl<'de> Buf<'de> for &'de [u8] {
    #[inline]
    fn read_bytes<const N: usize>(&mut self) -> Option<[u8; N]> {
        let (chunk, rem) = self.split_first_chunk::<N>()?;
        *self = rem;
        Some(*chunk)
    }

    fn read_to_slice(&mut self, dst: &mut [u8]) -> usize {
        if dst.len() <= self.len() {
            let (chunk, rem) = unsafe { self.split_at_unchecked(dst.len()) };
            dst.copy_from_slice(chunk);
            *self = rem;
            dst.len()
        } else {
            let (dst, _) = unsafe { dst.split_at_mut_unchecked(self.len()) };
            dst.copy_from_slice(self);
            *self = &[];
            dst.len()
        }
    }

    fn read_slice(&mut self, len: usize) -> Option<&'de [u8]> {
        (self.len() >= len).then(|| {
            let (chunk, rem) = unsafe { self.split_at_unchecked(len) };
            *self = rem;
            chunk
        })
    }
}

impl<'a> BufMut for &'a mut [u8] {
    fn write_from_slice(&mut self, src: &[u8]) -> usize {
        if self.len() >= src.len() {
            let (dst, rem) = core::mem::take(self).split_at_mut(src.len());
            dst.copy_from_slice(src);
            *self = rem;
            src.len()
        } else {
            let (src, _) = src.split_at(self.len());
            self.copy_from_slice(src);
            let len = self.len();
            *self = &mut [];
            len
        }
    }

    #[inline]
    fn write_bytes<const N: usize>(&mut self, bytes: [u8; N]) -> bool {
        let Some((arr, rem)) = core::mem::take(self).split_first_chunk_mut() else {
            return false;
        };
        *arr = bytes;
        *self = rem;
        true
    }

    type Chunk = &'a mut [u8];

    type ChunkBuf<'env>
        = &'a mut [u8]
    where
        Self: 'env;

    fn reserve_chunk(&mut self, len: usize) -> Option<Self::Chunk> {
        (self.len() >= len).then(|| {
            let (chunk, rem) = core::mem::take(self).split_at_mut(len);
            *self = rem;
            chunk
        })
    }

    #[inline]
    fn write_chunk<'env, F, U>(&'env mut self, chunk: Self::Chunk, f: F) -> Result<U, Error>
    where
        F: FnOnce(Self::ChunkBuf<'env>) -> Result<U, Error>,
    {
        f(chunk)
    }
}

#[cfg(feature = "alloc")]
#[derive(Debug)]
pub struct VecChunk {
    idx: usize,
    len: usize,
}

#[cfg(feature = "alloc")]
impl BufMut for alloc::vec::Vec<u8> {
    #[inline]
    fn write_from_slice(&mut self, src: &[u8]) -> usize {
        self.extend_from_slice(src);
        src.len()
    }

    type Chunk = VecChunk;

    type ChunkBuf<'a>
        = &'a mut [u8]
    where
        Self: 'a;

    #[inline]
    fn reserve_chunk(&mut self, len: usize) -> Option<Self::Chunk> {
        const PAD: [u8; 8] = [0; 8];
        let idx = self.len();
        for _ in 0..(len / PAD.len()) {
            self.extend_from_slice(&PAD);
        }
        self.extend(core::iter::repeat_n(0, len % PAD.len()));
        Some(VecChunk { idx, len })
    }

    #[inline]
    fn write_chunk<'env, F, U>(&'env mut self, chunk: Self::Chunk, f: F) -> Result<U, Error>
    where
        F: FnOnce(Self::ChunkBuf<'env>) -> Result<U, Error>,
    {
        if chunk.len == 0 {
            f(&mut [])
        } else if self.len() >= chunk.idx + chunk.len {
            f(&mut self[chunk.idx..chunk.idx + chunk.len])
        } else {
            Err(Error::OutOfBounds)
        }
    }
}

macro_rules! edcode_primitive {
    ($($t:ty),*$(,)?) => {
        $(
        impl<'de> Decode<'de> for $t {
            #[inline]
            fn decode<B: Buf<'de>>(mut buf: B) -> Result<Self, Error> {
                buf.read_bytes()
                    .map(Self::from_be_bytes)
                    .ok_or(Error::UnexpectedEOF)
            }
        }

        impl Encode for $t {
            #[inline]
            fn encode<B: BufMut>(&self, mut buf: B) -> Result<(), Error> {
                buf.write_bytes(self.to_be_bytes())
                    .ok_or(Error::UnexpectedEOF)
            }
        }
        )*
    };
}

edcode_primitive! {
    u8, u16, u32, u64, u128, usize,
    i8, i16, i32, i64, i128, isize,
    f32, f64,
}

macro_rules! edcode_nonzero_primitive {
    ($($t:ty),*$(,)?) => {
        $(
        impl Encode for NonZero<$t> {
            #[inline]
            fn encode<B: BufMut>(&self, buf: B) -> Result<(), Error> {
                self.get().encode(buf)
            }
        }

        impl Encode for Option<NonZero<$t>> {
            #[inline]
            fn encode<B: BufMut>(&self, buf: B) -> Result<(), Error> {
                self.map_or(0, NonZero::get).encode(buf)
            }
        }

        impl<'de> Decode<'de> for NonZero<$t> {
            #[inline]
            fn decode<B: Buf<'de>>(mut buf: B) -> Result<Self, Error> {
                buf.read::<$t>().and_then(|i| NonZero::new(i).ok_or(Error::UnexpectedZero))
            }
        }

        impl<'de> Decode<'de> for Option<NonZero<$t>> {
            #[inline]
            fn decode<B: Buf<'de>>(mut buf: B) -> Result<Self, Error> {
                buf.read::<$t>().map(NonZero::new)
            }
        }
        )*
    }
}

edcode_nonzero_primitive! {
    u8, u16, u32, u64, u128, usize,
    i8, i16, i32, i64, i128, isize,
}

/// Reading buffer.
pub trait SealedBuf<'de>: Buf<'de> {}

/// Writing buffer.
pub trait SealedBufMut: BufMut {}

impl<'de, T: SealedBuf<'de> + ?Sized> SealedBuf<'de> for &mut T {}
impl<T: SealedBufMut + ?Sized> SealedBufMut for &mut T {}

impl<'de> SealedBuf<'de> for &'de [u8] {}

impl SealedBufMut for &mut [u8] {}
#[cfg(feature = "alloc")]
impl SealedBufMut for alloc::vec::Vec<u8> {}
