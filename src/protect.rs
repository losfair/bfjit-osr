use libc::{c_int, c_void, siginfo_t};
use nix::sys::signal::{
    sigaction, SaFlags, SigAction, SigHandler, SigSet, Signal, SIGBUS, SIGFPE, SIGILL, SIGSEGV,
    SIGTRAP,
};
use std::any::Any;
use std::cell::{Cell, RefCell, UnsafeCell};
use std::collections::HashMap;
use std::ptr;
use std::sync::Arc;
use std::sync::Once;

extern "C" fn signal_trap_handler(
    signum: ::nix::libc::c_int,
    siginfo: *mut siginfo_t,
    ucontext: *mut c_void,
) {
    unsafe {
        match Signal::from_c_int(signum) {
            Ok(SIGTRAP) => {
                let info = get_fault_info(siginfo as _, ucontext);
                TRAP_INFO.with(|x| x.set(Some(info)));
                longjmp(SETJMP_BUFFER.with(|buf| buf.get()) as _, 42);
            }
            _ => {
                ::std::process::abort();
            }
        }
    }
}

extern "C" {
    pub fn setjmp(env: *mut c_void) -> c_int;
    fn longjmp(env: *mut c_void, val: c_int) -> !;
}

unsafe fn install_sighandler() {
    let sa = SigAction::new(
        SigHandler::SigAction(signal_trap_handler),
        SaFlags::SA_ONSTACK,
        SigSet::empty(),
    );
    sigaction(SIGTRAP, &sa).unwrap();
}

const SETJMP_BUFFER_LEN: usize = 27;
static SIGHANDLER_INIT: Once = Once::new();

thread_local! {
    static TRAP_INFO: Cell<Option<FaultInfo>> = Cell::new(None);
    static SETJMP_BUFFER: UnsafeCell<[c_int; SETJMP_BUFFER_LEN]> = UnsafeCell::new([0; SETJMP_BUFFER_LEN]);
}

pub unsafe fn call_protected(f: unsafe extern "C" fn(memory: *mut u8), mem: *mut u8) -> Result<(), FaultInfo> {
    unsafe {
        SIGHANDLER_INIT.call_once(|| {
            install_sighandler();
        });
        let jmp_buf = SETJMP_BUFFER.with(|buf| buf.get());
        let signum = setjmp(jmp_buf as *mut _);
        if signum != 0 {
            Err(TRAP_INFO.with(|x| x.get().unwrap()))
        } else {
            f(mem);
            Ok(())
        }
    }
}

#[derive(Copy, Clone, Debug)]
pub struct FaultInfo {
    pub addr: *const c_void,
    pub ip: *const c_void,
    pub rdi: usize,
}

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
unsafe fn get_fault_info(
    siginfo: *const c_void,
    ucontext: *const c_void,
) -> FaultInfo {
    use libc::{ucontext_t, RIP, RDI};

    #[allow(dead_code)]
    #[repr(C)]
    struct siginfo_t {
        si_signo: i32,
        si_errno: i32,
        si_code: i32,
        si_addr: u64,
        // ...
    }

    let siginfo = siginfo as *const siginfo_t;
    let si_addr = (*siginfo).si_addr;

    let ucontext = ucontext as *const ucontext_t;

    FaultInfo {
        addr: si_addr as _,
        ip: (*ucontext).uc_mcontext.gregs[RIP as usize],
        rdi: (*ucontext).uc_mcontext.gregs[RDI as usize],
    }
}

#[cfg(all(target_os = "macos", target_arch = "x86_64"))]
unsafe fn get_fault_info(
    siginfo: *const c_void,
    ucontext: *const c_void,
) -> FaultInfo {
    #[allow(dead_code)]
    #[repr(C)]
    struct ucontext_t {
        uc_onstack: u32,
        uc_sigmask: u32,
        uc_stack: libc::stack_t,
        uc_link: *const ucontext_t,
        uc_mcsize: u64,
        uc_mcontext: *const mcontext_t,
    }
    #[repr(C)]
    struct exception_state {
        trapno: u16,
        cpu: u16,
        err: u32,
        faultvaddr: u64,
    }
    #[repr(C)]
    struct regs {
        rax: u64,
        rbx: u64,
        rcx: u64,
        rdx: u64,
        rdi: u64,
        rsi: u64,
        rbp: u64,
        rsp: u64,
        r8: u64,
        r9: u64,
        r10: u64,
        r11: u64,
        r12: u64,
        r13: u64,
        r14: u64,
        r15: u64,
        rip: u64,
        rflags: u64,
        cs: u64,
        fs: u64,
        gs: u64,
    }
    #[allow(dead_code)]
    #[repr(C)]
    struct mcontext_t {
        es: exception_state,
        ss: regs,
        // ...
    }

    let siginfo = siginfo as *const siginfo_t;
    let si_addr = (*siginfo).si_addr;

    let ucontext = ucontext as *const ucontext_t;

    FaultInfo {
        addr: si_addr,
        ip: (*(*ucontext).uc_mcontext).ss.rip as _,
        rdi: (*(*ucontext).uc_mcontext).ss.rdi as _,
    }
}
