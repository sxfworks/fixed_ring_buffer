# Async lock-free Ringbuffer

[![Build](https://github.com/sxfworks/fixed_ring_buffer/workflows/build-and-test/badge.svg)](
https://github.com/sxfworks/fixed_ring_buffer/actions)
[![License](https://img.shields.io/badge/license-MIT-blue.svg)](
https://github.com/sxfworks/fixed_ring_buffer)
[![Cargo](https://img.shields.io/crates/v/fixed_ring_buffer.svg)](
https://crates.io/crates/fixed_ring_buffer)
[![Documentation](https://docs.rs/fixed_ring_buffer/badge.svg)](
https://docs.rs/fixed_ring_buffer)

FixedRingBuffer is an asynchronous SPSC fixed-capacity look-free ring buffer, which can be used to transfer data between two threads or between two asynchronous tasks. The overall feature is as follows:
* lock-free
* spsc
* Use a fixed-capacity buffer, and support buffer internal memory recycle
* Implement AsyncRead and AsyncBufRead trait for reader
* Implements AsyncWrite trait for writer

Quick Start
------------
Add the dependency package of async_ring_buffer in Cargo.toml as follows:
```rust
[dependencies]
fixed_ring_buffer = "0.1.0"
```

Use AsyncRead, AsyncBufRead and AsyncWrite external methods to operate AsyncRingBuffer as follows:
```rust
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
        contents.put("xxxxxnnnnnmmmmmmmmsssss".as_bytes());
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
```