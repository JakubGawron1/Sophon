use crate::exception;
use crate::mm::paging;
use cortex_a::{asm, regs::*};

#[no_mangle]
#[naked]
pub unsafe extern "C" fn _start() -> ! {
    // Halt non-promary processors
    asm! {"
            mrs     x0, mpidr_el1
            and     x0, x0, #3
            cbz     x0, 2f
        1:  wfe
            b       1b
        2:
    "};
    // Setup core 0 stack
    asm!("mov sp, $0"::"r"(0x80000));
    // Switch from EL2 -> EL1
    assert!(CurrentEL.get() == CurrentEL::EL::EL2.value);
    // Disable MMU
    SCTLR_EL1.set((3 << 28) | (3 << 22) | (1 << 20) | (1 << 11));
    // Enable time counter registers
    CNTHCTL_EL2.set(CNTHCTL_EL2.get() | 0b11);
    CNTVOFF_EL2.set(0);
    // Set execution mode = AArch64
    HCR_EL2.set(HCR_EL2.get() | (1 << 32));
    // Enable Debug+SError+IRQ+FIQ+EL1h
    SPSR_EL2.write(SPSR_EL2::D::Masked + SPSR_EL2::A::Masked + SPSR_EL2::I::Masked + SPSR_EL2::F::Masked + SPSR_EL2::M::EL1h);
    // Switch to EL1 -> kmain
    ELR_EL2.set(_start_el1 as *const () as u64); // EL1 PC after return from `eret`
    SP_EL1.set(0x80000); // EL1 stack
    asm::eret();
}

#[naked]
unsafe extern fn _start_el1() -> ! {
    // Enable all co-processors
    asm!("msr cpacr_el1, $0"::"r"(0xfffffff));
    // Setup paging
    crate::mm::paging::setup_kernel_pagetables();
    SP.set(SP.get() | 0xffff0000_00000000);
    // Call _start_el1_upper_address
    let fn_addr = _start_el1_upper_address as usize | 0xffff0000_00000000;
    let func: unsafe extern fn() -> ! = ::core::mem::transmute(fn_addr);
    func()
}

#[naked]
unsafe extern fn _start_el1_upper_address() -> ! {
    assert!(SP.get() & 0xffff0000_00000000 == 0xffff0000_00000000);
    // Set EL1 interrupt vector
    VBAR_EL1.set(&exception::exception_handlers as *const _ as _);
    // Call kmain
    crate::kmain()
}
