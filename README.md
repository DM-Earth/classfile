# classfile

Low-level but easy-to-use reader and writer for the **JVM classfile** format.

Both the reader and the writer expose a stream-based, visitor-style API: you
walk the classfile section by section and decide for each item whether to
inspect it, transform it, or simply pass it through. Structured types for every
section and for the supported attributes are provided on top of that, so you
get most of the ergonomics of a tree API while keeping the memory profile of a
streaming one.

See [`examples/print.rs`](examples/print.rs) for a read-only dump of every
section, and [`examples/pipe.rs`](examples/pipe.rs) for a read-then-write round
trip that copies a classfile from stdin to stdout.

## Feature flags

The crate is `no_std` in every configuration. With `alloc` enabled the `alloc`
crate is additionally linked, which provides:

- `alloc::vec::Vec<u8>` as a writing buffer, so a classfile can be written
  straight into a `Vec<u8>` as shown in [`examples/pipe.rs`](examples/pipe.rs).
- Support for some attributes that is hard to present without vectors.
