use core::{marker::PhantomData, mem};

pub type InterruptHandler = extern "x86-interrupt" fn(frame: InterruptStackFrame64);
pub type InterruptHandlerWithError =
    extern "x86-interrupt" fn(frame: InterruptStackFrame64, error_code: u64);

#[repr(C, align(4))]
#[derive(Default, Clone, Copy, Debug)]
pub struct InterruptStackFrame64 {
    pub rip: u64,
    pub cs: u8,
    pub rflags: u64,
    pub rsp: u64,
    pub ss: u8,
}

mod flags {
    pub const GATE_TYPE: u8 = 0b1110;
    pub const KEEP_INTERRUPTS: u8 = 1 << 0;
    pub const PRESENT: u8 = 1 << 7;
    pub const fn dpl(ring: u8) -> u8 {
        ring << 5
    }
}

#[repr(C, align(16))]
#[derive(Default, Clone, Copy)]
pub(super) struct InterruptDescriptorTableEntry<T> {
    offset_low: u16,
    selector: u16,
    ist: u8,
    flags: u8,
    offset_middle: u16,
    offset_high: u32,
    zero: u32,
    phantom: PhantomData<T>,
}

impl<T> InterruptDescriptorTableEntry<T> {
    pub const fn empty() -> Self {
        Self {
            offset_low: 0,
            selector: 0,
            ist: 0,
            flags: 0,
            offset_middle: 0,
            offset_high: 0,
            zero: 0,
            phantom: PhantomData,
        }
    }

    fn set_handler_ptr(&mut self, handler_addr: u64) -> &mut Self {
        let cs = super::get_cs();
        self.offset_low = handler_addr as u16;
        self.offset_middle = (handler_addr >> 16) as u16;
        self.offset_high = (handler_addr >> 32) as u32;
        self.ist = 0;
        self.selector = cs;
        self.flags = flags::PRESENT | flags::GATE_TYPE;
        self
    }

    pub fn set_stack_index(&mut self, stack_index: Option<u8>) -> &mut Self {
        let stack_index = stack_index.map(|i| i + 1).unwrap_or(0);
        assert!(stack_index <= 7);
        self.ist = stack_index;
        self
    }

    #[allow(dead_code)]
    pub fn set_disable_interrupts(&mut self, disable_interrupts: bool) -> &mut Self {
        if disable_interrupts {
            self.flags &= !flags::KEEP_INTERRUPTS;
        } else {
            self.flags |= flags::KEEP_INTERRUPTS;
        }
        self
    }

    #[allow(dead_code)]
    pub fn override_code_segment(&mut self, cs: u16) -> &mut Self {
        self.selector = cs;
        self
    }

    #[allow(dead_code)]
    pub fn set_privilege_level(&mut self, ring: u8) -> &mut Self {
        self.flags = (self.flags & !flags::dpl(0b11)) | flags::dpl(ring);
        self
    }
}

#[allow(dead_code)]
impl InterruptDescriptorTableEntry<InterruptHandler> {
    pub fn set_handler(&mut self, handler: InterruptHandler) -> &mut Self {
        self.set_handler_ptr(handler as *const u8 as u64)
    }
}

#[allow(dead_code)]
impl InterruptDescriptorTableEntry<InterruptHandlerWithError> {
    pub fn set_handler(&mut self, handler: InterruptHandlerWithError) -> &mut Self {
        self.set_handler_ptr(handler as *const u8 as u64)
    }
}

#[repr(C)]
pub(super) struct InterruptDescriptorTable {
    pub divide_by_zero: InterruptDescriptorTableEntry<InterruptHandler>,
    pub debug: InterruptDescriptorTableEntry<InterruptHandler>,
    pub non_maskable_interrupt: InterruptDescriptorTableEntry<InterruptHandler>,
    pub breakpoint: InterruptDescriptorTableEntry<InterruptHandler>,
    pub overflow: InterruptDescriptorTableEntry<InterruptHandler>,
    pub bound_range_exceeded: InterruptDescriptorTableEntry<InterruptHandler>,
    pub invalid_opcode: InterruptDescriptorTableEntry<InterruptHandler>,
    pub device_not_available: InterruptDescriptorTableEntry<InterruptHandler>,
    pub double_fault: InterruptDescriptorTableEntry<InterruptHandlerWithError>,
    pub coprocessor_segment_overrun: InterruptDescriptorTableEntry<()>,
    pub invalid_tss: InterruptDescriptorTableEntry<InterruptHandlerWithError>,
    pub segment_not_present: InterruptDescriptorTableEntry<InterruptHandlerWithError>,
    pub stack_exception: InterruptDescriptorTableEntry<InterruptHandlerWithError>,
    pub general_protection_fault: InterruptDescriptorTableEntry<InterruptHandlerWithError>,
    pub page_fault: InterruptDescriptorTableEntry<InterruptHandlerWithError>,
    pub reserved_1: InterruptDescriptorTableEntry<()>,
    pub x87_floating_point: InterruptDescriptorTableEntry<InterruptHandler>,
    pub alignment_check: InterruptDescriptorTableEntry<InterruptHandlerWithError>,
    pub machine_check: InterruptDescriptorTableEntry<InterruptHandler>,
    pub simd_floating_point: InterruptDescriptorTableEntry<InterruptHandler>,
    pub reserved_2: InterruptDescriptorTableEntry<()>,
    pub control_protection: InterruptDescriptorTableEntry<InterruptHandler>,
    pub reserved_3: [InterruptDescriptorTableEntry<()>; 6],
    pub hypervisor_injection: InterruptDescriptorTableEntry<InterruptHandler>,
    pub vmm_communication: InterruptDescriptorTableEntry<InterruptHandler>,
    pub security_exception: InterruptDescriptorTableEntry<InterruptHandlerWithError>,
    pub reserved_4: InterruptDescriptorTableEntry<()>,
    pub user_defined: [InterruptDescriptorTableEntry<InterruptHandler>; 256 - 32],
}

