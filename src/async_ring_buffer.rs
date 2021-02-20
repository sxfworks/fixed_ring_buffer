use core::pin::Pin;
use core::task::Context;
use std::cmp::min;
use std::ptr::copy_nonoverlapping;
use std::sync::Arc;
use std::task::{Poll, Waker};
use std::sync::atomic::{AtomicU64, AtomicBool, Ordering};
use futures::task::{AtomicWaker};
use futures::io::{AsyncBufRead, AsyncRead, AsyncWrite,  Result};

use crate::fixed_buffer;

/// Asynchronous RingBuffer with fixed capacity
pub struct RingBuffer {
  buffer: fixed_buffer::FixedBuffer,
  valid_data: AtomicU64,
  write_pos: AtomicU64,
  read_pos: AtomicU64,
  read_waker: AtomicWaker,
  write_waker: AtomicWaker,
  read_shutdown: AtomicBool,
  write_shutdown: AtomicBool,
}

impl RingBuffer {
  const EMPTY_BUFFER: [u8; 0] = [0; 0];

  /// Create a the memory will be released when the ringbuffer is destroyed
  pub fn new(capacity: usize) -> RingBuffer {
    let mut buffer = fixed_buffer::FixedBuffer::alloc(capacity);
    buffer.resize(capacity);
    return RingBuffer {
      buffer: buffer,
      write_pos: AtomicU64::new(0),
      valid_data: AtomicU64::new(0),
      read_pos: AtomicU64::new(0),
      read_waker: AtomicWaker::new(),
      write_waker: AtomicWaker::new(),
      read_shutdown: AtomicBool::new(false),
      write_shutdown: AtomicBool::new(false),
    }; 
  }

  /// Create a fixed-capacity ringbuffer through a fixed-capacity buffer, and the memory occupied by the buffer is controlled by the buffer itself
  pub fn from_fixed_buffer(buffer: fixed_buffer::FixedBuffer) -> RingBuffer {
    return RingBuffer {
      buffer: buffer,
      write_pos: AtomicU64::new(0),
      valid_data: AtomicU64::new(0),
      read_pos: AtomicU64::new(0),
      read_waker: AtomicWaker::new(),
      write_waker: AtomicWaker::new(),
      read_shutdown: AtomicBool::new(false),
      write_shutdown: AtomicBool::new(false),
    }; 
  }

  fn register_read_waker(&self, waker: &Waker) {
    self.read_waker.register(waker);
  }

  fn register_write_waker(&self, waker: &Waker) {
    self.write_waker.register(waker);
  }

  fn wake_writer(&self) {
    match &self.write_waker.take() {
      Some(waker) => {
        waker.wake_by_ref();
      },
      None => {},
    }
  }

  fn wake_reader(&self) {
    match &self.read_waker.take() {
      Some(waker) => {
        waker.wake_by_ref();
      },
      None => {},
    }
  }

  fn write_ptr(&self) -> *mut u8 {
    unsafe {
      let start = self.buffer.raw_data_mut();
      return start.add(self.write_pos.load(Ordering::SeqCst) as usize);
    }
  }

  fn read_ptr(&self) -> *const u8 {
    unsafe {
      let start = self.buffer.raw_data();
      return start.add(self.read_pos.load(Ordering::SeqCst) as usize);
    }
  }
}

/// Writer of asynchronous fixed-capacity ringbuffer
pub struct RingBufferWriter(Arc<RingBuffer>);

impl Drop for RingBufferWriter {
  fn drop(&mut self) {
    let ring_buffer = self.0.clone();
    ring_buffer.write_shutdown.store(true, Ordering::Relaxed);
    ring_buffer.wake_reader();
  }
}

impl RingBufferWriter {
  /// Create a writer for ringbuffer by ringbuffer
  pub fn new(buffer: Arc<RingBuffer>) -> RingBufferWriter {
    return RingBufferWriter(buffer);
  }

  /// Create a writer for ringbuffer by RingBufferReader
  pub fn from_reader(reader: &RingBufferReader) -> RingBufferWriter {
    return RingBufferWriter(reader.0.clone());
  }
}

