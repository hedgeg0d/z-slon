use std::io::{self, Write};

pub fn safe_println(args: std::fmt::Arguments<'_>) {
    let mut out = io::stdout().lock();
    let _ = out.write_fmt(args);
    let _ = out.write_all(b"\n");
    let _ = out.flush();
}

pub fn safe_print(args: std::fmt::Arguments<'_>) {
    let mut out = io::stdout().lock();
    let _ = out.write_fmt(args);
    let _ = out.flush();
}

#[macro_export]
macro_rules! uci_println {
    () => { $crate::output::safe_println(format_args!("")) };
    ($($arg:tt)*) => { $crate::output::safe_println(format_args!($($arg)*)) };
}

#[macro_export]
macro_rules! uci_print {
    ($($arg:tt)*) => { $crate::output::safe_print(format_args!($($arg)*)) };
}
