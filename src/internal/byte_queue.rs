use std::ops;

/// A queue of bytes that can be dereferenced as a slice.
pub struct ByteQueue {
    buf: Vec<u8>,
    offset: usize,
    head: i64,
}

// Implementing the Deref trait allows ByteQueue to be used as a slice.
impl ops::Deref for ByteQueue {
    type Target = [u8];

    fn deref(&self) -> &[u8] {
        &self.buf[self.offset..]
    }
}

impl ByteQueue {
    pub fn new() -> ByteQueue {
        ByteQueue {
            buf: Vec::new(),
            offset: 0,
            head: 0,
        }
    }

    pub fn write(&mut self, data: &[u8]) {
        let required_cap = self.buf.len() + data.len();
        if required_cap < self.buf.capacity() {
            let available = self.buf.len() - self.offset;
            if available >= data.len() {
                // Compact the data-structure by moving the available data to
                // the beginning of the vector, and truncating.
                self.buf.copy_within(self.offset.., 0);
                self.buf.truncate(self.buf.len() - self.offset);
                self.offset = 0;
            }
        }
        self.buf.extend_from_slice(data);
    }

    pub fn pop(&mut self, n: usize) {
        assert!(n < self.buf.len() - self.offset);
        self.offset += n;
        self.head += n as i64;
    }

    pub fn head(self) -> i64 {
        self.head
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_buf(start: u8, count: usize) -> Vec<u8> {
        let mut v = Vec::with_capacity(count);
        for i in 0..count {
            v.push(((start as usize) + i) as u8);
        }
        v
    }

    #[test]
    fn write_and_pop() {
        let mut q = ByteQueue::new();
        q.write(make_buf(0, 256).as_slice());
        q.write(make_buf(0, 256).as_slice());
        q.pop(384);

        assert_eq!(q.len(), 128);
        assert_eq!(q[0], 128);
        assert_eq!(q[127], 255);
    }

    #[test]
    fn wrap() {
        let mut q = ByteQueue::new();
        q.write(make_buf(0, 256).as_slice());
        q.write(make_buf(0, 256).as_slice());
        q.pop(384);
        q.write(make_buf(0, 368).as_slice());

        {
            let mut expected = make_buf(128, 128);
            expected.extend(make_buf(0, 368));
            assert_eq!(&q[..], &expected[..]);
        }
        q.pop(128);

        assert_eq!(&q[..], make_buf(0, 368).as_slice());
    }
}
