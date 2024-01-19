use core::mem::MaybeUninit;

/// A fixed size ring buffer
pub struct RingBuffer<T, const N: usize> {
    buffer: [MaybeUninit<T>; N],
    read_index: usize,
    write_index: usize,
}

#[allow(dead_code)]
impl<T, const N: usize> RingBuffer<T, N> {
    pub fn try_push(&mut self, value: T) -> bool {
        let next_index = (self.write_index + 1) % self.buffer.len();
        if next_index == self.read_index {
            return false;
        }

        self.buffer[self.write_index] = MaybeUninit::new(value);
        self.write_index = next_index;

        true
    }

    pub fn push(&mut self, value: T) {
        let next_index = (self.write_index + 1) % self.buffer.len();
        if next_index == self.read_index {
            panic!("Ring buffer overflow");
        }

        self.buffer[self.write_index] = MaybeUninit::new(value);
        self.write_index = next_index;
    }

    pub fn push_replace(&mut self, value: T) {
        let next_index = (self.write_index + 1) % self.buffer.len();
        // if the buffer is full, replace the oldest value
        if next_index == self.read_index {
            // drop the oldest value
            unsafe { self.buffer[self.read_index].assume_init_drop() };
            self.buffer[self.read_index] = MaybeUninit::new(value);
            self.write_index = next_index;
            // advance the read index
            self.read_index = (self.read_index + 1) % self.buffer.len();
        } else {
            self.buffer[self.write_index] = MaybeUninit::new(value);
            self.write_index = next_index;
        }
    }

    pub fn pop(&mut self) -> Option<T> {
        if self.read_index == self.write_index {
            return None;
        }

        let value = unsafe { self.buffer[self.read_index].assume_init_read() };
        self.read_index = (self.read_index + 1) % self.buffer.len();

        Some(value)
    }

    pub fn clear(&mut self) {
        self.read_index = 0;
        self.write_index = 0;
    }
}

impl<T: Copy, const N: usize> RingBuffer<T, N> {
    pub const fn empty() -> Self {
        Self {
            buffer: [MaybeUninit::<T>::uninit(); N],
            read_index: 0,
            write_index: 0,
        }
    }
}
