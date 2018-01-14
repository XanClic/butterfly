use display::{Color,Display};
use file::File;
use std::error::Error;

enum Mode {
    Read,
    Replace,
}

pub struct Buffer {
    file: File,
    display: Display,

    base_offset: u64,
    buffer: Vec<u8>,

    mode: Mode,
    loc: u64,
    old_loc: u64, // LOC before last update_cursor() call
    replacing_nibble: u8, // TODO: Can this be done better?

    quit_request: bool,

    command_line: Option<String>,

    // When set, this will be showed by update_status() until the next input.
    status_info: Option<(String, Color)>,
}

impl Buffer {
    pub fn new(file: File, display: Display) -> Result<Self, String> {
        let mut buf = Buffer {
            file: file,
            display: display,

            base_offset: 0,
            buffer: Vec::<u8>::new(),

            mode: Mode::Read,
            loc: 0,
            old_loc: 0,
            replacing_nibble: 0,

            quit_request: false,

            command_line: None,
            status_info: None,
        };

        if let Err(e) = buf.term_update() {
            buf.restore_display();
            return Err(e);
        }

        Ok(buf)
    }

    pub fn restore_display(&mut self) {
        if let Err(e) = self.display.restore() {
            eprintln!("Failed to restore display: {}", e);
        }
    }

    fn end_offset(&mut self) -> Result<u64, String> {
        let height = self.display.h();
        if height <= 2 {
            return Ok(self.base_offset);
        }

        let disp_end = self.base_offset + (height as u64 - 2) * 16;
        let file_end = self.file.len()?;

        if file_end < disp_end {
            Ok(file_end)
        } else {
            Ok(disp_end)
        }
    }

    pub fn update(&mut self) -> Result<(), String> {
        let end_offset = self.end_offset()?;

        let mut current_offset = self.base_offset;

        self.buffer.resize((end_offset - self.base_offset) as usize, 0);
        self.file.read(self.base_offset, &mut self.buffer)?;

        self.display.clear();

        while current_offset < end_offset {
            self.redraw_line(current_offset)?;
            current_offset += 16;
        }

        self.update_status()?; // Flushes
        Ok(())
    }

    fn update_status(&mut self) -> Result<(), String> {
        let height = self.display.h();
        let y = if height >= 2 { height - 2 } else { 0 };

        self.display.set_cursor_pos(0, y as usize);
        self.display.write_static("────────────────────────────────────────────\
                                   ────────────────────────────────────────────\
                                   ─\n");

        if let Some((ref status_info, ref status_color)) = self.status_info {
            self.display.color_on_ref(status_color);
            self.display.write_static(status_info.as_str());
            self.display.color_off_ref(status_color);
        } else if let Some(ref cmd_line) = self.command_line {
            self.display.write(format!(":{:<88}", cmd_line));
        } else {
            let (mode_str, mode_col) = match self.mode {
                Mode::Read      => ("READ", Color::StatusModeRead),
                Mode::Replace   => ("REPLACE", Color::StatusModeReplace),
            };

            self.display.write(format!("{:width$}", "",
                                       width = 16 - mode_str.len()));
            self.display.color_on(mode_col);
            self.display.write_static(mode_str);
            self.display.color_off(mode_col);

            self.display.write(format!("{:55}", ""));

            let loc_str = format!("{:#x}", self.loc);
            self.display.write(format!("{:width$}", "",
                                       width = 18 - loc_str.len()));
            self.display.color_on(Color::StatusLoc);
            self.display.write(loc_str);
            self.display.color_off(Color::StatusLoc);
        }

        self.update_cursor()?;
        Ok(())
    }

    pub fn term_update(&mut self) -> Result<(), String> {
        self.cursor_to_bounds(false)?;
        self.update()?;

        Ok(())
    }

