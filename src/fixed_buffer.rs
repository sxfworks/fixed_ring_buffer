use std::alloc::Layout;
use std::alloc::alloc;
use std::alloc::dealloc;
use std::sync::Arc;
use byteorder::{ByteOrder, BigEndian};
use core::{slice, usize};
use core::ops::{Deref, DerefMut};
use std::sync::atomic::{AtomicBool, AtomicPtr, AtomicU64, Ordering};

pub const PAGE_BITS: usize = 12;
pub const PAGE_SIZE: usize = 1 << PAGE_BITS;
pub const PAGE_MASK: usize = PAGE_SIZE - 1;

/// Align size according to 4096
pub fn align_size(size: usize) -> usize {
  (size + PAGE_MASK) & (!PAGE_MASK)
}

/// Allocate size of raw memory on the heap
pub fn alloc_bytes_ptr(size: usize) -> *mut u8 {
  unsafe {
    let layout = Layout::from_size_align(size, std::mem::size_of::<u8>()).unwrap();
    let ptr = alloc(layout);
    ptr
  }
}

/// Release size of heap memory
pub fn dealloc_bytes_ptr(ptr: *mut u8, size: usize) {
  unsafe {
    let layout = Layout::from_size_align(size, std::mem::size_of::<u8>()).unwrap();
    dealloc(ptr, layout);
  }
}

/// Fixed-capacity buffers, the underlying memory block of the buffer can be passed in from the outside. At this time, the recycling and release of the internal block is the responsibility of the allocator. If there is no external incoming memory block, the buffer will release the memory after all clone instances are destroyed.
pub struct FixedBuffer {
  must_be_call_dealloc: AtomicBool,
  capacity: usize,
  data_length: usize,
  ref_cnt: Arc<AtomicU64>,
  raw_data: AtomicPtr<u8>,
  recycle_fn_once: Option<Arc<dyn Fn(*mut u8, usize)  + Send + Sync>>,
}

impl FixedBuffer {
  const BUFFER_NULL: *mut u8 = 0 as *mut u8;

  /// Create a FixedBuffer through external memory. When the FixedBuffer is destroyed, recycle_fn_once will be called to recycle the memory
  #[inline]
  pub fn alloc_by_tag(tag: *mut u8, capacity: usize, recycle_fn_once: Option<Arc<dyn Fn(*mut u8, usize)  + Send + Sync>>) -> FixedBuffer {
    return FixedBuffer{
      must_be_call_dealloc: AtomicBool::new(false),
      capacity: capacity,
      data_length: 0,
      ref_cnt: Arc::new(AtomicU64::new(1)),
      raw_data: AtomicPtr::new(tag),
      recycle_fn_once: recycle_fn_once,
    };
  }

  /// Create a FixedBuffer by specifying the buffer size, the FixedBuffer internally will apply for memory on the heap, and the FixedBuffer will release its applied memory when it is destroyed
  pub fn alloc(capacity: usize)  -> FixedBuffer {
    let mut buffer = FixedBuffer{
      raw_data: AtomicPtr::new(Self::BUFFER_NULL),
      capacity: capacity,
      data_length: 0,
      ref_cnt: Arc::new(AtomicU64::new(1)),
      must_be_call_dealloc: AtomicBool::new(false),
      recycle_fn_once: None,
    };

    buffer.raw_data = AtomicPtr::new(alloc_bytes_ptr(capacity));
    buffer.must_be_call_dealloc = AtomicBool::new(true);
    return buffer;
  }

  /// Get the const pointer of the original memory block inside FixedBuffer
  #[inline]
  pub fn raw_data(&self) -> *const u8 {
    return self.raw_data.load(Ordering::Relaxed)
  }

  /// Get the mut pointer of the original memory block inside FixedBuffer
  #[inline]
  pub fn raw_data_mut(&self) -> *mut u8 {
    return self.raw_data.load(Ordering::Relaxed)
  }

  /// Return the internal data in the form of a slice, the length of the slice is equal to the length of the internal data of the buffer
  #[inline]
  pub fn as_slice(&self) -> &[u8] {
    unsafe { slice::from_raw_parts(self.raw_data(), self.len()) }
  }

  /// Return the internal data in the form of mut slice, the length of the slice is equal to the length of the internal data of the buffer
  #[inline]
  pub fn as_mut_slice(&mut self) -> &mut [u8] {
    unsafe { slice::from_raw_parts_mut(self.raw_data_mut(), self.len()) }
  }

