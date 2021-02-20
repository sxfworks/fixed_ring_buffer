use std::sync::Arc;
use bytes::BufMut;
use async_std::task;
use std::{thread};
use futures::io::{AsyncWriteExt};
use futures_lite::future;

use fixed_ring_buffer::async_ring_buffer::{RingBufferReader, RingBufferWriter, RingBuffer};

fn main() {
    let thread_handle = thread::spawn(move ||{
    for i in 0..1024 {
        let ring_buffer = Arc::new(RingBuffer::new(64));
        let reader = RingBufferReader::new(ring_buffer.clone());
        let mut writer = RingBufferWriter::new(ring_buffer.clone());
        let content: String = "warning: unused std::result::Result unused std::resulunused std::result::Result that must be used which should be unused std::result::Result that must be used which should be unused std::result::Result that must be used which should be unused std::result::Result that must be used which should be unused std::result::Result that must be used which should be unused std::result::Result that must be used which should be unused std::result::Result that must be used which should be unused std::result::Result that must be used which should be unused std::result::Result that must be used which should be unused std::result::Result that must be used which should be unused std::result::Result that must be used which should be unused std::result::Result that must be used which should be unused std::result::Result that must be used which should be unused std::result::Result that must be used which should be unused std::result::Result that must be used which should be unused std::result::Result that must be used which should be unused std::result::Result that must be used which should be unused std::result::Result that must be used which should be unused std::result::Result that must be used which should be unused std::result::Result that must be used which should be unused std::result::Result that must be used which should be unused std::result::Result that must be used which should be unused std::result::Result that must be used which should be unused std::result::Result that must be used which should be unused std::result::Result that must be used which should be unused std::result::Result that must be used which should be unused std::result::Result that must be used which should be unused std::result::Result that must be used which should be unused std::result::Result that must be used which should be unused std::result::Result that must be used which should be unused std::result::Result that must be used which should be unused std::result::Result that must be used which should be unused std::result::Result that must be used which should be unused std::result::Result that must be used which should be unused std::result::Result that must be used which should be unused std::result::Result that must be used which should be unused std::result::Result that must be used which should be unused std::result::Result that must be used which should be unused std::result::Result that must be used which should be t::Result that must be used which should be that must be used which should be handled which should be handled warning: warning: unused std::result::Result that must be used which should be handled which should be handled warnwarning: unused std::result::Result that must be used which should be handled which should be handled warnwarning: unused std::result::Result that must be used which should be handled which should be handled warnwarning: unused std::result::Result that must be used which should be handled which should be handled warnwarning: unused std::result::Result that must be used which should be handled which should be handled warning: 7 warnings emitted help: if this is intentional,warning: unused std::result::Result that must be used which should be handled which should be handled warning: 7 warnings emitted help: if this is intentional,warning: unused std::result::Result that must be used which should be handled which should be handled warning: 7 warnings emitted help: if this is intentional,warning: unused std::result::Result that must be used which should be handled which should be handled warning: 7 warnings emitted help: if this is intentional,warning: unused std::result::Result that must be used which should be handled which should be handled warning: 7 warnings emitted help: if this is intentional,warning: unused std::result::Result that must be used which should be handled which should be handled warning: 7 warnings emitted help: if this is intentional,warning: unused std::result::Result that must be used which should be handled which should be handled warning: 7 warnings emitted help: if this is intentional,warning: unused std::result::Result that must be used which should be handled which should be handled warning: 7 warnings emitted help: if this is intentional,warning: unused std::result::Result that must be used which should be handled which should be handled warning: 7 warnings emitted help: if this is intentional,warning: unused std::result::Result that must be used which should be handled which should be handled warning: 7 warnings emitted help: if this is intentional,warning: unused std::result::Result that must be used which should be handled which should be handled warning: 7 warnings emitted help: if this is intentional,warning: unused std::result::Result that must be used which should be handled which should be handled warning: 7 warnings emitted help: if this is intentional,warning: unused std::result::Result that must be used which should be handled which should be handled warning: 7 warnings emitted help: if this is intentional,warning: unused std::result::Result that must be used which should be handled which should be handled warning: 7 warnings emitted help: if this is intentional,warning: unused std::result::Result that must be used which should be handled which should be handled warning: 7 warnings emitted help: if this is intentional,warning: unused std::result::Result that must be used which should be handled which should be handled warning: 7 warnings emitted help: if this is intentional,warning: unused std::result::Result that must be used which should be handled which should be handled warning: 7 warnings emitted help: if this is intentional,warning: unused std::result::Result that must be used which should be handled which should be handled warning: 7 warnings emitted help: if this is intentional,warning: unused std::result::Result that must be used which should be handled which should be handled warning: 7 warnings emitted help: if this is intentional,warning: unused std::result::Result that must be used which should be handled which should be handled warning: 7 warnings emitted help: if this is intentional,warning: unused std::result::Result that must be used which should be handled which should be handled warning: 7 warnings emitted help: if this is intentional,warning: unused std::result::Result that must be used which should be handled which should be handled warning: 7 warnings emitted help: if this is intentional,warning: unused std::result::Result that must be used which should be handled which should be handled warning: 7 warnings emitted help: if this is intentional,warning: unused std::result::Result that must be used which should be handled which should be handled warning: 7 warnings emitted help: if this is intentional,warning: unused std::result::Result that must be used which should be handled which should be handled warning: 7 warnings emitted help: if this is intentional,warning: unused std::result::Result that must be used which should be handled which should be handled warning: 7 warnings emitted help: if this is intentional,ing: 7 warnings emitted help: if this is intentional,ing: 7 warnings emitted help: if this is intentional,ing: 7 warnings emitted help: if this is intentional,ing: 7 warnings emitted help: if this is intentional,7 warnings emitted help: if this is intentional, prefix it with an underscore:".to_string() + &i.to_string();
        let content_read = content.clone();
        let content_length = content.len();

        let read_handle = task::spawn(async move {
        let mut length = 0 as usize;
        let mut raw_content: Vec<u8> = Vec::new();
        raw_content.resize(content_length, 0);
        let mut buf_writer = futures::io::Cursor::new(raw_content);
        match futures::io::copy(reader, &mut buf_writer).await {
            Ok(size) => {
            if size > 0 {
                length += size as usize;
            }
            },
            Err(e) => {
            panic!("read err = {}", e);
            },
        }

        assert_eq!(length, content_length);
        assert_eq!(buf_writer.into_inner().as_slice(), content_read.as_bytes());
        });

        let write_handle = task::spawn(async move {
        let mut length = 0 as usize;
        let mut contents: Vec<u8> = Vec::new();

        contents.put(content.clone().as_bytes());

        loop {
            match writer.write(&mut contents[length..]).await {
            Ok(size_wrote) => {
                //println!("wrote_size = {}", size_wrote);
                length += size_wrote;
                if size_wrote <= 0 {
                break;
                }
            },
            Err(e) => {
                panic!("write err = {}", e);
            },
            }
        }
        assert_eq!(length, content_length);
        });

        let zip_task = future::zip(read_handle, write_handle);
        task::block_on(zip_task);
    }
    });

    thread_handle.join().unwrap();
}

