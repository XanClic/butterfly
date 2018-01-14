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

    old_height: u32,

    need_redraw: bool, // TODO: Should this be some callback?
    redraw_acknowledged: bool,
}

impl Display {
    pub fn new() -> Result<Self, String> {
        use self::termios::*;

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

        let (_, height) = Self::dim();

        Ok(Display {
            istream: istream,
            ostream: ostream,

            initial_tios: initial_tios,

            old_height: height,

            need_redraw: false,
            redraw_acknowledged: true,
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

    fn dim() -> (u32, u32) {
        match termsize::get() {
            Some(ts) => (ts.cols as u32, ts.rows as u32),
            None     => (80, 25)
        }
    }

    pub fn h(&mut self) -> u32 {
        let (_, height) = Self::dim();

        if height != self.old_height {
            if self.redraw_acknowledged {
                self.need_redraw = false;
                self.old_height = height;
            } else {
                self.need_redraw = true;
            }
        }

        // Better report the old height until the state has been properly
        // adapted to acknowledge the change
        self.old_height
    }

    pub fn need_redraw(&mut self) -> bool {
        if self.need_redraw {
            self.redraw_acknowledged = true;
        }

        self.need_redraw
    }
}
