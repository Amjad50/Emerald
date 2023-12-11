use crate::{fs, memory_management::virtual_memory};

pub mod elf;

#[allow(dead_code)]
pub fn load_elf_to_vm(
    elf: &elf::Elf,
    file: &mut fs::File,
    vm: &mut virtual_memory::VirtualMemoryManager,
) -> Result<(), fs::FileSystemError> {
    let old_vm = virtual_memory::get_current_vm();

    // switch temporaily so we can map the elf
    vm.switch_to_this();

    for segment in elf.program_headers() {
        if let elf::ElfProgramType::Load = segment.ty() {
            assert!(segment.virtual_address() == segment.physical_address());
            let mut flags = elf::to_virtual_memory_flags(segment.flags());
            flags |= virtual_memory::flags::PTE_USER;
            let entry = virtual_memory::VirtualMemoryMapEntry {
                virtual_address: segment.virtual_address(),
                physical_address: None,
                size: segment.mem_size(),
                flags,
            };
            eprintln!("Mapping segment: {:x?}", entry);
            vm.map(&entry);

            // read the file into the memory
            file.seek(segment.offset())?;

            let ptr = segment.virtual_address() as *mut u8;
            let slice =
                unsafe { core::slice::from_raw_parts_mut(ptr, segment.file_size() as usize) };

            // read the whole segment
            assert_eq!(file.read(slice)?, segment.file_size());
        }
    }

    // switch back to the old vm
    old_vm.switch_to_this();

    Ok(())
}
