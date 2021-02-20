use std::sync::Arc;
use bytes::BufMut;
use async_std::task;
use std::{thread};
use futures::io::{AsyncWriteExt, AsyncReadExt};

use fixed_ring_buffer::async_ring_buffer::{RingBufferReader, RingBufferWriter, RingBuffer};

fn main() {
    let ring_buffer = Arc::new(RingBuffer::new(32960));

    let mut reader = RingBufferReader::new(ring_buffer.clone());
    let mut writer = RingBufferWriter::new(ring_buffer.clone());

    let t1 = thread::spawn(move ||{
        let handle = task::spawn(async move {
        let mut length = 0 as usize;
        let mut contents: Vec<u8> = Vec::with_capacity(16096);
        contents.resize(16096, 0);
        loop {
            match reader.read(&mut contents).await {
            Ok(size) => {
                if size > 0 {
                length += size;
                } else {
                break;
                }
            },
            Err(e) => {
                panic!("read err = {}", e);
            },
            }
        }

        println!("length = {}", length);
        });

        task::block_on(handle);
    });

    let t2 = thread::spawn(move ||{
        let handle = task::spawn(async move {
        let mut length = 0 as usize;
        let mut contents: Vec<u8> = Vec::new();
        contents.put("warning: unused std::result::Result that must be used warning: unused std::result::Result that must be usedwarning: unused std::result::Result that must be usedwarning: unused std::result::Result that must be usedwarning: unused std::result::Result that must be usedwarning: unused std::result::Result that must be usedwarning: unused std::result::Result that must be usedwarning: unused std::result::Result that must be usedwarning: unused std::result::Result that must be usedwarning: unused std::result::Result that must be usedwarning: unused std::result::Result that must be usedwarning: unused std::result::Result that must be usedwarning: unused std::result::Result that must be usedwarning: unused std::result::Result that must be usedwarning: unused std::result::Result that must be usedwarning: unused std::result::Result that must be usedwarning: unused std::result::Result that must be usedwarning: unused std::result::Result that must be usedwarning: unused std::result::Result that must be usedwarning: unused std::result::Result that must be usedwarning: unused std::result::Result that must be usedwarning: unused std::result::Result that must be usedwarning: unused std::result::Result that must be usedwarning: unused std::result::Result that must be usedwarning: unused std::result::Result that must be usedwarning: unused std::result::Result that must be usedwarning: unused std::result::Result that must be usedwarning: unused std::result::Result that must be usedwarning: unused std::result::Result that must be usedwarning: unused std::result::Result that must be usedwarning: unused std::result::Result that must be usedwarning: unused std::result::Result that must be usedwarning: unused std::result::Result that must be usedwarning: unused std::result::Result that must be usedwarning: unused std::result::Result that must be usedwarning: unused std::result::Result that must be usedwarning: unused std::result::Result that must be usedwarning: unused std::result::Result that must be usedwarning: unused std::result::Result that must be usedwarning: unused std::result::Result that must be usedwarning: unused std::result::Result that must be usedwarning: unused std::result::Result that must be used".as_bytes());
        for i in 0..102400 as usize {
            match writer.write_all(&mut contents).await {
            Ok(()) => {
                length += contents.len();
            },
            Err(e) => {
                panic!("write err = {} index = {}", e, i);
            },
            }
        }
        println!("length = {}", length);
        });

        task::block_on(handle);
    });

    t1.join().unwrap();
    t2.join().unwrap();
}

