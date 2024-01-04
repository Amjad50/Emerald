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

pub fn get_virtual_for_physical(physical_start: u64, size: u64) -> u64 {
    let (aligned_start, size, offset) = align_range(physical_start as _, size as _, PAGE_4K);

    let mut allocator = VIRTUAL_SPACE_ALLOCATOR.lock();
    let virtual_addr = allocator.get_virtual_for_physical(aligned_start as u64, size as u64);
    // ensure its mapped
    virtual_memory_mapper::map_kernel(&VirtualMemoryMapEntry {
        virtual_address: virtual_addr,
        physical_address: Some(aligned_start as u64),
        size: size as _,
        flags: virtual_memory_mapper::flags::PTE_WRITABLE,
    });
    // to make sure no one else play around with the space while we are mapping it
    drop(allocator);

    virtual_addr + offset as u64
}

pub fn ensure_at_least_size(virtual_start: u64, size: u64) {
    let (aligned_start, size, _) = align_range(virtual_start as _, size as _, PAGE_4K);

    let mut allocator = VIRTUAL_SPACE_ALLOCATOR.lock();
    if let Some((allocated, physical_addr)) =
        allocator.ensure_at_least_size(aligned_start as u64, size as u64)
    {
        if allocated {
            // ensure its mapped
            virtual_memory_mapper::map_kernel(&VirtualMemoryMapEntry {
                virtual_address: virtual_start,
                physical_address: Some(physical_addr),
                size: size as _,
                flags: virtual_memory_mapper::flags::PTE_WRITABLE,
            });
        }
    } else {
        panic!("Could not ensure at least size")
    }
    drop(allocator);
}

pub fn allocate_and_map_virtual_space(physical_start: u64, size: u64) -> u64 {
    let (aligned_start, size, offset) = align_range(physical_start as _, size as _, PAGE_4K);

    let mut allocator = VIRTUAL_SPACE_ALLOCATOR.lock();
    let virtual_addr = allocator.allocate(aligned_start as u64, size as u64);

    virtual_memory_mapper::map_kernel(&VirtualMemoryMapEntry {
        virtual_address: virtual_addr,
        physical_address: Some(aligned_start as u64),
        size: size as _,
        flags: virtual_memory_mapper::flags::PTE_WRITABLE,
    });
    // to make sure no one else play around with the space while we are mapping it
    drop(allocator);

    virtual_addr + offset as u64
}

#[allow(dead_code)]
pub fn deallocate_virtual_space(virtual_start: u64, size: u64) {
    let (aligned_start, size, _) = align_range(virtual_start as _, size as _, PAGE_4K);

    let mut allocator = VIRTUAL_SPACE_ALLOCATOR.lock();
    allocator.deallocate(aligned_start as u64, size as u64);
    // unmap it after we deallocate (it will panic if its not valid deallocation)
    virtual_memory_mapper::unmap_kernel(
        &VirtualMemoryMapEntry {
            virtual_address: aligned_start as u64,
            physical_address: None,
            size: size as _,
            flags: virtual_memory_mapper::flags::PTE_WRITABLE,
        },
        // we did specify our own physical address on allocation, so we must set this to false
        false,
    );
    drop(allocator);
}

pub fn debug_blocks() {
    let allocator = VIRTUAL_SPACE_ALLOCATOR.lock();
    allocator.debug_blocks();
}

struct VirtualSpaceEntry {
    physical_start: Option<u64>,
    virtual_start: u64,
    size: u64,
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

    /// Checks if we have this range allocated, returns it, otherwise perform an allocation, map it, and return
    /// the new address
    fn get_virtual_for_physical(&mut self, req_phy_start: u64, req_size: u64) -> u64 {
        assert!(req_size > 0);
        assert!(is_aligned(req_phy_start as _, PAGE_4K));
        assert!(is_aligned(req_size as _, PAGE_4K));

        let mut cursor = self.entries.cursor_front();
        while let Some(entry) = cursor.current() {
            if let Some(current_phy_start) = entry.physical_start {
                // is inside?
                if current_phy_start <= req_phy_start
                    && current_phy_start + entry.size > req_phy_start
                {
                    // this has parts of it inside
                    // is it fully inside?
                    if current_phy_start + entry.size >= req_phy_start + req_size {
                        // yes, it is fully inside
                        return entry.virtual_start + req_phy_start - current_phy_start;
                    } else {
                        // no, it is not fully inside, but there is an overlap
                        // we can't allocate this and we can't relocate
                        panic!("Requested {:016X}..{:016X} is inside {:016X}..{:016X} but it is not fully inside, and we can't relocate it", 
                            req_phy_start, req_phy_start + req_size, current_phy_start, current_phy_start + entry.size);
                    }
                }
            }
            cursor.move_next();
        }
        // we didn't find it, allocate it
        self.allocate(req_phy_start, req_size)
    }

