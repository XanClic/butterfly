use display::{Color,Display};
use file::File;

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
        };

        if let Err(e) = buf.term_update() {
            buf.restore_display();
            return Err(e);
        }

        Ok(buf)
    }

    pub fn restore_display(&mut self) {
        if let Err(e) = self.display.restore() {
            println!("Failed to restore display: {}", e);
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
        let y = self.display.h() - 2;
        self.display.set_cursor_pos(0, y as usize);
        self.display.write_static("────────────────────────────────────────────\
                                   ────────────────────────────────────────────\
                                   ─\n");

        let mode_str = match self.mode {
            Mode::Read      => "READ",
            Mode::Replace   => "REPLACE",
        };
        self.display.write(format!(" {:>15}{:55}{:>#18x}",
                                   mode_str, "", self.loc));

        self.update_cursor()?;
        Ok(())
    }

    pub fn term_update(&mut self) -> Result<(), String> {
        self.cursor_to_bounds()?;
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
            self.display.color(Color::ActiveLine);
        }

        // Address
        self.display.write(format!("{:16x} │ ", base));

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

            let active_char = active_line && base + (i as u64) == self.loc;
            if active_char {
                self.display.color(Color::ActiveChar);
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
                self.display.color(Color::ActiveLine);
            }
        }

        if active_line {
            self.display.color(Color::Normal);
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

        let x = Self::byte_to_x((loc % 16) as u8) + self.replacing_nibble;
        let y = (loc - self.base_offset) / 16;

        self.display.set_cursor_pos((x + 19) as usize, y as usize);
        self.display.flush();
        Ok(())
    }

    /* NOTE: This method does not update the screen */
    fn cursor_to_bounds(&mut self) -> Result<(), String> {
        if self.loc < self.base_offset {
            self.base_offset = self.loc & !0xf;
        } else if self.loc >= self.end_offset()? {
            let next_line = (self.loc & !0xf) + 0x10;
            let disp_size = (self.display.h() as u64 - 2) * 16;

            if next_line < disp_size {
                self.base_offset = 0;
            } else {
                self.base_offset = next_line - disp_size;
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
            self.loc = lof - 1;
        }

        self.base_offset += offset;
        if self.base_offset >= lof {
            self.base_offset = lof & !0xf;
        }

        self.update()?;
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

        match input {
            'q' => self.quit_request = true,

            '\x1b' => {
                let mut escape_sequence = String::new();

                loop {
                    let input = match self.display.readchar()? {
                        Some(c) => c,
                        None    => return Ok(())
                    };

                    escape_sequence.push(input);
                    let seq = escape_sequence.as_bytes();

                    // TODO: Proper terminfo support
                    if seq.len() == 1 {
                        if seq[0] != ('[' as u8) {
                            break;
                        }
                    } else if seq.len() == 2 {
                        match seq[1] as char {
                            'A' | 'B' | 'C' | 'D' => break,
                            '5' | '6' => (),

                            _ => break
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

                    "[5~" => self.do_page_up()?,
                    "[6~" => self.do_page_down()?,

                    _ => (),
                }
            },

            _ => ()
        }

        Ok(())
    }
}