  /// Add data to the buffer, if the internal space of the buffer is not enough, an error will be returned
  #[inline]
  pub fn append(&mut self, src: & [u8]) -> Result<usize, String> {
    if self.len() + src.len() > self.capacity() {
      return Err("buffer no more space to append".to_string());
    }

    match self.write_at(src, src.len(), self.len()) {
      Ok(size) => {
        self.data_length += size;
        return Ok(size);
      },
      Err(_e) => {
        return Err(_e);
      }
    }
  }

  /// Read data from the buffer, if there is not enough data in the buffer, an error will be returned
  pub fn read_at(&self, dst: &mut [u8], length: usize, offset: usize) -> Result<usize, String> {
    if offset + length > self.capacity() {
      return Err("dst buffer no more space to read".to_string());
    }

    unsafe {
      std::ptr::copy(self.raw_data().add(offset), dst.as_ptr() as *mut u8, length);
      return Ok(length)
    }
  }

  /// Write data to the buffer, if the space in the buffer does not meet the demand, an error will be returned
  #[inline]
  pub fn write_at(&mut self, src: &[u8], length:usize, offset: usize) -> Result<usize, String> {
    if self.read_only() {
      return Err("buffer occupied by multiple shares".to_string());
    }

    unsafe {
      std::ptr::copy(src.as_ptr() as *const u8, self.raw_data_mut().add(offset), length);
      return Ok(length)
    }
  }

  /// Write u8 to the buffer. If the space in the buffer does not meet the demand after the offset, an error will be returned
  #[inline]
  pub fn write_bigendian_u8(&mut self, val: u8, offset: usize) -> Result<usize, String> {
    let mut buf: [u8; 1] = [0; 1];
    buf[0] = val;
    return self.write_buf_at(&buf, offset);
  }

  /// Read u8 from the buffer, if there is not enough data after the offset in the buffer, an error will be returned
  #[inline]
  pub fn read_bigendian_u8(&self, offset: usize) -> Result<u8, String> {
    let mut buf: [u8; 1] = [0; 1];
    let result = self.read_buf_at(&mut buf, offset);
    match result {
      Ok(_) => {
        return Ok(buf[0]);
      },
      Err(e) => {
        return Err(e)
      },
    }
  }

  /// Write u16 to the buffer, if the space in the buffer does not meet the demand after the offset, an error will be returned
  #[inline]
  pub fn write_bigendian_u16(&mut self, val: u16, offset: usize) -> Result<usize, String> {
    let mut buf: [u8; 2] = [0; 2];
    BigEndian::write_u16(&mut buf, val);
    return self.write_buf_at(&buf, offset);
  }

  /// Read u16 from the buffer, if there is not enough data after the offset in the buffer, an error will be returned
  #[inline]
  pub fn read_bigendian_u16(&self, offset: usize) -> Result<u16, String> {
    let mut buf: [u8; 2] = [0; 2];
    let result = self.read_buf_at(&mut buf, offset);
    match result {
      Ok(_) => {
        return Ok(BigEndian::read_u16(&buf));
      },
      Err(e) => {
        return Err(e)
      },
    }
  }

  /// Write u32 to the buffer. If the space in the buffer does not meet the demand after the offset, an error will be returned
  #[inline]
  pub fn write_bigendian_u32(&mut self, val: u32, offset: usize) -> Result<usize, String> {
    let mut buf: [u8; 4] = [0; 4];
    BigEndian::write_u32(&mut buf, val);
    return self.write_buf_at(&buf[0..], offset);
  }

  /// Read u32 from the buffer, if there is not enough data after the offset in the buffer, an error will be returned
  #[inline]
  pub fn read_bigendian_u32(&self, offset: usize) -> Result<u32, String> {
    let mut buf: [u8; 4] = [0; 4];
    let result = self.read_buf_at(&mut buf[0..], offset);
    match result {
      Ok(_) => {
        return Ok(BigEndian::read_u32(&buf));
      },
      Err(e) => {
        return Err(e)
      },
    }
  }

  /// Write u64 to the buffer, if the space in the buffer does not meet the demand after the offset, an error will be returned
  #[inline]
  pub fn write_bigendian_u64(&mut self, val: u64, offset: usize) -> Result<usize, String> {
    let mut buf: [u8; 8] = [0; 8];
    BigEndian::write_u64(&mut buf, val);
    return self.write_buf_at(&buf[0..], offset);
  }

