//! Console output that cannot bring the process down.
//!
//! `println!` panics if the write fails, and for mcopy that is a realistic
//! failure rather than a theoretical one. The Windows binary is linked with
//! `windows_subsystem = "windows"`, so stdout is frequently an invalid or
//! already-closed handle; piping into a reader that exits early (`mcopy status
//! | head -1`, or a captured subshell) closes the pipe under us on every
//! platform. Either case turns a successful command into
//! `failed printing to stdout: os error 232` and a non-zero exit.
//!
//! Output from a GUI-subsystem binary is inherently best-effort, so these
//! helpers write what they can and discard write errors. Anything that must not
//! be lost goes to the log (see [`crate::util::log`]) or to a window.

use std::fmt::Arguments;
use std::io::Write;

/// Write a line to stdout, ignoring a closed or absent stream.
pub fn line(args: Arguments<'_>) {
    let stdout = std::io::stdout();
    let mut handle = stdout.lock();
    let _ = handle.write_fmt(args);
    let _ = handle.write_all(b"\n");
    let _ = handle.flush();
}

/// Write a line to stderr, ignoring a closed or absent stream.
pub fn error_line(args: Arguments<'_>) {
    let stderr = std::io::stderr();
    let mut handle = stderr.lock();
    let _ = handle.write_fmt(args);
    let _ = handle.write_all(b"\n");
    let _ = handle.flush();
}

/// `println!` that cannot panic on a broken pipe.
#[macro_export]
macro_rules! outln {
    () => { $crate::util::out::line(format_args!("")) };
    ($($arg:tt)*) => { $crate::util::out::line(format_args!($($arg)*)) };
}

/// `eprintln!` that cannot panic on a broken pipe.
#[macro_export]
macro_rules! errln {
    () => { $crate::util::out::error_line(format_args!("")) };
    ($($arg:tt)*) => { $crate::util::out::error_line(format_args!($($arg)*)) };
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn writing_never_panics() {
        // The point of the module: these must be infallible from the caller's
        // perspective, whatever the state of the stream.
        line(format_args!("mcopy test line {}", 1));
        error_line(format_args!("mcopy test error {}", 2));
    }

    #[test]
    fn macros_accept_plain_and_formatted_input() {
        outln!();
        outln!("plain");
        outln!("formatted {} {}", 1, "two");
        errln!();
        errln!("plain");
        errln!("formatted {}", 3);
    }
}