impl AsyncWrite for RingBufferWriter {
    fn poll_write(self: Pin<&mut Self>, cx: &mut Context, buf: &[u8]) -> Poll<Result<usize>> {
      let ring_buffer = &self.0;

      if ring_buffer.write_shutdown.load(Ordering::Relaxed) {
        return Poll::Ready(Err(futures::io::Error::new(futures::io::ErrorKind::BrokenPipe, "write on ring buffer was closed")));
      }

      ring_buffer.register_write_waker(cx.waker());

      let capacity = ring_buffer.buffer.len();
      let start = ring_buffer.buffer.raw_data_mut();

      if ring_buffer.read_shutdown.load(Ordering::Relaxed) {
        return Poll::Ready(Err(futures::io::Error::new(futures::io::ErrorKind::BrokenPipe, "write on read was closed")));
      }

      let valid_data = ring_buffer.valid_data.load(Ordering::SeqCst) as usize;

      if valid_data == capacity {
        ring_buffer.wake_reader();
        return Poll::Pending;
      }

      let write_pos = ring_buffer.write_pos.load(Ordering::SeqCst) as usize;

      let buf_ptr = buf.as_ptr();
      let write_total = min(buf.len(), capacity - valid_data);

      unsafe {
        if write_pos + write_total < capacity {
          copy_nonoverlapping(buf_ptr, ring_buffer.write_ptr(), write_total);
          assert_eq!(ring_buffer.write_pos.compare_and_swap(write_pos as u64, (write_pos + write_total) as u64, Ordering::SeqCst), write_pos as u64);
        } else {
          let distance_end = capacity - write_pos;
          let remaining: usize = write_total - distance_end;
          copy_nonoverlapping(buf_ptr, ring_buffer.write_ptr(), distance_end);
          copy_nonoverlapping(buf_ptr.add(distance_end), start, remaining);
          assert_eq!(ring_buffer.write_pos.compare_and_swap(write_pos as u64, remaining as u64, Ordering::SeqCst), write_pos as u64);
        }
      }

      ring_buffer.valid_data.fetch_add(write_total as u64, Ordering::SeqCst);
      
      ring_buffer.wake_reader();
      return Poll::Ready(Ok(write_total));
    }

    fn poll_flush(self: Pin<&mut Self>, _: &mut Context) -> Poll<Result<()>> {
      return Poll::Ready(Ok(()));
    }

    fn poll_close(self: Pin<&mut Self>, _: &mut Context) -> Poll<Result<()>> {
      let ring_buffer = self.0.clone();
      ring_buffer.wake_reader();
      ring_buffer.write_shutdown.store(true, Ordering::Relaxed);
      return Poll::Ready(Ok(()));
    }
}

impl AsyncBufRead for RingBufferReader {
  fn poll_fill_buf<'a>(self: Pin<&'a mut Self>, cx: &mut Context) -> Poll<Result<&'a [u8]>> {
    let ring_buffer = self.0.clone();
    let raw_data = ring_buffer.buffer.raw_data();
    let capacity = ring_buffer.buffer.len();

    ring_buffer.register_read_waker(&cx.waker());

    let valid_data = ring_buffer.valid_data.load(Ordering::SeqCst) as usize;

    if valid_data <= 0 {
      if ring_buffer.write_shutdown.load(Ordering::Relaxed) {
        if ring_buffer.valid_data.load(Ordering::SeqCst) == 0 {
          return Poll::Ready(Ok(&RingBuffer::EMPTY_BUFFER[..]));
        } 
      }

      ring_buffer.wake_writer();
      return Poll::Pending;
    }

    let read_pos = ring_buffer.read_pos.load(Ordering::SeqCst) as usize;

    if read_pos + valid_data < capacity {
      return Poll::Ready(Ok(unsafe { core::slice::from_raw_parts(raw_data.add(read_pos), valid_data) }));
    } else {
      let distance_end  = capacity - read_pos;
      return Poll::Ready(Ok(unsafe { core::slice::from_raw_parts(raw_data.add(read_pos), distance_end) }));
    }
  }

  fn consume(self: Pin<&mut Self>, amt: usize) {
    let ring_buffer = self.0.clone();
    let valid_data = ring_buffer.valid_data.load(Ordering::SeqCst) as usize;
    let read_pos = ring_buffer.read_pos.load(Ordering::SeqCst) as usize;

    assert!(amt < valid_data);

    if read_pos + amt < ring_buffer.buffer.len() {
      ring_buffer.read_pos.fetch_add(amt as u64, Ordering::SeqCst);
    } else if read_pos + amt == ring_buffer.buffer.len() {
      ring_buffer.read_pos.store(0, Ordering::SeqCst);
    } else {
      ring_buffer.read_pos.store((read_pos + amt - ring_buffer.buffer.len()) as u64, Ordering::SeqCst);
    }

    ring_buffer.valid_data.fetch_sub(amt as u64, Ordering::SeqCst);

    ring_buffer.wake_writer();
  }
}

