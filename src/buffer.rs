use display::Display;
use file::File;

pub struct Buffer {
    file: File,
    display: Display,

    line: u64,
    buffer: Vec<u8>,
}

impl Buffer {
    pub fn new(file: File, display: Display) -> Result<Self, String> {
        let mut buf = Buffer {
            file: file,
            display: display,

            line: 0,
            buffer: Vec::<u8>::new(),
        };

        buf.update()?;
        Ok(buf)
    }

    pub fn update(&mut self) -> Result<(), String> {
        let display_lines = self.display.h();
        let file_rem = self.file.len()? - self.line * 16;
        let display_len = ((display_lines - 2) * 16) as u64;
        let mut remlen = if file_rem > display_len {
            display_len
        } else {
            file_rem
        };

        let base_offset = self.line * 16;
        let mut current_offset = 0;
        let mut current_output_y = 0;

        self.buffer.resize(remlen as usize, 0);
        self.file.read(base_offset, &mut self.buffer)?;

        self.display.clear();

        while remlen > 0 {
            self.display.write(format!("{:16x} | ",
                                       base_offset + current_offset));

            for i in 0..16 {
                if i == 4 || i == 12 {
                    self.display.write_static(" ");
                } else if i == 8 {
                    self.display.write_static("  ");
                }

                if i < remlen {
                    let val = self.buffer[(current_offset + i) as usize];
                    self.display.write(format!("{:02x} ", val));
                } else {
                    self.display.write_static("   ");
                }
            }

            self.display.write_static("| ");
            for i in 0..16 {
                let chr = if i < remlen {
                    self.buffer[(current_offset + i) as usize]
                } else {
                    32
                };

                if chr < 0x20 || chr > 0x7e {
                    self.display.write_static(".");
                } else {
                    self.display.write(format!("{}", chr as char));
                }
            }

            self.display.write_static("\n");
            current_output_y += 1;

            current_offset += 16;
            if remlen >= 16 {
                remlen -= 16;
            } else {
                remlen = 0;
            }
        }

        while current_output_y < display_lines - 2 {
            self.display.write_static(
                "                 \
                 |                                                     \
                 |                 \n");
            current_output_y += 1;
        }

        self.display.write_static("————————————————————————————————————————————\
                                   ————————————————————————————————————————————\
                                   —\n");

        // FIXME (hard-coded mode and position)
        self.display.write(format!(" {:>15}{:55}{:>#18x}", "READ", "", 0));

        self.display.flush();

        Ok(())
    }
}
