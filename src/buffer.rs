use config::ConfigFile;
use display::{Color,Display};
use file::File;
use regex::Regex;
use structs::Structs;
use undo_file::UndoFile;

enum Mode {
    Read,
    Modify,
    Replace,
}

pub struct Buffer {
    file: File,
    undo_file: UndoFile,
    display: Display,

    structs: Structs,
    active_struct: Option<usize>,

    base_offset: u64,
    buffer: Vec<u8>,

    mode: Mode,
    loc: u64,
    old_loc: u64, // LOC before last update_cursor() call

    // TODO: Can this be done better?
    replacing_nibble: u8,
    replacing_old: u8,
    replacing_loc: u64,

    quit_request: bool,

    command_line: Option<String>,

    // When set, this will be showed by update_status() until the next input.
    status_info: Option<(String, Color)>,

    // TODO: Proper commands with their own local data?
    jump_stack: Vec<u64>,

    mouse_input_regex_1006: Regex,
    mouse_input_regex_1015: Regex,
}

const SCROLL_OFFSET: u64 = 0x100;

impl Buffer {
    pub fn new(display: Display, file: File, undo_file: UndoFile,
               config: &mut ConfigFile)
        -> Result<Self, String>
    {
        let mut buf = Buffer {
            file: file,
            undo_file: undo_file,
            display: display,

            structs: Structs::load(config)?,
            active_struct: None,

            base_offset: 0,
            buffer: Vec::<u8>::new(),

            mode: Mode::Read,
            loc: 0,
            old_loc: 0,

            replacing_nibble: 0,
            replacing_old: 0,
            replacing_loc: 0,

            quit_request: false,

            command_line: None,
            status_info: None,

            jump_stack: vec![],

            mouse_input_regex_1006:
                Regex::new(r"^\[<([0-9]+);([0-9]+);([0-9]+)([mM])$").unwrap(),
            mouse_input_regex_1015:
                Regex::new(r"^\[([0-9]+);([0-9]+);([0-9]+)M$").unwrap(),
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

    fn update_struct(&mut self) -> Result<(), String> {
        // FIXME: Hard-coding is bad
        let start_x = 92;

        let a_s_i = match self.active_struct {
            Some(i) => i,
            None    => return Ok(())
        };
        let a_s = self.structs.get_mut(a_s_i);

        if let Err(e) = a_s.update(&mut self.file, self.loc,
                                   &mut self.display, start_x, 0)
        {
            // TODO: Don't just overwrite this
            self.status_info = Some((format!("struct: {}", e),
                                     Color::ErrorInfo));
        }

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
            self.display.write(format!("{:<89}", status_info));
            self.display.color_off_ref(status_color);
        } else if let Some(ref cmd_line) = self.command_line {
            self.display.write(format!(":{:<88}", cmd_line));
        } else {
            let (mode_str, mode_col) = match self.mode {
                Mode::Read      => ("READ-ONLY", Color::StatusModeRead),
                Mode::Modify    => ("MODIFY", Color::StatusModeModify),
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

    fn x_to_byte(x: u8) -> Option<u8> {
        let byte3 =
            if x >= 40 {
                x - 4
            } else if x >= 27 {
                x - 3
            } else if x >= 13 {
                x - 1
            } else {
                x
            };

        if byte3 >= 48 {
            None
        } else {
            Some(byte3 / 3)
        }
    }

    pub fn update_cursor(&mut self) -> Result<(), String> {
        let loc = self.loc;
        let old_loc = self.old_loc;

        self.update_struct()?;

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

    fn do_scroll_up(&mut self) -> Result<(), String> {
        if self.base_offset < SCROLL_OFFSET {
            self.base_offset = 0;
        } else {
            self.base_offset -= SCROLL_OFFSET;
        }

        // TODO (maybe in other places, too): self.loc is out of bounds on
        //      error.  Bad?
        let end_offset = self.end_offset()?;
        if self.loc >= end_offset {
            // We did not move self.loc, so end_offset must be the screen's end
            assert!(end_offset % 0x10 == 0);
            self.loc = end_offset - 0x10 + self.loc % 0x10;
        }

        self.update()?;
        Ok(())
    }

    fn do_scroll_down(&mut self) -> Result<(), String> {
        let height = self.display.h();
        if height <= 2 {
            return Ok(());
        }

        let disp_size = (height as u64 - 2) * 16;
        let disp_end = self.base_offset + disp_size;
        let file_end = self.file.len()?;

        if disp_end >= file_end {
            return Ok(());
        } else if disp_end + SCROLL_OFFSET >= file_end {
            self.base_offset = ((file_end + 0xf) & !0xf) - disp_size;
        } else {
            self.base_offset += SCROLL_OFFSET;
        }

        if self.loc < self.base_offset {
            self.loc = self.base_offset + self.loc % 0x10;
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

        let mut input = match self.display.readchar()? {
            Some(c) => c,
            None    => { self.quit_request = true; return Ok(()) }
        };

        self.status_info = None;

        if let Some(mut cmd_line) = self.command_line.take() {
            if (input as u8) < 0x20 && input != '\n' {
                // TODO (Whenever this manages to sufficiently annoy me)
                cmd_line.push('^');
                input = (input as u8 + '@' as u8) as char;
            }

            match input {
                '\n' => {
                    if let Err(e) = self.execute_cmdline(cmd_line) {
                        self.status_info = Some((format!("Error: {}", e),
                                                 Color::ErrorInfo));
                    }
                    self.update_status()?;
                    return Ok(());
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

        if let Mode::Replace = self.mode {
            let input_asc = input as u8;
            if (input_asc >= '0' as u8 && input_asc <= '9' as u8) ||
               (input_asc >= 'a' as u8 && input_asc <= 'f' as u8) ||
               (input_asc >= 'A' as u8 && input_asc <= 'F' as u8)
            {
                let val = if input_asc >= '0' as u8 && input_asc <= '9' as u8 {
                    input_asc - '0' as u8
                } else if input_asc >= 'a' as u8 && input_asc <= 'f' as u8 {
                    input_asc - 'a' as u8
                } else {
                    input_asc - 'A' as u8
                };

                let buf_offset = (self.loc - self.base_offset) as usize;
                let shift = 4 - self.replacing_nibble * 4;

                if self.replacing_nibble == 0 {
                    self.replacing_old = self.buffer[buf_offset];
                    self.replacing_loc = self.loc;
                } else {
                    assert!(self.replacing_loc == self.loc);
                }

                self.buffer[buf_offset] &= !(0xf << shift);
                self.buffer[buf_offset] |=   val << shift;

                let mut error = false;
                if self.replacing_nibble == 1 {
                    let old = self.replacing_old;
                    let new = self.buffer[buf_offset];
                    if let Err(e) = self.perform_replacement(old, new) {
                        self.status_info = Some((e, Color::ErrorInfo));
                        error = true;
                    }
                }

                if self.replacing_nibble == 0 {
                    self.replacing_nibble += 1;
                    self.update_cursor()?;
                } else {
                    self.replacing_nibble = 0;
                    self.do_cursor_right()?;
                }

                if error {
                    self.update()?;
                }

                return Ok(());
            }
        }

        if let Err(e) = match input {
            '\x12' => { // ^R
                self.cmd_redo(vec![String::from("^R")])
            },

            '\x14' => { // ^T
                self.cmd_jump_back(vec![String::from("^T")])
            },

            ':' => {
                self.command_line = Some(String::new());
                self.update_status()?;
                Ok(())
            },

            'M' => {
                self.cmd_modify_mode(vec![String::from("M")])
            },

            'q' => {
                self.cmd_quit(vec![String::from("q")])
            },

            'R' => {
                self.cmd_replace_mode(vec![String::from("R")])
            },

            't' => {
                self.cmd_jump_stack_push(vec![String::from("t")])
            },

            'u' => {
                self.cmd_undo(vec![String::from("u")])
            },

            '\x1b' => {
                let mut escape_sequence = String::with_capacity(256);

                // FIXME: This is a very arbitrary max length.
                //        Also, we need proper terminfo support.
                while escape_sequence.len() < 256 {
                    let input = match self.display.readchar_nonblock()? {
                        Some(c) => c,
                        None    => break
                    };

                    if input == '\x1b' {
                        self.display.unreadchar(input);
                        break;
                    }

                    escape_sequence.push(input);
                }

                self.handle_escape_sequence(escape_sequence)
            },

            _ => Ok(())
        } {
            self.status_info = Some((format!("Error: {}", e),
                                     Color::ErrorInfo));
            self.update_status()?;
        }

        Ok(())
    }

    fn perform_replacement(&mut self, old: u8, new: u8) -> Result<(), String> {
        let buf_offset = (self.loc - self.base_offset) as usize;

        if let Err(e) = self.undo_file.enter(self.loc, old, new) {
            self.buffer[buf_offset] = old;
            return Err(format!("Undo log error: {}", e));
        }

        if let Err(e) = self.file.write_u8(self.loc, new) {
            self.buffer[buf_offset] = old;
            return Err(format!("Write error: {}", e));
        }

        if let Err(e) = self.undo_file.settle() {
            return Err(format!("Undo log error: {}", e));
        }

        Ok(())
    }

    fn handle_mouse(&mut self, seq: &String) -> Result<bool, String> {
        let match_type;
        let mut button;
        let mut x;
        let mut y;

        {
            let regex;
            if self.mouse_input_regex_1006.is_match(seq.as_str()) {
                regex = &self.mouse_input_regex_1006;
                match_type = 1006;
            } else if self.mouse_input_regex_1015.is_match(seq.as_str()) {
                regex = &self.mouse_input_regex_1015;
                match_type = 1015;
            } else {
                return Ok(false);
            }

            let m = regex.captures(seq.as_str()).unwrap();

            button = match m[1].parse::<u32>() {
                Ok(v)   => v,
                Err(e)  => return Err(format!("Mouse input: {}: {}: {}",
                                              seq, &m[1], e))
            };

            x = match m[2].parse::<u32>() {
                Ok(v)   => v,
                Err(e)  => return Err(format!("Mouse input: {}: {}: {}",
                                              seq, &m[2], e))
            };

            y = match m[3].parse::<u32>() {
                Ok(v)   => v,
                Err(e)  => return Err(format!("Mouse input: {}: {}: {}",
                                              seq, &m[3], e))
            };

            if match_type == 1006 {
                if &m[4] == "m" {
                    // Button up; ignore anything non-button-down for now
                    return Ok(true);
                }
            } else {
                button -= 32;

                if button == 3 {
                    // Button up; ignore anything non-button-down for now
                    return Ok(true);
                }
            }
        }

        if button == 64 {
            self.do_scroll_up()?;
            return Ok(true);
        } else if button == 65 {
            self.do_scroll_down()?;
            return Ok(true);
        }

        if button >= 32 {
            // Movement; just interpret this as a new click
            button -= 32;
        }

        if button != 0 {
            // Ignore anything but the left button
            return Ok(true);
        }

        x -= 1;
        y -= 1;

        // FIXME: Hard-coding these things is not too nice
        let height = self.display.h();
        if height < 2 || y >= self.display.h() - 2 {
            return Ok(true);
        }

        if x < 19 || x > 89 {
            return Ok(true);
        }

        let byte =
            if let Some(hexbyte) = Self::x_to_byte((x - 19) as u8) {
                hexbyte
            } else if x >= 74 {
                (x - 74) as u8
            } else {
                return Ok(true);
            };

        self.loc = self.base_offset + y as u64 * 16 + byte as u64;
        self.update_cursor()?;
        self.update_status()?;

        Ok(true)
    }

    fn handle_escape_sequence(&mut self, mut seq: String) -> Result<(), String> {
        if self.handle_mouse(&seq)? {
            return Ok(());
        }

        let ret =
            if seq.starts_with("[A") {
                seq = seq.split_off(2); self.do_cursor_up()
            } else if seq.starts_with("[B") {
                seq = seq.split_off(2); self.do_cursor_down()
            } else if seq.starts_with("[C") {
                seq = seq.split_off(2); self.do_cursor_right()
            } else if seq.starts_with("[D") {
                seq = seq.split_off(2); self.do_cursor_left()
            } else if seq.starts_with("[F") {
                seq = seq.split_off(2); self.do_key_end()
            } else if seq.starts_with("[H") {
                seq = seq.split_off(2); self.do_key_home()
            } else if seq.starts_with("[5~") {
                seq = seq.split_off(3); self.do_page_up()
            } else if seq.starts_with("[6~") {
                seq = seq.split_off(3); self.do_page_down()
            } else if seq.starts_with("[1;5F") {
                seq = seq.split_off(5); self.do_goto(0xffffffffffffffffu64)
            } else if seq.starts_with("[1;5H") {
                seq = seq.split_off(5); self.do_goto(0)
            } else if seq.is_empty() {
                self.cmd_read_mode(vec![String::from("")])
            } else {
                // Return immediately, do not try to push some suffix back
                return Ok(());
            };

        if !seq.is_empty() {
            for c in seq.chars() {
                self.display.unreadchar(c);
            }
        }

        ret
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
            "struct" => self.cmd_struct(args),

            _ => Err(format!("Unknown command “{}”", args[0]))
        }
    }

    fn do_goto(&mut self, mut position: u64) -> Result<(), String> {
        self.jump_stack.push(self.loc);

        let lof = self.file.len()?;
        if position >= lof {
            if lof > 0 {
                position = lof - 1;
            } else {
                position = 0;
            }
        }
        self.loc = position;
        self.cursor_to_bounds(true)?;
        self.update()?;

        Ok(())
    }

    fn cmd_goto(&mut self, args: Vec<String>) -> Result<(), String> {
        if args.len() != 2 {
            return Err(format!("Usage: {} <address|start|end>", args[0]));
        }

        // Rust is so nice to read
        let position =
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
            Err(e)  => return Err(format!("{}: {}", args[1], e))
        };

        self.do_goto(position)
    }

    fn cmd_jump_back(&mut self, _: Vec<String>) -> Result<(), String> {
        self.loc = match self.jump_stack.pop() {
            Some(loc)   => loc,
            None        => return Err(String::from("Jump stack empty"))
        };

        if self.loc >= self.file.len()? {
            panic!("Invalid jump stack entry");
        }

        self.cursor_to_bounds(true)?;
        self.update()?;

        Ok(())
    }

    fn cmd_jump_stack_push(&mut self, _: Vec<String>) -> Result<(), String> {
        self.jump_stack.push(self.loc);
        Ok(())
    }

    fn cmd_modify_mode(&mut self, _: Vec<String>) -> Result<(), String> {
        self.mode = Mode::Modify;
        self.update_status()?;
        Ok(())
    }

    fn cmd_quit(&mut self, _: Vec<String>) -> Result<(), String> {
        self.quit_request = true;
        Ok(())
    }

    fn cmd_redo(&mut self, _: Vec<String>) -> Result<(), String> {
        if let Mode::Read = self.mode {
            return Err(String::from("Cannot redo in read-only mode"));
        }

        let (address, val) = match self.undo_file.redo()? {
            Some(x) => x,
            None    => return Err(String::from("Nothing to redo"))
        };

        if let Err(e) = self.file.write_u8(address, val) {
            return Err(format!("Write error: {}", e));
        }

        self.undo_file.settle()?;

        self.do_goto(address)?; // Performs a screen update

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

    fn cmd_struct(&mut self, args: Vec<String>) -> Result<(), String> {
        if args.len() != 2 {
            return Err(format!("Usage: {} <struct name>", args[0]));
        }

        let mut a_s = 0;
        while a_s < self.structs.len() {
            if self.structs.get(a_s).get_name() == args[1] {
                break;
            }

            a_s += 1;
        }
        if a_s == self.structs.len() {
            return Err(format!("Unknown struct “{}”", args[1]));
        }

        self.active_struct = Some(a_s);
        self.update()?;

        Ok(())
    }

    fn cmd_undo(&mut self, _: Vec<String>) -> Result<(), String> {
        if let Mode::Read = self.mode {
            return Err(String::from("Cannot undo in read-only mode"));
        }

        let (address, val) = match self.undo_file.undo()? {
            Some(x) => x,
            None    => return Err(String::from("Nothing to undo"))
        };

        if let Err(e) = self.file.write_u8(address, val) {
            return Err(format!("Write error: {}", e));
        }

        self.undo_file.settle()?;

        self.do_goto(address)?; // Performs a screen update

        Ok(())
    }
}