    /* NOTE: This method does not flush the output, and it assumes self.buffer
     *       to be up-to-date */
    fn redraw_line(&mut self, base: u64) -> Result<(), String> {
        let end_offset = self.end_offset()?;

        if base < self.base_offset {
            return Ok(());
        } else if base >= end_offset {
            return Ok(());
        } else if (base & 0xf) != 0 {
            panic!("Base address is not aligned");
        }

        let buffer_base = (base - self.base_offset) as usize;

        let y = (base - self.base_offset) / 16;
        self.display.set_cursor_pos(0 as usize, y as usize);

        let active_line = (self.loc & !0xf) == base;

        if active_line {
            self.display.color_on(Color::ActiveLine);
        }

        // Address
        self.display.color_on(Color::AddressColumn);
        self.display.write(format!("{:16x}", base));
        self.display.color_off(Color::AddressColumn);
        self.display.write_static(" │ ");

        // Hex data
        for i in 0..16 {
            if i == 4 || i == 12 {
                self.display.write_static(" ");
            } else if i == 8 {
                self.display.write_static("  ");
            }

            if base + (i as u64) < end_offset {
                let val = self.buffer[buffer_base + i];
                self.display.write(format!("{:02x} ", val));
            } else {
                self.display.write_static("   ");
            }
        }

        self.display.write_static("│ ");

        // Character data
        for i in 0..16 {
            let chr = if base + (i as u64) < end_offset {
                Some(self.buffer[buffer_base + i])
            } else {
                None
            };

            // Only draw cursor here if the real cursor is actually in the hex
            // column (which it is not when entering a command)
            let active_char = active_line && self.command_line.is_none() &&
                              base + (i as u64) == self.loc;
            if active_char {
                self.display.color_on(Color::ActiveChar);
            }

            if let Some(c) = chr {
                if c == 0x00 {
                    self.display.write_static(" ");
                } else if c == 0x0a {
                    self.display.write_static("¶");
                } else if c == 0x20 {
                    self.display.write_static("␣");
                } else if c < 0x20 || c > 0x7e {
                    self.display.write_static("·");
                } else {
                    self.display.write((c as char).to_string());
                }
            } else {
                self.display.write_static(" ");
            }

            if active_char {
                self.display.color_off(Color::ActiveChar);
            }
        }

        if active_line {
            self.display.color_off(Color::ActiveLine);
        }

        Ok(())
    }

    fn byte_to_x(byte: u8) -> u8 {
        if byte >= 12 {
            byte * 3 + 4
        } else if byte >= 8 {
            byte * 3 + 3
        } else if byte >= 4 {
            byte * 3 + 1
        } else {
            byte * 3
        }
    }

    pub fn update_cursor(&mut self) -> Result<(), String> {
        let loc = self.loc;
        let old_loc = self.old_loc;

        if loc < self.base_offset {
            return Ok(());
        }

        // Redraw (unhighlight) old line
        self.redraw_line(old_loc & !0xf)?;

        // Redraw (highlight) new line
        self.redraw_line(loc & !0xf)?;

        self.old_loc = loc;

        let x: usize;
        let y: usize;

        if let Some(ref cmd_line) = self.command_line {
            x = cmd_line.len() + 1;
            y = self.display.h() as usize - 1;
        } else {
            x = (Self::byte_to_x((loc % 16) as u8) + self.replacing_nibble)
                as usize + 19;
            y = ((loc - self.base_offset) / 16) as usize;
        }
        self.display.set_cursor_pos(x, y);
        self.display.flush();
        Ok(())
    }

    /*
     * Adjusts self.base_offset so the cursor is visible on screen.
     * Does nothing if the cursor is visible already.
     * Otherwise, if @recenter is true, self.base_offset is adjusted so the
     * cursor line is centered (unless LOC is head or tail).
     * If @recenter is false and the cursor is before self.base_offset, it will
     * be positioned on the first line.  If the cursor is beyond
     * self.end_offset(), it will be positioned on the last line.
     *
     * NOTE: This method does not update the screen
     */
    fn cursor_to_bounds(&mut self, recenter: bool) -> Result<(), String> {
        let disp_size = (self.display.h() as u64 - 2) * 16;
        let half_disp_size = (disp_size / 2) & !0xf;
        let loc_line = self.loc & !0xf;

        if recenter {
            if self.loc < self.base_offset || self.loc >= self.end_offset()? {
                if loc_line >= half_disp_size {
                    self.base_offset = loc_line - half_disp_size;
                } else {
                    self.base_offset = 0;
                }

                // Adjust to tail (ignore centering here)
                let lof = self.file.len()?;
                if self.base_offset + disp_size > lof {
                    let line_after_end = (lof + 0xf) & !0xf;
                    if line_after_end >= disp_size {
                        self.base_offset = line_after_end - disp_size;
                    } else {
                        self.base_offset = 0;
                    }
                }
            }
        } else {
            if self.loc < self.base_offset {
                self.base_offset = loc_line;
            } else if self.loc >= self.end_offset()? {
                let next_line = loc_line + 0x10;

                if next_line < disp_size {
                    self.base_offset = 0;
                } else {
                    self.base_offset = next_line - disp_size;
                }
            }
        }

        Ok(())
    }

