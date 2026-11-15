#![cfg_attr(windows, windows_subsystem = "windows")]

mod cache;
mod release;

#[cfg(windows)]
mod process;
#[cfg(windows)]
mod ui;
#[cfg(windows)]
mod update;

#[cfg(windows)]
fn main() {
    ui::run();
}

#[cfg(not(windows))]
fn main() {
    eprintln!("This tool runs on Windows only.");
}
