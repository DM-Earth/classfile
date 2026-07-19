#![allow(missing_docs)]

use std::io::{Read as _, stdin};

use classfile::Reader;

extern crate std;

fn main() {
    let mut input = Vec::new();
    stdin().read_to_end(&mut input).unwrap();
    let reader = Reader::new(&*input);
    let (header, mut reader) = reader.header().unwrap();
    dbg!(header);
    while let Some((idx, entry)) = reader.next().transpose().unwrap() {
        dbg!((idx, entry));
    }
    let reader = reader.finish().unwrap();
    let (metadata, mut reader) = reader.metadata().unwrap();
    dbg!(metadata);
    while let Some(interface) = reader.next().transpose().unwrap() {
        dbg!(interface);
    }
    let mut reader = reader.finish().unwrap();
    while let Some(mut field) = reader.next().transpose().unwrap() {
        dbg!(field.header());
        while let Some(attrs) = field.next().transpose().unwrap() {
            dbg!(attrs);
        }
    }
    let mut reader = reader.finish().unwrap();
    while let Some(mut method) = reader.next().transpose().unwrap() {
        dbg!(method.header());
        while let Some(attrs) = method.next().transpose().unwrap() {
            dbg!(attrs);
        }
    }
    let mut reader = reader.finish().unwrap();
    while let Some(attr) = reader.next().transpose().unwrap() {
        dbg!(attr);
    }
}