/// Reader of asynchronous fixed-capacity ringbuffer
pub struct RingBufferReader(Arc<RingBuffer>);

impl Drop for RingBufferReader {
    fn drop(&mut self) {
        let ring_buffer = self.0.clone();
        ring_buffer.read_shutdown.store(true, Ordering::Relaxed);
        ring_buffer.wake_writer();
    }
}

impl RingBufferReader {
  /// Create a reader for ringbuffer by ringbuffer
  pub fn new(buffer: Arc<RingBuffer>) -> RingBufferReader {
    return RingBufferReader(buffer);
  }

  /// Create a reader for ringbuffer by RingBufferWriter
  pub fn from_writer(writer: &RingBufferWriter) -> RingBufferReader {
    return RingBufferReader(writer.0.clone());
  }
}

impl AsyncRead for RingBufferReader {
    fn poll_read(self: Pin<&mut Self>, cx: &mut Context, buf: &mut [u8]) -> Poll<Result<usize>> {
      let ring_buffer = self.0.clone();

      ring_buffer.register_read_waker(cx.waker());

      let valid_data = ring_buffer.valid_data.load(Ordering::SeqCst) as usize;

      if valid_data <= 0 {
        if ring_buffer.write_shutdown.load(Ordering::SeqCst) {
          if ring_buffer.valid_data.load(Ordering::SeqCst) == 0 {
            return Poll::Ready(Ok(0));
          }
        }
        ring_buffer.wake_writer();
        return Poll::Pending;
      }

      let read_pos = ring_buffer.read_pos.load(Ordering::SeqCst) as usize;

      let capacity = ring_buffer.buffer.len();
      let start = ring_buffer.buffer.raw_data();

      let buf_ptr = buf.as_mut_ptr();
      let read_total = min(buf.len(), valid_data);

      unsafe {
        if read_pos + read_total < capacity {
          copy_nonoverlapping(ring_buffer.read_ptr(), buf_ptr, read_total);
          ring_buffer.read_pos.store((read_pos + read_total) as u64, Ordering::SeqCst);
        } else {
          let distance_end = capacity - read_pos;
          let remaining: usize = read_total - distance_end;
          copy_nonoverlapping(start.add(read_pos), buf_ptr, distance_end);
          copy_nonoverlapping(start, buf_ptr.add(distance_end), remaining);
          ring_buffer.read_pos.store(remaining as u64, Ordering::SeqCst);
        }
      }

      ring_buffer.valid_data.fetch_sub(read_total as u64, Ordering::SeqCst);
      ring_buffer.wake_writer();
      return Poll::Ready(Ok(read_total));
    }
}

#[cfg(test)]
mod unit_tests {
  use std::sync::Arc;
  use bytes::BufMut;
  use async_std::task;
  use std::{thread};
  use futures::io::{AsyncWriteExt, AsyncReadExt};
  use futures_lite::future;

  use crate::async_ring_buffer::{RingBufferReader, RingBufferWriter, RingBuffer};

