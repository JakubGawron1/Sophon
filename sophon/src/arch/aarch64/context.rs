use super::exception::ExceptionFrame;
use crate::memory::kernel::{KERNEL_HEAP, KERNEL_MEMORY_MAPPER};
use crate::memory::kernel::{KERNEL_STACK_PAGES, KERNEL_STACK_SIZE};
use crate::task::{Message, Proc};
use crate::utils::unint_lock::UnintMutex;
use crate::{arch::*, memory::physical::*};
use alloc::vec;
use alloc::vec::Vec;
use core::iter::Step;
use core::ops::Range;
use core::ptr;
use cortex_a::registers::*;
use memory::address::{Address, V};
use memory::page::*;
use memory::page_table::*;
use spin::Mutex;
use tock_registers::interfaces::Readable;

#[repr(C, align(4096))]
pub struct KernelStack {
    /// This page is protected to trap stack overflow
    guard: [u8; Size4K::BYTES],
    stack: [u8; KERNEL_STACK_SIZE],
}

impl KernelStack {
    pub fn new() -> &'static mut Self {
        let pages = KERNEL_STACK_PAGES + 1;
        let stack = KERNEL_HEAP.virtual_allocate::<Size4K>(pages);
        for i in 0..pages {
            let frame = PHYSICAL_MEMORY.acquire::<Size4K>().unwrap();
            KERNEL_MEMORY_MAPPER.map_fixed(
                Page::forward(stack.start, i),
                frame,
                PageFlags::kernel_data_flags_4k(),
            );
        }
        let kernel_stack = unsafe { stack.start.start().as_mut::<Self>() };
        kernel_stack.init();
        kernel_stack
    }
    pub fn init(&mut self) {
        // let guard_page = Page::<Size4K, V>::new(Address::from(&self.guard as *const [u8; Size4K::SIZE]));
        // PageTable::<L4>::get(true).update_flags(guard_page, PageFlags::_KERNEL_STACK_GUARD_FLAGS);
        // let stack_page_start = Page::<Size4K, V>::new(Address::from(&self.stack as *const [u8; KERNEL_STACK_SIZE]));
        // let stack_page_end = stack_page_start.add_usize(KERNEL_STACK_PAGES).unwrap();
        // for stack_page in stack_page_start..stack_page_end {
        //     PageTable::<L4>::get(true).update_flags(stack_page, PageFlags::_KERNEL_STACK_FLAGS);
        // }
        for i in 0..KERNEL_STACK_SIZE {
            unsafe {
                ::core::intrinsics::volatile_store(&mut self.stack[i], 0);
            }
        }
    }
    pub const fn range(&self) -> Range<Address> {
        let start = Address::from(&self.stack as *const [u8; KERNEL_STACK_SIZE]);
        let end = start + KERNEL_STACK_SIZE;
        start..end
    }
}

impl Drop for KernelStack {
    // Unprotect stack pages
    fn drop(&mut self) {
        unreachable!()
        // let guard_page = Page::<Size4K, V>::new(Address::from(&self.guard as *const [u8; Size4K::SIZE]));
        // PageTable::<L4>::get(true).update_flags(guard_page, PageFlags::_KERNEL_DATA_FLAGS_4K);
        // let stack_page_start = Page::<Size4K, V>::new(Address::from(&self.stack as *const [u8; KERNEL_STACK_SIZE]));
        // let stack_page_end = ::core::iter::Step::forward(stack_page_start, KERNEL_STACK_PAGES);
        // for stack_page in stack_page_start..stack_page_end {
        //     PageTable::<L4>::get(true).update_flags(stack_page, PageFlags::_KERNEL_DATA_FLAGS_4K);
        // }
    }
}

/// Represents the archtectural context (i.e. registers)
#[allow(improper_ctypes)]
#[repr(C)]
pub struct AArch64Context {
    exception_frames: Mutex<Vec<*mut ExceptionFrame>>,
    // sp: *mut u8,
    // x19_to_x29: [usize; 11],
    // x19: usize,
    // x20: usize,
    // x21: usize,
    // x22: usize,
    // x23: usize,
    // x24: usize,
    // x25: usize,
    // x26: usize,
    // x27: usize,
    // x28: usize,
    // x29: usize, // FP
    entry_pc: *mut u8, // x30

    // q: [u128; 32], // Neon registers
    kernel_stack: Option<*mut KernelStack>,
    kernel_stack_top: *mut u8,
    response_message: UnintMutex<Option<Message>>,
    response_status: UnintMutex<Option<isize>>,
}

