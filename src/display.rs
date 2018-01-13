extern crate termios;
extern crate termsize;

use std;
use std::error::Error;
use std::io::{Read,Write};
use std::os::unix::io::AsRawFd;


pub struct Display {
    istream: std::io::Stdin,
    ostream: std::io::Stdout,

    initial_tios: termios::Termios,

    /* width: u32, */
    height: u32,
}

impl Display {
    pub fn new() -> Result<Self, String> {
        use self::termios::*;

        let ts = match termsize::get() {
            Some(ts) => ts,
            None     =>
                return Err(String::from("Failed to inquire terminal size"))
        };

        let istream = std::io::stdin();
        let ostream = std::io::stdout();

        let initial_tios = match Termios::from_fd(istream.as_raw_fd()) {
            Ok(t)   => t,
            Err(e)  => return Err(String::from(e.description()))
        };
        let mut tios = initial_tios.clone();

        tios.c_lflag &= !(ECHO | ECHOE | ECHOK | ECHONL | ICANON);
        tios.c_cc[VTIME] = 1;
        tios.c_cc[VMIN] = 1;

        if let Err(e) = tcsetattr(istream.as_raw_fd(), TCSANOW, &mut tios) {
            return Err(String::from(e.description()));
        }

        Ok(Display {
            istream: istream,
            ostream: ostream,

            initial_tios: initial_tios,

            /* width: ts.cols as u32, */
            height: ts.rows as u32,
        })
    }

    pub fn restore(&mut self) -> Result<(), String> {
        use self::termios::*;

        if let Err(e) = tcsetattr(self.istream.as_raw_fd(), TCSANOW,
                                  &mut self.initial_tios)
        {
            return Err(String::from(e.description()));
        }

        Ok(())
    }

    pub fn clear(&mut self) {
        self.ostream.write(b"\x1b[2J\x1b[;H").unwrap();
    }

    pub fn set_cursor_pos(&mut self, x: usize, y: usize) {
        self.write(format!("\x1b[{};{}H", y + 1, x + 1));
    }

    pub fn write(&mut self, text: String) {
        self.ostream.write(text.as_bytes()).unwrap();
    }

    pub fn write_static(&mut self, text: &str) {
        self.ostream.write(text.as_bytes()).unwrap();
    }

    pub fn readchar(&mut self) -> Result<Option<char>, String> {
        let mut input: [u8; 1] = [0];
        let ret = match self.istream.read(&mut input) {
            Ok(r)   => r,
            Err(e)  => return Err(String::from(e.description()))
        };

        if ret < 1 {
            Ok(None)
        } else {
            Ok(Some(input[0] as char))
        }
    }

    pub fn flush(&mut self) {
        self.ostream.flush().unwrap();
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
