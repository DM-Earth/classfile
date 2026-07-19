# classfile

Low-level but easy-to-use reader and writer for JVM bytecode.

This crate is `no_std` with optional `alloc` dependency required by some attributes.

Both reader and writer allow you to interact with the bytecode in a structured way while
still sticked to a stream-based API, taking advantage from both Visitor and Tree API of ASM.
See examples (or docs) for the use of it.
