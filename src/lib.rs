//! jaci encodes images into 24-bit colour ANSI art files and shows them as a
//! shell-startup banner beside a text panel.

mod ansi;
pub mod art_file;
mod atomic_file;
pub mod cache;
pub mod cli;
pub mod compose;
pub mod config;
pub mod image_input;
pub mod init;
pub mod named;
pub mod palette;
pub mod render;
pub mod size;
pub mod sysinfo;
pub mod template;
pub mod text;