  #[test]
  fn test_async_ring_buffer() {
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
        for i in 0..1024 as usize {
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

  #[test]
  fn test_async_ring_buffer_write_all_read() {
    let thread_handle = thread::spawn(move ||{
      for i in 0..1024 {
        let ring_buffer = Arc::new(RingBuffer::new(64));
        let mut reader = RingBufferReader::new(ring_buffer.clone());
        let mut writer = RingBufferWriter::new(ring_buffer.clone());
        let content: String = "warning: unused std::result::Result unused std::resulunused std::result::Result that must be used which should be unused std::result::Result that must be used which should be unused std::result::Result that must be used which should be unused std::result::Result that must be used which should be unused std::result::Result that must be used which should be unused std::result::Result that must be used which should be unused std::result::Result that must be used which should be unused std::result::Result that must be used which should be unused std::result::Result that must be used which should be unused std::result::Result that must be used which should be unused std::result::Result that must be used which should be unused std::result::Result that must be used which should be unused std::result::Result that must be used which should be unused std::result::Result that must be used which should be unused std::result::Result that must be used which should be unused std::result::Result that must be used which should be unused std::result::Result that must be used which should be unused std::result::Result that must be used which should be unused std::result::Result that must be used which should be unused std::result::Result that must be used which should be unused std::result::Result that must be used which should be unused std::result::Result that must be used which should be unused std::result::Result that must be used which should be unused std::result::Result that must be used which should be unused std::result::Result that must be used which should be unused std::result::Result that must be used which should be unused std::result::Result that must be used which should be unused std::result::Result that must be used which should be unused std::result::Result that must be used which should be unused std::result::Result that must be used which should be unused std::result::Result that must be used which should be unused std::result::Result that must be used which should be unused std::result::Result that must be used which should be unused std::result::Result that must be used which should be unused std::result::Result that must be used which should be unused std::result::Result that must be used which should be unused std::result::Result that must be used which should be unused std::result::Result that must be used which should be unused std::result::Result that must be used which should be t::Result that must be used which should be that must be used which should be handled which should be handled warning: warning: unused std::result::Result that must be used which should be handled which should be handled warnwarning: unused std::result::Result that must be used which should be handled which should be handled warnwarning: unused std::result::Result that must be used which should be handled which should be handled warnwarning: unused std::result::Result that must be used which should be handled which should be handled warnwarning: unused std::result::Result that must be used which should be handled which should be handled warning: 7 warnings emitted help: if this is intentional,warning: unused std::result::Result that must be used which should be handled which should be handled warning: 7 warnings emitted help: if this is intentional,warning: unused std::result::Result that must be used which should be handled which should be handled warning: 7 warnings emitted help: if this is intentional,warning: unused std::result::Result that must be used which should be handled which should be handled warning: 7 warnings emitted help: if this is intentional,warning: unused std::result::Result that must be used which should be handled which should be handled warning: 7 warnings emitted help: if this is intentional,warning: unused std::result::Result that must be used which should be handled which should be handled warning: 7 warnings emitted help: if this is intentional,warning: unused std::result::Result that must be used which should be handled which should be handled warning: 7 warnings emitted help: if this is intentional,warning: unused std::result::Result that must be used which should be handled which should be handled warning: 7 warnings emitted help: if this is intentional,warning: unused std::result::Result that must be used which should be handled which should be handled warning: 7 warnings emitted help: if this is intentional,warning: unused std::result::Result that must be used which should be handled which should be handled warning: 7 warnings emitted help: if this is intentional,warning: unused std::result::Result that must be used which should be handled which should be handled warning: 7 warnings emitted help: if this is intentional,warning: unused std::result::Result that must be used which should be handled which should be handled warning: 7 warnings emitted help: if this is intentional,warning: unused std::result::Result that must be used which should be handled which should be handled warning: 7 warnings emitted help: if this is intentional,warning: unused std::result::Result that must be used which should be handled which should be handled warning: 7 warnings emitted help: if this is intentional,warning: unused std::result::Result that must be used which should be handled which should be handled warning: 7 warnings emitted help: if this is intentional,warning: unused std::result::Result that must be used which should be handled which should be handled warning: 7 warnings emitted help: if this is intentional,warning: unused std::result::Result that must be used which should be handled which should be handled warning: 7 warnings emitted help: if this is intentional,warning: unused std::result::Result that must be used which should be handled which should be handled warning: 7 warnings emitted help: if this is intentional,warning: unused std::result::Result that must be used which should be handled which should be handled warning: 7 warnings emitted help: if this is intentional,warning: unused std::result::Result that must be used which should be handled which should be handled warning: 7 warnings emitted help: if this is intentional,warning: unused std::result::Result that must be used which should be handled which should be handled warning: 7 warnings emitted help: if this is intentional,warning: unused std::result::Result that must be used which should be handled which should be handled warning: 7 warnings emitted help: if this is intentional,warning: unused std::result::Result that must be used which should be handled which should be handled warning: 7 warnings emitted help: if this is intentional,warning: unused std::result::Result that must be used which should be handled which should be handled warning: 7 warnings emitted help: if this is intentional,warning: unused std::result::Result that must be used which should be handled which should be handled warning: 7 warnings emitted help: if this is intentional,warning: unused std::result::Result that must be used which should be handled which should be handled warning: 7 warnings emitted help: if this is intentional,ing: 7 warnings emitted help: if this is intentional,ing: 7 warnings emitted help: if this is intentional,ing: 7 warnings emitted help: if this is intentional,ing: 7 warnings emitted help: if this is intentional,7 warnings emitted help: if this is intentional, prefix it with an underscore:".to_string() + &i.to_string();
        let content_read = content.clone();
        let content_length = content.len();

        let read_handle = task::spawn(async move {
          let mut length = 0 as usize;
          let mut raw_content: Vec<u8> = Vec::new();
          let mut buf: Vec<u8> = Vec::new();
          buf.resize(10, 0);
          loop {
            match reader.read(&mut buf).await {
              Ok(size) => {
                if size > 0 {
                  raw_content.put(&buf.as_slice()[..size]);
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
  
          assert_eq!(length, content_length);
          assert_eq!(raw_content.as_slice(), content_read.as_bytes());
        });

        let write_handle = task::spawn(async move {
          let mut length = 0 as usize;
          let mut contents: Vec<u8> = Vec::new();
          contents.put(content.clone().as_bytes());
          match writer.write_all(&mut contents).await {
            Ok(()) => {
              length += contents.len();
            },
            Err(e) => {
              panic!("write err = {}", e);
            },
          }
  
          assert_eq!(length, content_length);
        });
  
        let zip_task = future::zip(read_handle, write_handle);
        task::block_on(zip_task);
      }
    });

    thread_handle.join().unwrap();
  }

  #[test]
  fn test_async_ring_buffer_write_read() {
    let thread_handle = thread::spawn(move ||{
      for i in 0..1024 {
        let ring_buffer = Arc::new(RingBuffer::new(64));
        let mut reader = RingBufferReader::new(ring_buffer.clone());
        let mut writer = RingBufferWriter::new(ring_buffer.clone());
        let content: String = "warning: unused std::result::Result unused std::resulunused std::result::Result that must be used which should be unused std::result::Result that must be used which should be unused std::result::Result that must be used which should be unused std::result::Result that must be used which should be unused std::result::Result that must be used which should be unused std::result::Result that must be used which should be unused std::result::Result that must be used which should be unused std::result::Result that must be used which should be unused std::result::Result that must be used which should be unused std::result::Result that must be used which should be unused std::result::Result that must be used which should be unused std::result::Result that must be used which should be unused std::result::Result that must be used which should be unused std::result::Result that must be used which should be unused std::result::Result that must be used which should be unused std::result::Result that must be used which should be unused std::result::Result that must be used which should be unused std::result::Result that must be used which should be unused std::result::Result that must be used which should be unused std::result::Result that must be used which should be unused std::result::Result that must be used which should be unused std::result::Result that must be used which should be unused std::result::Result that must be used which should be unused std::result::Result that must be used which should be unused std::result::Result that must be used which should be unused std::result::Result that must be used which should be unused std::result::Result that must be used which should be unused std::result::Result that must be used which should be unused std::result::Result that must be used which should be unused std::result::Result that must be used which should be unused std::result::Result that must be used which should be unused std::result::Result that must be used which should be unused std::result::Result that must be used which should be unused std::result::Result that must be used which should be unused std::result::Result that must be used which should be unused std::result::Result that must be used which should be unused std::result::Result that must be used which should be unused std::result::Result that must be used which should be unused std::result::Result that must be used which should be t::Result that must be used which should be that must be used which should be handled which should be handled warning: warning: unused std::result::Result that must be used which should be handled which should be handled warnwarning: unused std::result::Result that must be used which should be handled which should be handled warnwarning: unused std::result::Result that must be used which should be handled which should be handled warnwarning: unused std::result::Result that must be used which should be handled which should be handled warnwarning: unused std::result::Result that must be used which should be handled which should be handled warning: 7 warnings emitted help: if this is intentional,warning: unused std::result::Result that must be used which should be handled which should be handled warning: 7 warnings emitted help: if this is intentional,warning: unused std::result::Result that must be used which should be handled which should be handled warning: 7 warnings emitted help: if this is intentional,warning: unused std::result::Result that must be used which should be handled which should be handled warning: 7 warnings emitted help: if this is intentional,warning: unused std::result::Result that must be used which should be handled which should be handled warning: 7 warnings emitted help: if this is intentional,warning: unused std::result::Result that must be used which should be handled which should be handled warning: 7 warnings emitted help: if this is intentional,warning: unused std::result::Result that must be used which should be handled which should be handled warning: 7 warnings emitted help: if this is intentional,warning: unused std::result::Result that must be used which should be handled which should be handled warning: 7 warnings emitted help: if this is intentional,warning: unused std::result::Result that must be used which should be handled which should be handled warning: 7 warnings emitted help: if this is intentional,warning: unused std::result::Result that must be used which should be handled which should be handled warning: 7 warnings emitted help: if this is intentional,warning: unused std::result::Result that must be used which should be handled which should be handled warning: 7 warnings emitted help: if this is intentional,warning: unused std::result::Result that must be used which should be handled which should be handled warning: 7 warnings emitted help: if this is intentional,warning: unused std::result::Result that must be used which should be handled which should be handled warning: 7 warnings emitted help: if this is intentional,warning: unused std::result::Result that must be used which should be handled which should be handled warning: 7 warnings emitted help: if this is intentional,warning: unused std::result::Result that must be used which should be handled which should be handled warning: 7 warnings emitted help: if this is intentional,warning: unused std::result::Result that must be used which should be handled which should be handled warning: 7 warnings emitted help: if this is intentional,warning: unused std::result::Result that must be used which should be handled which should be handled warning: 7 warnings emitted help: if this is intentional,warning: unused std::result::Result that must be used which should be handled which should be handled warning: 7 warnings emitted help: if this is intentional,warning: unused std::result::Result that must be used which should be handled which should be handled warning: 7 warnings emitted help: if this is intentional,warning: unused std::result::Result that must be used which should be handled which should be handled warning: 7 warnings emitted help: if this is intentional,warning: unused std::result::Result that must be used which should be handled which should be handled warning: 7 warnings emitted help: if this is intentional,warning: unused std::result::Result that must be used which should be handled which should be handled warning: 7 warnings emitted help: if this is intentional,warning: unused std::result::Result that must be used which should be handled which should be handled warning: 7 warnings emitted help: if this is intentional,warning: unused std::result::Result that must be used which should be handled which should be handled warning: 7 warnings emitted help: if this is intentional,warning: unused std::result::Result that must be used which should be handled which should be handled warning: 7 warnings emitted help: if this is intentional,warning: unused std::result::Result that must be used which should be handled which should be handled warning: 7 warnings emitted help: if this is intentional,ing: 7 warnings emitted help: if this is intentional,ing: 7 warnings emitted help: if this is intentional,ing: 7 warnings emitted help: if this is intentional,ing: 7 warnings emitted help: if this is intentional,7 warnings emitted help: if this is intentional, prefix it with an underscore:".to_string() + &i.to_string();
        let content_read = content.clone();
        let content_length = content.len();

        let read_handle = task::spawn(async move {
          let mut length = 0 as usize;
          let mut raw_content: Vec<u8> = Vec::new();
          let mut buf: Vec<u8> = Vec::new();
          buf.resize(10, 0);
          loop {
            match reader.read(&mut buf).await {
              Ok(size) => {
                if size > 0 {
                  raw_content.put(&buf.as_slice()[..size]);
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
  
          assert_eq!(length, content_length);
          assert_eq!(raw_content.as_slice(), content_read.as_bytes());
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

  #[test]
  fn test_async_ring_buffer_write_read_buf() {
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
}