# Async lock-free Ringbuffer

FixedRingBuffer is an asynchronous SPSC fixed-capacity look-free ring buffer, which can be used to transfer data between two threads or between two asynchronous tasks. The overall feature is as follows:
* lock-free
* spsc
* Use a fixed-capacity buffer, and support buffer internal memory recycle
* Implement AsyncRead and AsyncBufRead trait for reader
* Implements AsyncWrite trait for writer

Quick Start
------------
```rust
    let ring_buffer = Arc::new(RingBuffer::new(1024));
    
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
        contents.put("example-data-data".as_bytes());
        for i in 0..102400 as usize {
          match writer.write_all(&mut contents).await {
            Ok(()) => {
              length += contents.len();
            },
            Err(e) => {
              panic!("write err = {}", e);
            },
          }
        }
        println!("length = {}", length);
      });

      task::block_on(handle);
    });

    t1.join();
    t2.join();
```