use core::{fmt, mem::MaybeUninit, ptr::NonNull};

use alloc::collections::LinkedList;

use crate::{
    memory_management::memory_layout::{
        align_range, is_aligned, MemSize, KERNEL_EXTRA_MEMORY_BASE, KERNEL_EXTRA_MEMORY_SIZE,
        PAGE_4K,
    },
    sync::spin::mutex::Mutex,
};

use super::virtual_memory_mapper::{self, VirtualMemoryMapEntry};

static VIRTUAL_SPACE_ALLOCATOR: Mutex<VirtualSpaceAllocator> =
    Mutex::new(VirtualSpaceAllocator::empty());

pub enum VirtualSpaceError {
    OutOfSpace,
    AlreadyMapped,
    NotFullRange,
    EntryNotFound,
}

impl fmt::Debug for VirtualSpaceError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            VirtualSpaceError::OutOfSpace => write!(f, "Out of space"),
            VirtualSpaceError::AlreadyMapped => write!(f, "Already mapped"),
            VirtualSpaceError::NotFullRange => write!(f, "Not full range"),
            VirtualSpaceError::EntryNotFound => write!(f, "Entry not found"),
        }
    }
}

type Result<T> = core::result::Result<T, VirtualSpaceError>;

/// A wrapper over memory that is defined by its `physical address`.
/// We map this memory in `virtual space`, and return a pointer to it.
///
pub struct VirtualSpace<T: ?Sized> {
    size: usize,
    data: NonNull<T>,
}

impl<T> VirtualSpace<T> {
    /// Create a new virtual space for the given `physical_start` on the given type `T`.
    ///
    /// # Safety
    /// - Must be a valid physical address
    /// - The memory must be defined by default. if its not, use [`new_uninit`](Self::new_uninit) instead
    pub unsafe fn new(physical_start: u64) -> Result<Self> {
        let size = core::mem::size_of::<T>();
        let virtual_start = allocate_and_map_virtual_space(physical_start, size)?;
        let data = NonNull::new(virtual_start as *mut T).unwrap();
        Ok(Self { size, data })
    }

    /// Create a new virtual space for the given `physical_start` on the given type `T`.
    /// But will assume that the memory is not initialized, and will return a `MaybeUninit` pointer.
    ///
    /// # Safety
    /// - Must be a valid physical address
    #[allow(dead_code)]
    pub unsafe fn new_uninit(physical_start: u64) -> Result<VirtualSpace<MaybeUninit<T>>> {
        let size = core::mem::size_of::<T>();
        let virtual_start = allocate_and_map_virtual_space(physical_start, size)?;
        let data = NonNull::new(virtual_start as *mut T).unwrap();
        Ok(VirtualSpace {
            size,
            data: NonNull::new_unchecked(data.as_ptr() as *mut MaybeUninit<T>),
        })
    }

    /// Create a new virtual space for the given `physical_start` on the given slice type `[T]`.
    ///
    /// # Safety
    /// - Must be a valid physical address
    /// - The memory must be defined by default. currently, there is no way to create a slice of `MaybeUninit`
    pub unsafe fn new_slice(physical_start: u64, len: usize) -> Result<VirtualSpace<[T]>> {
        let size = core::mem::size_of::<T>() * len;
        let virtual_start = allocate_and_map_virtual_space(physical_start, size)?;
        let data = NonNull::new(virtual_start as *mut T).unwrap();
        let slice = core::slice::from_raw_parts_mut(data.as_ptr(), len);

        Ok(VirtualSpace {
            size,
            data: NonNull::new_unchecked(slice as *mut [T]),
        })
    }
}

impl<T: ?Sized> core::ops::Deref for VirtualSpace<T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        unsafe { self.data.as_ref() }
    }
}

impl<T: ?Sized> core::ops::DerefMut for VirtualSpace<T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        unsafe { self.data.as_mut() }
    }
}

unsafe impl<T: ?Sized + Send> Send for VirtualSpace<T> {}
unsafe impl<T: ?Sized + Sync> Sync for VirtualSpace<T> {}