impl InterruptDescriptorTable {
    pub(super) const fn empty() -> Self {
        Self {
            divide_by_zero: InterruptDescriptorTableEntry::empty(),
            debug: InterruptDescriptorTableEntry::empty(),
            non_maskable_interrupt: InterruptDescriptorTableEntry::empty(),
            breakpoint: InterruptDescriptorTableEntry::empty(),
            overflow: InterruptDescriptorTableEntry::empty(),
            bound_range_exceeded: InterruptDescriptorTableEntry::empty(),
            invalid_opcode: InterruptDescriptorTableEntry::empty(),
            device_not_available: InterruptDescriptorTableEntry::empty(),
            double_fault: InterruptDescriptorTableEntry::empty(),
            coprocessor_segment_overrun: InterruptDescriptorTableEntry::empty(),
            invalid_tss: InterruptDescriptorTableEntry::empty(),
            segment_not_present: InterruptDescriptorTableEntry::empty(),
            stack_exception: InterruptDescriptorTableEntry::empty(),
            general_protection_fault: InterruptDescriptorTableEntry::empty(),
            page_fault: InterruptDescriptorTableEntry::empty(),
            reserved_1: InterruptDescriptorTableEntry::empty(),
            x87_floating_point: InterruptDescriptorTableEntry::empty(),
            alignment_check: InterruptDescriptorTableEntry::empty(),
            machine_check: InterruptDescriptorTableEntry::empty(),
            simd_floating_point: InterruptDescriptorTableEntry::empty(),
            reserved_2: InterruptDescriptorTableEntry::empty(),
            control_protection: InterruptDescriptorTableEntry::empty(),
            reserved_3: [InterruptDescriptorTableEntry::empty(); 6],
            hypervisor_injection: InterruptDescriptorTableEntry::empty(),
            vmm_communication: InterruptDescriptorTableEntry::empty(),
            security_exception: InterruptDescriptorTableEntry::empty(),
            reserved_4: InterruptDescriptorTableEntry::empty(),
            user_defined: [InterruptDescriptorTableEntry::empty(); 256 - 32],
        }
    }

    pub fn init_default_handlers(&mut self) {
        self.divide_by_zero.set_handler(default_handler::<0>);
        self.debug.set_handler(default_handler::<1>);
        self.non_maskable_interrupt
            .set_handler(default_handler::<2>);
        self.breakpoint.set_handler(default_handler::<3>);
        self.overflow.set_handler(default_handler::<4>);
        self.bound_range_exceeded
            .set_handler(default_handler::<5>)
            .set_stack_index(Some(1));
        self.invalid_opcode
            .set_handler(default_handler::<6>)
            .set_stack_index(Some(1));
        self.device_not_available
            .set_handler(default_handler::<7>)
            .set_stack_index(Some(1));
        self.double_fault
            .set_handler(default_handler_with_error::<8>)
            .set_stack_index(Some(2));
        self.invalid_tss
            .set_handler(default_handler_with_error::<10>);
        self.segment_not_present
            .set_handler(default_handler_with_error::<11>);
        self.stack_exception
            .set_handler(default_handler_with_error::<12>);
        self.general_protection_fault
            .set_handler(default_handler_with_error::<13>);
        self.page_fault
            .set_handler(default_handler_with_error::<14>)
            .set_stack_index(Some(3));
        self.x87_floating_point.set_handler(default_handler::<16>);
        self.alignment_check
            .set_handler(default_handler_with_error::<17>)
            .set_stack_index(Some(2));
        self.machine_check.set_handler(default_handler::<18>);
        self.simd_floating_point.set_handler(default_handler::<19>);
        self.control_protection.set_handler(default_handler::<21>);
        self.hypervisor_injection.set_handler(default_handler::<28>);
        self.vmm_communication.set_handler(default_handler::<29>);
        self.security_exception
            .set_handler(default_handler_with_error::<30>);

        for entry in self.user_defined.iter_mut() {
            entry.set_handler(default_handler::<0xFF>);
        }
    }

    pub(super) fn apply_idt(&'static self) {
        let idt_ptr = InterruptDescriptorTablePointer {
            limit: mem::size_of::<InterruptDescriptorTable>() as u16 - 1,
            base: self,
        };

        unsafe {
            super::lidt(&idt_ptr);
        }
    }
}

#[repr(C, packed(2))]
pub(super) struct InterruptDescriptorTablePointer {
    limit: u16,
    base: *const InterruptDescriptorTable,
}

extern "x86-interrupt" fn default_handler<const N: u8>(frame: InterruptStackFrame64) {
    panic!("[{N}] Got exception: \n frame: {:x?}", frame);
}

extern "x86-interrupt" fn default_handler_with_error<const N: u8>(
    frame: InterruptStackFrame64,
    error_code: u64,
) {
    let cr2: u64;
    unsafe {
        core::arch:: asm!("mov {}, cr2", out(reg) cr2);
    }
    panic!(
        "[{N}] Got exception: \n frame: {:x?}\n error: {:016X}\n cr2: {:X}",
        frame, error_code, cr2
    );
}
