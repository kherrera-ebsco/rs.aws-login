//! Provides utilities that may be shared by multiple subcommands.

mod command;
mod config;
mod interface;
mod shell;

pub use command::*;
pub use config::*;
pub use interface::*;
pub use shell::*;

/// Prints a debugging message in debug builds.
#[cfg(debug_assertions)]
#[macro_export]
macro_rules! debug {
    ($message:tt) => {
        eprintln!($message);
    };
    ($message:tt, $($args:tt)*) => {
        eprintln!($message, $($args)*);
    };
}

/// Removes debug logs in release builds.
#[cfg(not(debug_assertions))]
#[macro_export]
macro_rules! debug {
    ($message:tt) => {};
    ($message:tt, $($args:tt)*) => {};
}