impl<T: ?Sized> Drop for VirtualSpace<T> {
    fn drop(&mut self) {
        let size = self.size;
        deallocate_virtual_space(self.data.as_ptr() as *mut u8 as usize, size).unwrap();
    }
}

impl<T: ?Sized + fmt::Debug> fmt::Debug for VirtualSpace<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(&**self, f)
    }
}

impl<T: ?Sized + fmt::Display> fmt::Display for VirtualSpace<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(&**self, f)
    }
}

fn allocate_and_map_virtual_space(physical_start: u64, size: usize) -> Result<usize> {
    let (aligned_start, size, offset) = align_range(physical_start, size, PAGE_4K);

    let mut allocator = VIRTUAL_SPACE_ALLOCATOR.lock();
    let virtual_addr = allocator.allocate(aligned_start, size)?;

    virtual_memory_mapper::map_kernel(&VirtualMemoryMapEntry {
        virtual_address: virtual_addr,
        physical_address: Some(aligned_start),
        size,
        flags: virtual_memory_mapper::flags::PTE_WRITABLE,
    });
    // to make sure no one else play around with the space while we are mapping it
    drop(allocator);

    Ok(virtual_addr + offset)
}

fn deallocate_virtual_space(virtual_start: usize, size: usize) -> Result<()> {
    let (aligned_start, size, _) = align_range(virtual_start, size, PAGE_4K);

    let mut allocator = VIRTUAL_SPACE_ALLOCATOR.lock();
    allocator.deallocate(aligned_start, size)?;
    // unmap it after we deallocate (it will panic if its not valid deallocation)
    virtual_memory_mapper::unmap_kernel(
        &VirtualMemoryMapEntry {
            virtual_address: aligned_start,
            physical_address: None,
            size,
            flags: virtual_memory_mapper::flags::PTE_WRITABLE,
        },
        // we did specify our own physical address on allocation, so we must set this to false
        false,
    );

    Ok(())
}

pub fn debug_blocks() {
    let allocator = VIRTUAL_SPACE_ALLOCATOR.lock();
    allocator.debug_blocks();
}

struct VirtualSpaceEntry {
    physical_start: Option<u64>,
    virtual_start: usize,
    size: usize,
}

#[allow(dead_code)]
impl VirtualSpaceEntry {
    /// Return `None` if its not mapped, or if the `physical_start` is not inside this entry
    fn virtual_for_physical(&self, physical_start: u64) -> Option<usize> {
        if let Some(current_phy_start) = self.physical_start {
            // is inside?
            if current_phy_start <= physical_start
                && current_phy_start + self.size as u64 > physical_start
            {
                return Some(self.virtual_start + (physical_start - current_phy_start) as usize);
            }
        }
        None
    }
}

struct VirtualSpaceAllocator {
    entries: LinkedList<VirtualSpaceEntry>,
}

impl VirtualSpaceAllocator {
    const fn empty() -> Self {
        Self {
            entries: LinkedList::new(),
        }
    }

    /// Returns `(virtual_start, is_fully_inside)`
    fn get_entry_containing(
        &mut self,
        req_phy_start: u64,
        req_size: usize,
    ) -> Option<(&VirtualSpaceEntry, bool)> {
        assert!(req_size > 0);
        assert!(is_aligned(req_phy_start, PAGE_4K));
        assert!(is_aligned(req_size, PAGE_4K));

        let mut cursor = self.entries.cursor_front();
        while let Some(entry) = cursor.current() {
            if let Some(current_phy_start) = entry.physical_start {
                // is inside?
                if current_phy_start <= req_phy_start
                    && current_phy_start + entry.size as u64 > req_phy_start
                {
                    // this has parts of it inside
                    // is it fully inside?
                    if current_phy_start + entry.size as u64 >= req_phy_start + req_size as u64 {
                        // yes, it is fully inside
                        return Some((entry, true));
                    } else {
                        // no, it is not fully inside, but there is an overlap
                        // we can't allocate this and we can't relocate
                        return Some((entry, false));
                    }
                }
            }
            cursor.move_next();
        }
        None
    }

