#![feature(proc_macro_hygiene)]
#![feature(slice_patterns)]

#[macro_use]
extern crate dynasm;
extern crate dynasmrt;

mod codegen;
mod runtime;
mod protect;

use std::{
    io::{Read, Write},
    fs::File,
    sync::Arc,
    thread::{spawn, sleep},
    time::Duration,
};
use codegen::CGContext;
use runtime::Runtime;

fn main() {
    let mut f = File::open(::std::env::args().nth(1).unwrap()).unwrap();
    let mut code = String::new();
    f.read_to_string(&mut code).unwrap();
    let rt = Arc::new(Runtime::new(code));
    let rt2 = rt.clone();
    spawn(move || {
        sleep(Duration::from_millis(1000));
        let opt_level: u8 = 1;
        let delay = Some(Duration::from_millis(1));
        loop {
            osr_with_color::<Red>(&rt2, opt_level, delay);
            osr_with_color::<Green>(&rt2, opt_level, delay);
            osr_with_color::<Yellow>(&rt2, opt_level, delay);
            osr_with_color::<Blue>(&rt2, opt_level, delay);
            osr_with_color::<Magenta>(&rt2, opt_level, delay);
            osr_with_color::<Cyan>(&rt2, opt_level, delay);
        }
    });
    unsafe {
        rt.run();
    }
    print!("\x1b[0m");
}

trait Color {
    fn get_code() -> &'static [u8];
}

fn osr_with_color<C: Color>(rt: &Runtime, opt_level: u8, delay: Option<Duration>) {
    rt.do_osr(CGContext {
        putchar: putchar_for_color::<C>,
        getchar: runtime::getchar_default,
        opt_level: opt_level,
    });
    if let Some(delay) = delay {
        sleep(delay);
    }
}

macro_rules! define_color {
    ($name:ident, $val:expr) => {
        struct $name;
        impl Color for $name {
            fn get_code() -> &'static [u8] {
                $val
            }
        }
    }
}

define_color!(Red, b"\x1b[31m");
define_color!(Green, b"\x1b[32m");
define_color!(Yellow, b"\x1b[33m");
define_color!(Blue, b"\x1b[34m");
define_color!(Magenta, b"\x1b[35m");
define_color!(Cyan, b"\x1b[36m");

unsafe extern "C" fn putchar_for_color<C: Color>(x: u8) {
    let mut out = ::std::io::stdout();
    out.write_all(C::get_code()).unwrap();
    out.write_all(&[x]).unwrap();
    out.flush().unwrap();
}
