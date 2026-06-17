#![allow(missing_docs)]

use std::io::{Read as _, Write as _, stdin, stdout};

use classfile::{Reader, Writer};

fn main() {
    let mut input = Vec::new();
    stdin().read_to_end(&mut input).unwrap();

    let mut src = &*input;
    let reader = Reader::new(&mut src);

    let mut dst: Vec<u8> = Vec::new();
    let writer = Writer::new(&mut dst);

    let (header, mut reader) = reader.header().unwrap();
    let mut writer = writer.set_header(&header).unwrap();

    while let Some((_, entry)) = reader.next().transpose().unwrap() {
        writer.push(&entry).unwrap();
    }
    let reader = reader.finish().unwrap();
    let writer = writer.finish().unwrap();

    let (metadata, mut reader) = reader.metadata().unwrap();
    let mut writer = writer.set_metadata(&metadata).unwrap();

    while let Some(interface) = reader.next().transpose().unwrap() {
        writer.push(interface).unwrap();
    }
    let mut reader = reader.finish().unwrap();
    let writer = writer.finish().unwrap();

    let mut writer = Some(writer);
    while let Some(mut field) = reader.next().transpose().unwrap() {
        let mut w = writer.take().unwrap().set_header(field.header()).unwrap();
        while let Some(attr) = field.next().transpose().unwrap() {
            w.push(&attr).unwrap();
        }
        writer = Some(w.finish().unwrap())
    }
    let mut reader = reader.finish().unwrap();
    let writer = writer.unwrap().finish().unwrap();

    let mut writer = Some(writer);
    while let Some(mut method) = reader.next().transpose().unwrap() {
        let mut w = writer.take().unwrap().set_header(method.header()).unwrap();
        while let Some(attr) = method.next().transpose().unwrap() {
            w.push(&attr).unwrap();
        }
        writer = Some(w.finish().unwrap())
    }
    let mut reader = reader.finish().unwrap();
    let mut writer = writer.unwrap().finish().unwrap();

    while let Some(attr) = reader.next().transpose().unwrap() {
        writer.push(&attr).unwrap();
    }
    writer.finish().unwrap();

    stdout().write_all(&dst).unwrap();
}