    fn allocate(&mut self, phy_start: u64, size: usize) -> Result<usize> {
        assert!(size > 0);
        assert!(is_aligned(phy_start, PAGE_4K));
        assert!(is_aligned(size, PAGE_4K));

        if self.get_entry_containing(phy_start, size).is_some() {
            return Err(VirtualSpaceError::AlreadyMapped);
        }

        let mut cursor = self.entries.cursor_front_mut();
        // find largest fitting entry and allocate from it
        while let Some(entry) = cursor.current() {
            if entry.physical_start.is_none() && entry.size >= size {
                // found it, split into two, and add to the list

                // the new entry (after this)
                let new_entry = VirtualSpaceEntry {
                    physical_start: None,
                    virtual_start: entry.virtual_start + size,
                    size: entry.size - size,
                };
                // shrink this entry
                entry.size = size;
                entry.physical_start = Some(phy_start);
                let virtual_address = entry.virtual_start;

                // add the new entry
                cursor.insert_after(new_entry);
                return Ok(virtual_address);
            }
            cursor.move_next();
        }
        // if this is the first time, add a new entry and try again
        if self.entries.is_empty() {
            assert!(is_aligned(KERNEL_EXTRA_MEMORY_SIZE, PAGE_4K));
            self.entries.push_back(VirtualSpaceEntry {
                physical_start: None,
                virtual_start: KERNEL_EXTRA_MEMORY_BASE,
                size: KERNEL_EXTRA_MEMORY_SIZE,
            });
            self.allocate(phy_start, size)
        } else {
            Err(VirtualSpaceError::OutOfSpace)
        }
    }

    fn deallocate(&mut self, req_virtual_start: usize, req_size: usize) -> Result<()> {
        assert!(req_size > 0);
        assert!(is_aligned(req_virtual_start, PAGE_4K));
        assert!(is_aligned(req_size, PAGE_4K));

        let mut cursor = self.entries.cursor_front_mut();
        while let Some(entry) = cursor.current() {
            // is inside?
            if entry.virtual_start <= req_virtual_start
                && entry.virtual_start + entry.size > req_virtual_start
            {
                // it must match the whole entry
                if req_virtual_start != entry.virtual_start || req_size != entry.size {
                    // panic!("Requested to deallocate {:016X}..{:016X}, but its partially inside {:016X}..{:016X}, must match exactly",
                    //     req_virtual_start, req_virtual_start + req_size, entry.virtual_start, entry.virtual_start + entry.size);
                    return Err(VirtualSpaceError::NotFullRange);
                }

                // found it, deallocate it
                assert!(entry.physical_start.is_some());
                entry.physical_start = None;

                // try to merge with after and before
                // extract the current so we can play around with values easily
                let mut current = cursor.remove_current().unwrap();

                // merge with next
                if let Some(next_entry) = cursor.current() {
                    if next_entry.physical_start.is_none() {
                        // merge with next
                        current.size += next_entry.size;
                        // here `cursor` is pointing to `next_entry`
                        cursor.remove_current();
                    }
                }
                // go the the previous entry (before current)
                cursor.move_prev();
                // merge with prev
                if let Some(prev_entry) = cursor.current() {
                    if prev_entry.physical_start.is_none() {
                        // merge with prev
                        prev_entry.size += current.size;
                        // no need to remove the `current` since its already removed
                    }
                }
                // add `current` back
                cursor.insert_after(current);
                return Ok(());
            }
            cursor.move_next();
        }
        Err(VirtualSpaceError::EntryNotFound)
    }

    fn debug_blocks(&self) {
        println!("Virtual space blocks:");
        for entry in self.entries.iter() {
            println!(
                "  range={:016x}..{:016x}, len={:4} => {:016X?}",
                entry.virtual_start,
                entry.virtual_start + entry.size,
                MemSize(entry.size),
                entry.physical_start
            );
        }
    }
}