impl AArch64Context {
    pub fn push_exception_frame(&self, exception_frame: *mut ExceptionFrame) {
        let _guard = interrupt::uninterruptable();
        self.exception_frames.lock().push(exception_frame)
    }
    fn pop_exception_frame(&self) -> Option<*mut ExceptionFrame> {
        let _guard = interrupt::uninterruptable();
        self.exception_frames.lock().pop()
    }
}

impl ArchContext for AArch64Context {
    fn empty() -> Self {
        Self {
            exception_frames: Mutex::new(vec![]),
            entry_pc: ptr::null_mut(),
            kernel_stack: None,
            kernel_stack_top: ptr::null_mut(),
            response_message: UnintMutex::new(None),
            response_status: UnintMutex::new(None),
        }
    }

    /// Create a new context with empty regs, given kernel stack,
    /// and current p4 table
    fn new(entry: *const extern "C" fn(a: *mut ()) -> !, ctx_ptr: *mut ()) -> Self {
        // Alloc kernel stack (SP_EL1)
        let kernel_stack = KernelStack::new();
        let sp: *mut u8 = kernel_stack.range().end.as_mut_ptr();
        let mut ctx = Self::empty();
        ctx.entry_pc = entry as _;
        ctx.kernel_stack_top = sp;
        ctx.kernel_stack = Some(kernel_stack);
        ctx.set_response_status(unsafe { ::core::mem::transmute(ctx_ptr) });
        ctx
    }

    fn set_response_message(&self, m: Message) {
        *self.response_message.lock() = Some(m);
    }

    fn set_response_status(&self, s: isize) {
        *self.response_status.lock() = Some(s);
    }

    unsafe extern "C" fn return_to_user(&self) -> ! {
        assert!(!interrupt::is_enabled());
        // Switch page table
        let p4 = Proc::current().get_page_table();
        if p4 as *mut _ as u64 != TTBR0_EL1.get() {
            // log!(
            //     "Switch page table {:?} -> {:?}",
            //     TTBR0_EL1.get() as *mut u8,
            //     p4 as *mut _
            // );
            TargetArch::set_current_page_table(Frame::new(p4.into()));
        }

        let exception_frame = self.pop_exception_frame().unwrap_or_else(|| {
            let mut frame: *mut ExceptionFrame =
                (self.kernel_stack_top as usize - ::core::mem::size_of::<ExceptionFrame>()) as _;
            (*frame).elr_el1 = self.entry_pc as _;
            (*frame).spsr_el1 = 0b0101;
            frame
        });
        if let Some(msg) = self.response_message.lock().take() {
            let slot = Address::<V>::from((*exception_frame).x2 as *mut Message);
            ::core::ptr::write(slot.as_mut_ptr(), msg);
        }
        if let Some(status) = self.response_status.lock().take() {
            let slot = Address::<V>::from(&(*exception_frame).x0 as *const usize);
            slot.store(status);
            (*exception_frame).x0 = ::core::mem::transmute(status);
        }
        // log!(
        //     "[return-to-user] SP={:?} IP={:?}",
        //     exception_frame,
        //     (*exception_frame).elr_el1
        // );
        asm!("mov sp, {}", in(reg) exception_frame);
        // log!(crate::Kernel: "exit_exception ");
        // Return from exception
        super::exception::exit_exception();
    }

    unsafe fn enter_usermode(
        entry: extern "C" fn(_argc: isize, _argv: *const *const u8),
        sp: Address,
        page_table: &mut PageTable,
    ) -> ! {
        log!(
            "TTBR0_EL1={:x} elr_el1={:?} sp_el0={:?}",
            TTBR0_EL1.get(),
            entry as *const extern "C" fn(_argc: isize, _argv: *const *const u8),
            sp
        );
        interrupt::disable();
        asm! {
            "
                msr spsr_el1, {0}
                msr elr_el1, {1}
                msr sp_el0, {2}
                msr	ttbr0_el1, {3}
                tlbi vmalle1is
                dsb sy
                isb sy
                eret
            ",
            in(reg) 0usize,
            in(reg) entry,
            in(reg) sp.as_usize(),
            in(reg) page_table as *const _
        }
        unreachable!()
    }
}

impl Drop for AArch64Context {
    fn drop(&mut self) {
        // println!("Context drop");
    }
}
