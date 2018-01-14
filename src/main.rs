#[macro_use]
extern crate bitmask;
extern crate libc;

use std::env;
use std::process::exit;

mod buffer;
use buffer::Buffer;

mod display;
use display::Display;

mod file;
use file::File;

fn main() {
    let argv: Vec<String> = env::args().collect();

    if argv.len() != 2 {
        eprintln!("Usage: {} <file>", argv[0]);
        exit(1);
    }

    let file = match File::new(argv[1].clone()) {
        Ok(f)   => f,
        Err(e)  => { eprintln!("Failed to open: {}", e); exit(1) }
    };

    let display = match Display::new() {
        Ok(d)   => d,
        Err(e)  => { eprintln!("Failed to open display: {}", e); exit(1) }
    };

    let mut buffer = match Buffer::new(file, display) {
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
