use crate::{fs, memory_management::virtual_memory_mapper};

pub mod elf;

#[allow(dead_code)]
pub fn load_elf_to_vm(
    elf: &elf::Elf,
    file: &mut fs::File,
    vm: &mut virtual_memory_mapper::VirtualMemoryMapper,
) -> Result<(usize, usize), fs::FileSystemError> {
    let old_vm = virtual_memory_mapper::get_current_vm();

    // switch temporaily so we can map the elf
    vm.switch_to_this();

    let mut min_address = u64::MAX;
    let mut max_address = 0;

    for segment in elf.program_headers() {
        if let elf::ElfProgramType::Load = segment.ty() {
            let segment_virtual = segment.virtual_address();
            assert!(segment_virtual == segment.physical_address());
            let mut flags = elf::to_virtual_memory_flags(segment.flags());
            flags |= virtual_memory_mapper::flags::PTE_USER;
            let entry = virtual_memory_mapper::VirtualMemoryMapEntry {
                virtual_address: segment_virtual,
                physical_address: None,
                size: segment.mem_size(),
                flags,
            };
            min_address = min_address.min(entry.virtual_address);
            max_address = max_address.max(entry.virtual_address + entry.size);
            eprintln!("Mapping segment: {:x?}", entry);
            vm.map(&entry);

            // read the file into the memory
            file.seek(segment.offset())?;

            let ptr = segment_virtual as *mut u8;
            let slice =
                unsafe { core::slice::from_raw_parts_mut(ptr, segment.file_size() as usize) };

            // read the whole segment
            assert_eq!(file.read(slice)?, segment.file_size());
        }
    }

    // switch back to the old vm
    old_vm.switch_to_this();

    Ok((min_address as usize, max_address as usize))
}