    // Returns `Some` if it ensures the space
    // Returns `None` if it can't ensure the space
    //  Return true if it needs to be mapped, i.e. allocated
    //  Return false if it is already allocated
    fn ensure_at_least_size(
        &mut self,
        req_virtual_start: u64,
        req_size: u64,
    ) -> Option<(bool, u64)> {
        assert!(req_size > 0);
        assert!(is_aligned(req_virtual_start as _, PAGE_4K));
        assert!(is_aligned(req_size as _, PAGE_4K));

        let mut cursor = self.entries.cursor_front_mut();
        while let Some(entry) = cursor.current() {
            let entry_virtual_end = entry.virtual_start + entry.size;
            // is inside?
            if entry.virtual_start <= req_virtual_start && entry_virtual_end > req_virtual_start {
                let Some(current_phy_start) = entry.physical_start else {
                    // it is not allocated
                    panic!(
                        "Could not ensure size for {:016X}..{:016X}, it is not allocated",
                        req_virtual_start,
                        req_virtual_start + req_size
                    );
                };
                // get the offset from the start of this block
                let virt_offset = req_virtual_start - entry.virtual_start;
                // this has parts of it inside
                // is it fully inside?
                if entry_virtual_end >= req_virtual_start + req_size {
                    // yes, it is fully inside
                    return Some((false, current_phy_start + virt_offset));
                } else {
                    let addition_size = (req_virtual_start + req_size) - entry_virtual_end;
                    assert!(addition_size > 0);
                    let new_size = entry.size + addition_size;
                    // try to allocate from the next entry only if it is not allocated
                    let current = cursor.remove_current().unwrap();
                    if let Some(next_entry) = cursor.current() {
                        if next_entry.physical_start.is_none() {
                            // next is not taken, take part of it
                            let new_entry = VirtualSpaceEntry {
                                physical_start: Some(current_phy_start),
                                virtual_start: current.virtual_start,
                                size: new_size,
                            };
                            next_entry.size -= addition_size;
                            next_entry.virtual_start += addition_size;
                            cursor.insert_before(new_entry);
                            return Some((true, current_phy_start + virt_offset));
                        } else {
                            // add `current` back
                            cursor.insert_before(current);
                            // next is already taken, we can't allocate
                            // TODO: merge them together as one `allocated` chunk,
                            //       would need the deallocator to be able to deallocate part as well
                            return None;
                        }
                    }
                }
            }
            cursor.move_next();
        }
        panic!("Could not find virtual space to ensure at least size")
    }

    fn allocate(&mut self, phy_start: u64, size: u64) -> u64 {
        assert!(size > 0);
        assert!(is_aligned(phy_start as _, PAGE_4K));
        assert!(is_aligned(size as _, PAGE_4K));

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
                return virtual_address;
            }
            cursor.move_next();
        }
        // if this is the first time, add a new entry and try again
        if self.entries.is_empty() {
            assert!(is_aligned(KERNEL_EXTRA_MEMORY_SIZE, PAGE_4K));
            self.entries.push_back(VirtualSpaceEntry {
                physical_start: None,
                virtual_start: KERNEL_EXTRA_MEMORY_BASE as u64,
                size: KERNEL_EXTRA_MEMORY_SIZE as u64,
            });
            self.allocate(phy_start, size)
        } else {
            panic!("Out of virtual space");
        }
    }

    fn deallocate(&mut self, req_virtual_start: u64, req_size: u64) {
        assert!(req_size > 0);
        assert!(is_aligned(req_virtual_start as _, PAGE_4K));
        assert!(is_aligned(req_size as _, PAGE_4K));

        let mut cursor = self.entries.cursor_front_mut();
        while let Some(entry) = cursor.current() {
            // is inside?
            if entry.virtual_start <= req_virtual_start
                && entry.virtual_start + entry.size > req_virtual_start
            {
                // it must match the whole entry
                if req_virtual_start != entry.virtual_start || req_size != entry.size {
                    panic!("Requested to deallocate {:016X}..{:016X}, but its partially inside {:016X}..{:016X}, must match exactly", 
                        req_virtual_start, req_virtual_start + req_size, entry.virtual_start, entry.virtual_start + entry.size);
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
                return;
            }
            cursor.move_next();
        }
        panic!("Could not find virtual space to deallocate");
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
