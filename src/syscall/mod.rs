#[macro_use]
pub mod utils;
pub mod fork;

use crate::exception::ExceptionFrame;

#[repr(usize)]
#[derive(Debug, Eq, PartialEq, Copy, Clone)]
pub enum SysCall {
    Fork = 0x0,
    // Exit,
    // MemoryMap,
    // MemoryUnmap,
    #[allow(non_camel_case_types)] __MAX_SYSCALLS,
}

type Handler = fn (exception_frame: &mut ExceptionFrame) -> isize;

macro_rules! handlers {
    ($($f: expr,)*) => { handlers![$($f),*] };
    ($($f: expr),*) => {[
        $(|ef: &mut ExceptionFrame| unsafe { ::core::mem::transmute($f(ef)) }),*
    ]};
}

static SYSCALL_HANDLERS: [Handler; SysCall::__MAX_SYSCALLS as usize] = handlers![
    fork::fork,
];


pub unsafe fn handle_syscall(exception_frame: &mut ExceptionFrame) {
    debug!("exception_frame@{:?}", exception_frame as *mut _);
    let syscall_id: SysCall = unsafe { ::core::mem::transmute((*exception_frame).x0) };
    debug!("Syscall: {:?}", syscall_id);
    let handler = SYSCALL_HANDLERS[syscall_id as usize];
    let result = handler(exception_frame);
    debug!("Syscall {:?} returned {:?}", syscall_id, result);
    exception_frame.x0 = ::core::mem::transmute(result);
}


