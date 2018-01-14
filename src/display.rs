extern crate termios;
extern crate termsize;

use std;
use std::io::{Read,Write};
use std::os::unix::io::AsRawFd;


pub struct Display {
    istream: std::io::Stdin,
    ostream: std::io::Stdout,

    initial_tios: termios::Termios,

    old_height: u32,

    need_redraw: bool, // TODO: Should this be some callback?
    redraw_acknowledged: bool,

    mode: ColorMode,
}

bitmask! {
    pub mask ColorMode: u64 where flags Color {
        ActiveLine          = (1u64 <<  0),
        ActiveChar          = (1u64 <<  1),
        ErrorInfo           = (1u64 <<  2),
        AddressColumn       = (1u64 <<  3),
        StatusModeRead      = (1u64 <<  4),
        StatusModeReplace   = (1u64 <<  5),
        StatusLoc           = (1u64 <<  6),
    }
}

impl Display {
    pub fn new() -> Result<Self, String> {
        use self::termios::*;

        let istream = std::io::stdin();
        let ostream = std::io::stdout();

        let initial_tios = match Termios::from_fd(istream.as_raw_fd()) {
            Ok(t)   => t,
            Err(e)  => return Err(format!("Failed to read termios: {}", e))
        };
        let mut tios = initial_tios.clone();

        tios.c_lflag &= !(ECHO | ECHOE | ECHOK | ECHONL | ICANON);
        tios.c_cc[VTIME] = 1;
        tios.c_cc[VMIN] = 1;

        if let Err(e) = tcsetattr(istream.as_raw_fd(), TCSANOW, &mut tios) {
            return Err(format!("Failed to set terminal attributes: {}", e));
        }

        let (_, height) = Self::dim();

        Ok(Display {
            istream: istream,
            ostream: ostream,

            initial_tios: initial_tios,

            old_height: height,

            need_redraw: false,
            redraw_acknowledged: false,

            mode: ColorMode::none(),
        })
    }

    pub fn restore(&mut self) -> Result<(), String> {
        use self::termios::*;

        if let Err(e) = tcsetattr(self.istream.as_raw_fd(), TCSANOW,
                                  &mut self.initial_tios)
        {
            return Err(format!("Failed to restore terminal attributes: {}", e));
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
            Err(e)  => {
                if e.kind() == std::io::ErrorKind::WouldBlock {
                    0
                } else {
                    return Err(format!("Failed to read from stdin: {}", e))
                }
            }
        };

        if ret < 1 {
            Ok(None)
        } else {
            Ok(Some(input[0] as char))
        }
    }

    pub fn readchar_nonblock(&mut self) -> Result<Option<char>, String> {
        use libc;

        let fl: libc::c_int;

        unsafe {
            fl = libc::fcntl(libc::STDIN_FILENO, libc::F_GETFL);
            if fl < 0 {
                return Err(String::from("F_GETFL failed on stdin"));
            }
            if libc::fcntl(libc::STDIN_FILENO, libc::F_SETFL,
                           fl | libc::O_NONBLOCK) < 0
            {
                return Err(String::from("Failed to make stdin non-blocking"));
            }
        }

        let ret = self.readchar();

        unsafe {
            if libc::fcntl(libc::STDIN_FILENO, libc::F_SETFL, fl) < 0 &&
               ret.is_ok()
            {
                return Err(String::from("Failed to restore stdin's FD flags"));
            }
        }

        ret
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
                self.redraw_acknowledged = false;
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

    fn update_color(&mut self) {
        // TODO: Make these customizable
        let mut sgr_string = String::from("\x1b[0");

        if self.mode.contains(Color::ActiveLine) {
            // BG color 0
            sgr_string.push_str(";40");
        }
        if self.mode.contains(Color::ErrorInfo) {
            // Bold, red
            sgr_string.push_str(";1;31")
        }
        if self.mode.contains(Color::AddressColumn) {
            // Cyan
            sgr_string.push_str(";36")
        }
        if self.mode.contains(Color::StatusModeRead) {
            // Bold, green
            sgr_string.push_str(";1;32")
        }
        if self.mode.contains(Color::StatusModeReplace) {
            // Bold, underline, red
            sgr_string.push_str(";1;4;31")
        }
        if self.mode.contains(Color::StatusLoc) {
            // Cyan
            sgr_string.push_str(";36")
        }

        // Should be at the end
        if self.mode.contains(Color::ActiveChar) {
            // Swap FG and BG
            sgr_string.push_str(";7");
        }
        sgr_string.push('m');

        self.write(sgr_string)
    }

    pub fn color_on(&mut self, color: Color) {
        self.color_on_ref(&color);
    }

    pub fn color_off(&mut self, color: Color) {
        self.color_off_ref(&color);
    }

    pub fn color_on_ref(&mut self, color: &Color) {
        self.mode.set(*color);
        self.update_color();
    }

    pub fn color_off_ref(&mut self, color: &Color) {
        self.mode.unset(*color);
        self.update_color();
    }
}