    pub fn should_quit(&self) -> bool {
        self.quit_request
    }

    fn do_cursor_up(&mut self) -> Result<(), String> {
        self.replacing_nibble = 0;

        if self.loc >= 16 {
            if self.loc < self.base_offset + 16 {
                self.base_offset -= 16;
                self.update()?;
            }
            self.loc -= 16;
        }

        self.update_status()?;
        self.update_cursor()?;
        Ok(())
    }

    fn do_cursor_down(&mut self) -> Result<(), String> {
        self.replacing_nibble = 0;

        if self.loc + 16 < self.file.len()? {
            if self.loc + 16 >= self.end_offset()? {
                self.base_offset += 16;
                self.update()?;
            }
            self.loc += 16;
        }

        self.update_status()?;
        self.update_cursor()?;
        Ok(())
    }

    fn do_cursor_right(&mut self) -> Result<(), String> {
        self.replacing_nibble = 0;

        if self.loc + 1 < self.file.len()? {
            if self.loc % 16 == 15 {
                self.loc -= 15;
                self.do_cursor_down()?;
            } else {
                self.loc += 1;
            }
        }

        self.update_status()?;
        self.update_cursor()?;
        Ok(())
    }

    fn do_cursor_left(&mut self) -> Result<(), String> {
        self.replacing_nibble = 0;

        if self.loc > 0 {
            if self.loc % 16 == 0 {
                self.loc += 15;
                self.do_cursor_up()?;
            } else {
                self.loc -= 1;
            }
        }

        self.update_status()?;
        self.update_cursor()?;
        Ok(())
    }

    fn do_page_up(&mut self) -> Result<(), String> {
        self.replacing_nibble = 0;

        let offset = 16 * (self.display.h() as u64 - 2);

        if self.loc >= offset {
            self.loc -= offset;
        } else {
            self.loc = 0;
        }

        if self.base_offset >= offset {
            self.base_offset -= offset;
        } else {
            self.base_offset = 0;
        }

        self.update()?;
        Ok(())
    }

    fn do_page_down(&mut self) -> Result<(), String> {
        let lof = self.file.len()?;
        self.replacing_nibble = 0;

        let offset = 16 * (self.display.h() as u64 - 2);

        self.loc += offset;
        if self.loc >= lof {
            if lof > 0 {
                self.loc = lof - 1;
            } else {
                self.loc = 0;
            }
        }

        self.base_offset += offset;
        if self.base_offset + offset >= lof {
            let line_after_end = (lof + 0xf) & !0xf;
            if line_after_end >= offset {
                self.base_offset = line_after_end - offset;
            } else {
                self.base_offset = 0;
            }
        }

        self.update()?;
        Ok(())
    }

    fn do_key_end(&mut self) -> Result<(), String> {
        let lof = self.file.len()?;
        self.replacing_nibble = 0;

        self.loc = (self.loc & !0xf) + 0xf;
        if self.loc >= lof {
            if lof > 0 {
                self.loc = lof - 1;
            } else {
                self.loc = 0;
            }
        }

        self.update_status()?;
        self.update_cursor()?;
        Ok(())
    }

    fn do_key_home(&mut self) -> Result<(), String> {
        self.replacing_nibble = 0;
        self.loc &= !0xf;

        self.update_status()?;
        self.update_cursor()?;
        Ok(())
    }

