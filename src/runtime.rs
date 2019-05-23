use crate::codegen::{CGContext, Codegen, JitOutput};
use crate::protect::call_protected;
use libc::{mmap, mprotect, munmap, MAP_ANON, MAP_PRIVATE, PROT_EXEC, PROT_READ, PROT_WRITE};
use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::{Arc, Mutex, RwLock};
use std::io::{Read, Write};

pub struct CodeBuffer {
    ptr: *mut u8,
    size: usize,
}

unsafe impl Send for CodeBuffer {}
unsafe impl Sync for CodeBuffer {}

impl CodeBuffer {
    pub fn new(data: &[u8]) -> CodeBuffer {
        fn round_up_to_page_size(size: usize) -> usize {
            (size + (4096 - 1)) & !(4096 - 1)
        }
        let size = round_up_to_page_size(data.len());
        let ptr = unsafe {
            mmap(
                ::std::ptr::null_mut(),
                size,
                PROT_READ | PROT_WRITE | PROT_EXEC,
                MAP_PRIVATE | MAP_ANON,
                -1,
                0,
            )
        };
        if ptr as isize == -1 {
            panic!("cannot allocate code memory");
        }
        unsafe {
            ::std::slice::from_raw_parts_mut(ptr as *mut u8, size)[..data.len()].copy_from_slice(data);
        }
        CodeBuffer {
            ptr: ptr as _,
            size: size,
        }
    }

    pub fn offset(&self, x: usize) -> *const u8 {
        if x >= self.size {
            panic!("index out of bounds");
        }
        unsafe {
            self.ptr.offset(x as _)
        }
    }

    pub fn view(&self) -> &[AtomicU8] {
        unsafe {
            let slice: &[u8] = ::std::slice::from_raw_parts(self.ptr, self.size);
            ::std::mem::transmute(slice)
        }
    }
}

impl Drop for CodeBuffer {
    fn drop(&mut self) {
        unsafe {
            munmap(self.ptr as _, self.size);
        }
    }
}

pub struct Runtime {
    source: String,
    current_code: RwLock<(CodeBuffer, Vec<usize>)>,
    new_code: Mutex<Option<(CodeBuffer, Vec<usize>)>>,
}

impl Runtime {
    pub fn new(source: String) -> Runtime {
        let initial_code = Codegen::new(CGContext {
            putchar: putchar_default,
            getchar: getchar_default,
            opt_level: 0,
        }).translate(&source);
        Runtime {
            source: source,
            current_code: RwLock::new((
                CodeBuffer::new(&initial_code.buffer),
                initial_code.loop_end_patch_offsets,
            )),
            new_code: Mutex::new(None),
        }
    }

    pub fn do_osr(&self, ctx: CGContext) {
        let target_code = Codegen::new(ctx).translate(&self.source);
        *self.new_code.lock().unwrap() = Some((
            CodeBuffer::new(&target_code.buffer),
            target_code.loop_end_patch_offsets,
        ));

        let code = self.current_code.read().unwrap();
        let view = code.0.view();
        let offsets = &code.1;
        for x in offsets {
            view[*x].store(0xcc, Ordering::SeqCst);
        }
    }

    pub unsafe fn run(&self) {
        let mut offset: usize = 0;
        let mut mem: Vec<u8> = vec![0; 1048576];
        let mut mem_ptr = mem.as_mut_ptr();

        loop {
            let code = self.current_code.read().unwrap();
            match call_protected(::std::mem::transmute(code.0.offset(offset)), mem_ptr) {
                Ok(()) => break,
                Err(info) => {
                    let bp_offset = info.ip as usize - code.0.offset(0) as usize;
                    let idx = code.1.iter().enumerate().find(|&(i, &offset)| offset == bp_offset - 1).unwrap().0;
                    drop(code);
                    let mut code = self.current_code.write().unwrap();
                    ::std::mem::replace(&mut *code, self.new_code.lock().unwrap().take().unwrap());
                    offset = code.1[idx];
                    mem_ptr = info.rdi as _;
                }
            }
        }
    }
}

pub unsafe extern "C" fn putchar_default(x: u8) {
    let mut out = ::std::io::stdout();
    out.write_all(&[x]).unwrap();
    out.flush().unwrap();
}

pub unsafe extern "C" fn getchar_default() -> u8 {
    let mut buf: [u8; 1] = [0; 1];
    ::std::io::stdin().read_exact(&mut buf).unwrap();
    buf[0]
}
