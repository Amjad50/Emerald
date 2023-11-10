use core::mem::MaybeUninit;

pub struct RingBuffer<T> {
    buffer: [MaybeUninit<T>; 1024],
    read_index: usize,
    write_index: usize,
}

#[allow(dead_code)]
impl<T> RingBuffer<T> {
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

    pub fn pop(&mut self) -> Option<T> {
        if self.read_index == self.write_index {
            return None;
        }

        let value = unsafe { self.buffer[self.read_index].assume_init_read() };
        self.read_index = (self.read_index + 1) % self.buffer.len();

        Some(value)
    }
}

impl<T: Copy> RingBuffer<T> {
    pub const fn empty() -> Self {
        Self {
            buffer: [MaybeUninit::<T>::uninit(); 1024],
            read_index: 0,
            write_index: 0,
        }
    }
}