    pub fn handle_input(&mut self) -> Result<(), String> {
        if self.display.need_redraw() {
            self.term_update()?;
        }

        let input = match self.display.readchar()? {
            Some(c) => c,
            None    => { self.quit_request = true; return Ok(()) }
        };

        self.status_info = None;

        if let Some(mut cmd_line) = self.command_line.take() {
            match input {
                '\n' => {
                    if let Err(e) = self.execute_cmdline(cmd_line) {
                        self.status_info = Some((format!("Error: {}", e),
                                                 Color::ErrorInfo));
                    }
                    self.update_status()?;
                    return Ok(());
                },

                // TODO (Whenever this manages to sufficiently annoy me)
                '\x1b' => {
                    cmd_line.push('^');
                    cmd_line.push('[');
                },

                // Backspace
                '\x7f' => {
                    if cmd_line.pop().is_none() {
                        self.command_line = None;
                        self.update_status()?;
                        return Ok(());
                    }
                },

                _ => {
                    cmd_line.push(input);
                }
            }

            self.command_line = Some(cmd_line);
            self.update_status()?;
            return Ok(());
        }

        match input {
            ':' => {
                self.command_line = Some(String::new());
                self.update_status()?;
            },

            'q' => {
                self.cmd_quit(vec![String::from("q")])?;
            },

            'R' => {
                self.cmd_replace_mode(vec![String::from("R")])?;
            },

            '\x1b' => {
                let mut escape_sequence = String::new();

                loop {
                    let input = match self.display.readchar_nonblock()? {
                        Some(c) => c,
                        None    => break
                    };

                    escape_sequence.push(input);
                    let seq = escape_sequence.as_bytes();

                    // TODO: Proper terminfo support
                    if seq.len() == 1 {
                        if seq[0] != ('[' as u8) {
                            break;
                        }
                    } else if seq.len() == 2 {
                        if seq[1] >= 'A' as u8 && seq[1] <= 'Z' as u8 {
                            break;
                        } else if seq[1] >= '0' as u8 && seq[1] <= '9' as u8 {
                        } else {
                            break;
                        }
                    } else {
                        break;
                    }
                }

                match escape_sequence.as_str() {
                    "[A" => self.do_cursor_up()?,
                    "[B" => self.do_cursor_down()?,
                    "[C" => self.do_cursor_right()?,
                    "[D" => self.do_cursor_left()?,
                    "[F" => self.do_key_end()?,
                    "[H" => self.do_key_home()?,

                    "[5~" => self.do_page_up()?,
                    "[6~" => self.do_page_down()?,

                    "" => {
                        self.cmd_read_mode(vec![String::from("")])?;
                    },

                    _ => (),
                }
            },

            _ => ()
        }

        Ok(())
    }

    fn execute_cmdline(&mut self, cmdline: String) -> Result<(), String> {
        let mut args = vec![];
        for arg in cmdline.split(' ') {
            if !arg.is_empty() {
                args.push(String::from(arg));
            }
        }

        if args.is_empty() {
            return Ok(());
        }

        // TODO: Needs something proper.
        match args[0].as_str() {
            "g" | "goto" => self.cmd_goto(args),
            "q" | "quit" => self.cmd_quit(args),

            _ => Err(format!("Unknown command “{}”", args[0]))
        }
    }

    fn cmd_goto(&mut self, args: Vec<String>) -> Result<(), String> {
        if args.len() != 2 {
            return Err(format!("Usage: {} <address|start|end>", args[0]));
        }

        // Rust is so nice to read
        self.loc =
            match if args[1] == "end" {
                    Ok(0xffffffffffffffffu64)
                } else if args[1] == "start" || args[1] == "begin" {
                    Ok(0u64)
                } else if args[1].starts_with("0x") {
                    u64::from_str_radix(&args[1][2..], 16)
                } else if args[1].starts_with("0b") {
                    // nice gimmmick
                    u64::from_str_radix(&args[1][2..], 2)
                } else if args[1].starts_with("0") {
                    u64::from_str_radix(args[1].as_str(), 8)
                } else {
                    args[1].parse::<u64>()
                }
        {
            Ok(v)   => v,
            Err(e)  => return Err(format!("{}: {}", args[1], e.description()))
        };

        let lof = self.file.len()?;
        if self.loc >= lof {
            if lof > 0 {
                self.loc = lof - 1;
            } else {
                self.loc = 0;
            }
        }

        self.cursor_to_bounds(true)?;
        self.update()?;

        Ok(())
    }

    fn cmd_quit(&mut self, _: Vec<String>) -> Result<(), String> {
        self.quit_request = true;
        Ok(())
    }

    fn cmd_replace_mode(&mut self, _: Vec<String>) -> Result<(), String> {
        self.mode = Mode::Replace;
        self.update_status()?;
        Ok(())
    }

    fn cmd_read_mode(&mut self, _: Vec<String>) -> Result<(), String> {
        self.mode = Mode::Read;
        self.update_status()?;

        Ok(())
    }
}
