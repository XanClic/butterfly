extern crate termios;
extern crate termsize;

use std;
use std::collections::VecDeque;
use std::io::{Read,Write};
use std::os::unix::io::AsRawFd;


pub struct Display {
    istream: std::io::Stdin,
    ostream: std::io::Stdout,

    tios: termios::Termios,
    initial_tios: termios::Termios,

    old_height: u32,

    need_redraw: bool, // TODO: Should this be some callback?
    redraw_acknowledged: bool,

    mode: ColorMode,

    fifo: VecDeque<char>,
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
        StatusModeModify    = (1u64 <<  7),
        StructH0            = (1u64 <<  8),
        StructH1            = (1u64 <<  9),
        StructH2            = (1u64 << 10),
        StructH3P           = (1u64 << 11),
        Highlight           = (1u64 << 12),
    }
}

impl Display {
    pub fn new() -> Result<Self, String> {
        use self::termios::*;

        let istream = std::io::stdin();
        let mut ostream = std::io::stdout();

        let initial_tios = match Termios::from_fd(istream.as_raw_fd()) {
            Ok(t)   => t,
            Err(e)  => return Err(format!("Failed to read termios: {}", e))
        };
        let mut tios = initial_tios.clone();

        tios.c_lflag &= !(ECHO | ECHOE | ECHOK | ECHONL | ICANON);
        tios.c_cc[VTIME] = 0;
        tios.c_cc[VMIN] = 1;

        if let Err(e) = tcsetattr(istream.as_raw_fd(), TCSANOW, &mut tios) {
            return Err(format!("Failed to set terminal attributes: {}", e));
        }

        // Announce mouse support
        ostream.write(b"\x1b[?1002;1006;1015h").unwrap();
        ostream.flush().unwrap();

        let (_, height) = Self::dim();

        Ok(Display {
            istream: istream,
            ostream: ostream,

            tios: tios,
            initial_tios: initial_tios,

            old_height: height,

            need_redraw: false,
            redraw_acknowledged: false,

            mode: ColorMode::none(),

            fifo: VecDeque::<char>::new(),
        })
    }

    pub fn restore(&mut self) -> Result<(), String> {
        use self::termios::*;

        self.write_static("\x1b[?1002;1006;1015l");
        self.flush();

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

    pub fn clear_line(&mut self) {
        self.ostream.write(b"\x1b[K").unwrap();
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
        if let Some(x) = self.fifo.pop_front() {
            return Ok(Some(x));
        }

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
        use self::termios::*;

        if let Some(x) = self.fifo.pop_front() {
            return Ok(Some(x));
        }

        let mut tios = self.tios.clone();
        tios.c_cc[VMIN] = 0;

        if let Err(e) = tcsetattr(self.istream.as_raw_fd(), TCSANOW, &mut tios)
        {
            return Err(format!("Failed to switch terminal to non-blocking: {}",
                               e));
        }

        let ret = self.readchar();

        tios.c_cc[VMIN] = 1;
        if let Err(e) = tcsetattr(self.istream.as_raw_fd(), TCSANOW, &mut tios)
        {
            return Err(format!("Failed to switch terminal to blocking: {}", e));
        }

        ret
    }

    pub fn unreadchar(&mut self, chr: char) {
        self.fifo.push_back(chr);
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

        let mut bold = false;
        let mut underline = false;
        let mut swap = false;
        let mut fg_color = 9;
        let mut bg_color = 9;

        if self.mode.contains(Color::ActiveLine) {
            bg_color = 0; // usually black (default), but may differ
        }
        if self.mode.contains(Color::ActiveChar) {
            swap = true;
        }
        if self.mode.contains(Color::ErrorInfo) {
            bold = true;
            fg_color = 1; // red
        }
        if self.mode.contains(Color::AddressColumn) {
            fg_color = 6; // cyan
        }
        if self.mode.contains(Color::StatusModeRead) {
            bold = true;
            fg_color = 2; // green
        }
        if self.mode.contains(Color::StatusModeModify) {
            bold = true;
            fg_color = 1; // red
        }
        if self.mode.contains(Color::StatusModeReplace) {
            bold = true;
            underline = true;
            fg_color = 1; // red
        }
        if self.mode.contains(Color::StatusLoc) {
            fg_color = 6; // cyan
        }
        if self.mode.contains(Color::StructH0) {
            bold = true;
            fg_color = 4; // blue
        }
        if self.mode.contains(Color::StructH1) {
            fg_color = 4; // blue
        }
        if self.mode.contains(Color::Highlight) {
            bold = true;
            underline = true;
        }

        let mut sgr_string = String::from("\x1b[0");
        if bold && !swap {
            // swap can do funny things with bold, so let's leave that out
            sgr_string.push_str(";1");
        }
        if underline {
            sgr_string.push_str(";4");
        }
        if fg_color != 9 {
            sgr_string.push_str(&format!(";{}", fg_color + 30));
        }
        if bg_color != 9 {
            sgr_string.push_str(&format!(";{}", bg_color + 40));
        }
        if swap {
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
