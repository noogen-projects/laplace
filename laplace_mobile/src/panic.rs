//! Slightly changed copy of the https://github.com/sfackler/rust-log-panics/blob/master/src/lib.rs
use std::{
    panic::{self, PanicInfo},
    thread,
};

use log::error;

fn panic_handler(info: &PanicInfo<'_>) {
    let thread = thread::current();
    let thread = thread.name().unwrap_or("unnamed");

    let msg = match info.payload().downcast_ref::<&'static str>() {
        Some(s) => *s,
        None => match info.payload().downcast_ref::<String>() {
            Some(s) => &**s,
            None => "Box<Any>",
        },
    };

    match info.location() {
        Some(location) => {
            error!(
                target: "panic", "thread '{}' panicked at '{}': {}:{}",
                thread,
                msg,
                location.file(),
                location.line()
            );
        },
        None => error!(
            target: "panic",
            "thread '{}' panicked at '{}'",
            thread,
            msg
        ),
    }
}

/// Initializes the panic hook.
///
/// After this method is called, all panics will be logged rather than printed
/// to standard error.
pub fn set_logger_hook() {
    let next = panic::take_hook();
    panic::set_hook(Box::new(move |info| {
        panic_handler(info);
        next(info);
    }));
}