  /// Read u64 from the buffer, if there is not enough data after the offset in the buffer, an error will be returned
  #[inline]
  pub fn read_bigendian_u64(&self, offset: usize) -> Result<u64, String> {
    let mut buf: [u8; 8] = [0; 8];
    let result = self.read_buf_at(&mut buf[0..], offset);
    match result {
      Ok(_) => {
        return Ok(BigEndian::read_u64(&buf));
      },
      Err(e) => {
        return Err(e)
      },
    }
  }

  /// From the offset specified by the offset, read the data of the length of buf.len() into buf. If there is not enough data after the offset in the buffer, an error will be returned
  pub fn read_buf_at(&self, buf: &mut [u8], offset: usize) -> Result<usize, String> {
    if offset + buf.len() > self.len() {
      return Err( "buffer no more space to read".to_string());
    }
    return self.read_at(buf, buf.len(), offset);
  }

  /// From the offset specified by the offset, write the data of the length of buf.len() to buf. If there is not enough space after the offset in the buffer, an error will be returned
  pub fn write_buf_at(&mut self, buf: &[u8], offset: usize) -> Result<usize, String> {
    if offset + buf.len() > self.len() {
      return Err("buffer no more space to write".to_string());
    }
    return self.write_at(buf, buf.len(), offset);
  }

  /// Adjusting the size of the buffer will change the return value of len(), but it cannot exceed the capacity set when the buffer is created
  pub fn resize(&mut self, new: usize) {
    assert_eq!(new <= self.capacity, true);
    self.data_length = new;
  }

  /// Get the capacity value set when the buffer is created
  #[inline]
  pub fn capacity(&self) -> usize {
    return self.capacity;
  }

  /// Return the length of the internal data of the buffer
  #[inline]
  pub fn len(&self) -> usize {
    return self.data_length;
  }

  /// Determine whether the buffer is cloned, the buffer still shares the underlying memory during clone
  #[inline]
  pub fn read_only(&self) -> bool {
    return self.ref_cnt.load(Ordering::SeqCst) > 1;
  }
}

impl Clone for FixedBuffer {
  fn clone(&self) -> FixedBuffer {
    self.ref_cnt.fetch_add(1, Ordering::SeqCst);
    return FixedBuffer {
      must_be_call_dealloc: AtomicBool::new(self.must_be_call_dealloc.load(Ordering::Relaxed)),
      capacity: self.capacity,
      data_length: self.data_length,
      ref_cnt: self.ref_cnt.clone(),
      raw_data: AtomicPtr::new(self.raw_data.load(Ordering::Relaxed)),
      recycle_fn_once: self.recycle_fn_once.clone(),
    };
  }
}

impl Drop for FixedBuffer {
  fn drop(&mut self) {
    if self.ref_cnt.fetch_sub(1, Ordering::SeqCst) <= 1 {
      if self.must_be_call_dealloc.load(Ordering::Relaxed) {
        if self.raw_data() != FixedBuffer::BUFFER_NULL {
          dealloc_bytes_ptr(self.raw_data_mut(), self.capacity);
        }
      } else {
        match &self.recycle_fn_once {
          Some(recycle_fn_once) => {
            recycle_fn_once(self.raw_data_mut(), self.capacity);
          },
          None => {},
        }
      }
    }
  }
}

impl AsRef<[u8]> for FixedBuffer {
  #[inline]
  fn as_ref(&self) -> &[u8] {
      self.as_slice()
  }
}

impl Deref for FixedBuffer {
  type Target = [u8];

  #[inline]
  fn deref(&self) -> &[u8] {
      self.as_ref()
  }
}

impl AsMut<[u8]> for FixedBuffer {
  #[inline]
  fn as_mut(&mut self) -> &mut [u8] {
      self.as_mut_slice()
  }
}

impl DerefMut for FixedBuffer {
  #[inline]
  fn deref_mut(&mut self) -> &mut [u8] {
      self.as_mut()
  }
}

#[cfg(test)]
mod unit_tests {
  use crate::{fixed_buffer::{FixedBuffer}};

  #[test]
  fn test_fixed_buffer() {
    let mut fix_buf = FixedBuffer::alloc(1024);
    fix_buf.resize(1024);
    assert_eq!(fix_buf.len(), 1024);
    assert_eq!(1024, fix_buf.capacity());

    let mut fix_buf_read_only = fix_buf.clone();
    fix_buf_read_only.write_buf_at("test".as_bytes(), 0).unwrap();
  }
}
