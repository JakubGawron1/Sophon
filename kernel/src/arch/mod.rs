use core::ops::*;
use crate::memory::*;

#[repr(usize)]
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum InterruptId {
    Timer = 0,
    Soft = 1,
    PageFault = 2,
}

pub type InterruptHandler = fn(a: usize, b: usize, c: usize, d: usize, e: usize, f: usize) -> isize;

pub trait AbstractInterruptController: Sized {
    fn init();
    
    fn is_enabled() -> bool;
    fn enable();
    fn disable();

    fn set_handler(id: InterruptId, handler: Option<InterruptHandler>);

    fn uninterruptable<R, F: FnOnce() -> R>(f: F) -> R {
        let enabled = Self::is_enabled();
        if enabled {
            Self::disable();
        }
        let ret = f();
        if enabled {
            Self::enable();
        }
        ret
    }
}

pub struct TemporaryPage<S: PageSize>(Page<S>);

impl <S: PageSize> Deref for TemporaryPage<S> {
    type Target = Page<S>;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl <S: PageSize> Drop for TemporaryPage<S> {
    fn drop(&mut self) {
        Target::MemoryManager::unmap(self.0);
    }
}

pub trait AbstractMemoryManager: Sized {
    fn alloc_frame<S: PageSize>() -> Frame<S>;
    fn dealloc_frame<S: PageSize>(frame: Frame<S>);
    fn map<S: PageSize>(page: Page<S>, frame: Frame<S>, flags: PageFlags);
    fn translate(address: Address<V>) -> Option<(Address<P>, PageFlags)>;
    fn update_flags<S: PageSize>(page: Page<S>, flags: PageFlags);
    fn unmap<S: PageSize>(page: Page<S>);
    fn map_temporarily<S: PageSize>(page: Page<S>, frame: Frame<S>, flags: PageFlags) -> TemporaryPage<S> {
        Target::MemoryManager::map(page, frame, flags);
        TemporaryPage(page)
    }
}

pub trait AbstractTimer: Sized {
    fn init();
    fn wait(ms: usize);
}

pub trait AbstractContext: Sized {
    fn empty() -> Self;
    fn new(entry: *const extern fn() -> !) -> Self;
    fn fork(&self) -> Self;
    fn set_response_message(&mut self, m: crate::task::Message);
    fn set_response_status(&mut self, s: isize);
    unsafe extern fn return_to_user(&mut self) -> !;
}

pub trait AbstractLogger: Sized {
    fn put(c: char);
}

pub trait AbstractArch: Sized {
    type Interrupt: AbstractInterruptController;
    type Timer: AbstractTimer;
    type MemoryManager: AbstractMemoryManager;
    type Context: AbstractContext;
    type Logger: AbstractLogger;

    /// Platform initialization code
    /// Initialize: VirtualMemory/ExceptionVectorTable/...
    /// Should be tagged with `#[inline(always)]`
    #[naked]
    unsafe fn _start() -> !;
}

#[cfg(target_arch="aarch64")]
pub mod aarch64;
#[cfg(target_arch="aarch64")]
pub use aarch64::AArch64 as SelectedArch;

static mut BOOTED: bool = false;
fn set_booted() {
    unsafe { BOOTED = true }
}
pub fn booted() -> bool {
    unsafe { BOOTED }
}

#[allow(non_snake_case)]
pub mod Target {
    use super::*;
    pub type Arch = SelectedArch;
    pub type Interrupt = <SelectedArch as AbstractArch>::Interrupt;
    pub type Timer = <SelectedArch as AbstractArch>::Timer;
    pub type Context = <SelectedArch as AbstractArch>::Context;
    pub type MemoryManager = <SelectedArch as AbstractArch>::MemoryManager;
    pub type Logger = <SelectedArch as AbstractArch>::Logger;
}


/// Entry point for the low-level boot code
#[no_mangle]
#[naked]
pub unsafe extern fn _start() -> ! {
    Target::Arch::_start()
}
