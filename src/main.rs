#[macro_use] extern crate bitmask;
             extern crate regex;
             extern crate serde;
#[macro_use] extern crate serde_derive;
             extern crate serde_json;

use std::env;
use std::process::exit;

mod buffer;
use buffer::Buffer;

mod config;
use config::ConfigFile;

mod display;
use display::Display;

mod file;
use file::File;

mod structs;

mod undo_file;
use undo_file::UndoFile;

fn main() {
    let argv: Vec<String> = env::args().collect();

    if argv.len() != 2 {
        eprintln!("Usage: {} <file>", argv[0]);
        exit(1);
    }

    let path = match std::fs::canonicalize(std::path::Path::new(&argv[1])) {
        Ok(p)   => p,
        Err(e)  => { eprintln!("Failed to find {}: {}", argv[1], e); exit(1) }
    };
    let path_str = match path.to_str() {
        Some(s) => String::from(s),
        None    => { eprintln!("Invalid path {}", argv[1]); exit(1) }
    };

    let mut config = match ConfigFile::new() {
        Ok(f)   => f,
        Err(e)  => { eprintln!("Failed to open config file: {}", e); exit(1) }
    };

    let file = match File::new(path_str.clone()) {
        Ok(f)   => f,
        Err(e)  => { eprintln!("Failed to open: {}", e); exit(1) }
    };

    let undo_file = match UndoFile::new(&mut config, path_str.clone()) {
        Ok(f)   => f,
        Err(e)  => { eprintln!("Failed to open undo file: {}", e); exit(1) }
    };

    let display = match Display::new() {
        Ok(d)   => d,
        Err(e)  => { eprintln!("Failed to open display: {}", e); exit(1) }
    };

    let mut buffer = match Buffer::new(display, file, undo_file, &mut config) {
        Ok(b)   => b,
        Err(e)  => { eprintln!("Failed to initialize buffer: {}", e); exit(1) }
    };

    while !buffer.should_quit() {
        if let Err(e) = buffer.handle_input() {
            buffer.restore_display();
            eprintln!("\n\n\nMain loop error: {}", e);
            exit(1);
        }
    }

    buffer.restore_display();
}
