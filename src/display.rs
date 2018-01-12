extern crate termsize;

use std;
use std::io::Write;


pub struct Display {
    stream: std::io::Stdout,

    /* width: u32, */
    height: u32,
}

impl Display {
    pub fn new() -> Result<Self, String> {
        let ts = match termsize::get() {
            Some(ts) => ts,
            None     =>
                return Err(String::from("Failed to inquire terminal size"))
        };

        Ok(Display {
            stream: std::io::stdout(),

            /* width: ts.cols as u32, */
            height: ts.rows as u32,
        })
    }

    pub fn clear(&mut self) {
        self.stream.write(b"\x1b[2J\x1b[;H").unwrap();
    }

    pub fn write(&mut self, text: String) {
        self.stream.write(text.as_bytes()).unwrap();
    }

    pub fn write_static(&mut self, text: &str) {
        self.stream.write(text.as_bytes()).unwrap();
    }

    pub fn flush(&mut self) {
        self.stream.flush().unwrap();
    }

    /*
    pub fn w(&self) -> u32 {
        self.width
    }
     */

    pub fn h(&self) -> u32 {
        self.height
    }
}
