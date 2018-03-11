use config::ConfigFile;
use display::{Color, Display};
use file::File;
use std;
use std::num::Wrapping;


pub struct StructCode {
    buffer: Vec<u8>,
}

pub struct Struct {
    name: String,
    code: StructCode,
}

pub struct Structs {
    list: Vec<Struct>,
}


impl Structs {
    pub fn load(cfg: &ConfigFile) -> Result<Self, String> {
        let mut structs = Vec::<Struct>::new();

        for (name, cs) in cfg.get_structs().iter() {
            let mut file = File::new(cs.path.clone())?;
            let len = file.len()?;

            let mut buffer = Vec::new();
            buffer.resize(len as usize, 0);

            file.read(0, &mut buffer)?;

            let s = Struct {
                name: name.clone(),
                code: StructCode {
                    buffer: buffer,
                },
            };

            structs.push(s);
        }

        Ok(Structs {
            list: structs,
        })
    }

    pub fn len(&self) -> usize {
        self.list.len()
    }

    pub fn get(&self, i: usize) -> &Struct {
        &self.list[i]
    }

    pub fn get_mut(&mut self, i: usize) -> &mut Struct {
        &mut self.list[i]
    }
}


impl Struct {
    pub fn get_name(&self) -> &str {
        self.name.as_ref()
    }

    pub fn update(&mut self, file: &mut File, loc: u64,
                  display: &mut Display, start_x: usize, start_y: usize)
        -> Result<(), String>
    {
        let height = display.h() as usize;
        let mut y = start_y;

        let mut last_output_was_not_header = false;

        let mut pc = 0;
        let mut file_be = false;

        let mut stack = Vec::<u64>::new();
        let mut sstack = Vec::<String>::new();

        let mut wram = Vec::<u64>::new();

        loop {
            if pc >= self.code.buffer.len() {
                break;
            }

            let opcode = self.code.buffer[pc];
            pc += 1;

            match opcode {
                0x00 => { // stop
                    break
                },

                0x01 => { // Switch endianness
                    let mode = self.code.buffer[pc];
                    pc += 1;

                    match mode {
                        0x00 => { // f2le
                            file_be = false;
                        },

                        0x01 => { //f2be
                            file_be = true;
                        },

                        _ => {
                            return Err(format!("Unknown opcode {:x} {:x}",
                                               opcode, mode))
                        }
                    }
                },


                0x10 => { // lic <constant>
                    let c = self.load_constant_u64(pc);
                    pc += 8;

                    stack.push(c);
                },

                0x12 => { // lsc <constant>
                    let len = self.load_constant_u64(pc);
                    pc += 8;

                    let (string, bytelen) =
                        self.load_constant_utf8_string(pc, Some(len))?;
                    pc += bytelen;

                    sstack.push(string);
                },

                0x14 => { // lic $LOC
                    stack.push(loc);
                },

                0x18 => { // Load integer from file
                    let subfunc = self.code.buffer[pc];
                    pc += 1;

                    let offset = self.stack_pop(&mut stack)?;
                    let (len, sign): (usize, bool) = match subfunc {
                        0x00 => (8, false), // flu64
                        0x01 => (8, false), // fli64 (same as flu64)
                        0x02 => (4, false), // flu32
                        0x03 => (4, true),  // fli32
                        0x04 => (2, false), // flu16
                        0x05 => (2, true),  // fli16
                        0x06 => (1, false), // flu8
                        0x07 => (1, true),  // fli8

                        _ => {
                            return Err(format!("Unknown opcode {:x} {:x}",
                                               opcode, subfunc))
                        }
                    };

                    let mut val = 0u64;
                    for i in 0..len {
                        let ofs = offset + i as u64;

                        if file_be {
                            val <<= 8;
                            val |= file.read_u8(ofs)? as u64;
                        } else {
                            val |= (file.read_u8(ofs)? as u64) << (i * 8);
                        }
                    }

                    if sign && ((val >> (len * 8 - 1)) & 1) != 0 {
                        // Sign extension
                        val |= std::u64::MAX - ((1u64 << (len * 8)) - 1);
                    }

                    stack.push(val);
                },

                0x1a => { // Load string from file
                    let subfunc = self.code.buffer[pc];
                    pc += 1;

                    let (string, _) = match subfunc {
                        0x00 => { // flsutf8null
                            let offset = self.stack_pop(&mut stack)?;

                            self.load_file_utf8_string(file, offset, None,
                                                       true)?
                        },

                        0x01 => { // flsutf8sized
                            let len = self.stack_pop(&mut stack)?;
                            let offset = self.stack_pop(&mut stack)?;

                            self.load_file_utf8_string(file, offset, Some(len),
                                                       true)?
                        },

                        0x02 => { // flsasciinull
                            let offset = self.stack_pop(&mut stack)?;

                            self.load_file_utf8_string(file, offset, None,
                                                       false)?
                        },

                        0x03 => { // flsasciisized
                            let len = self.stack_pop(&mut stack)?;
                            let offset = self.stack_pop(&mut stack)?;

                            self.load_file_utf8_string(file, offset, Some(len),
                                                       false)?
                        },

                        _ => {
                            return Err(format!("Unknown opcode {:x} {:x}",
                                               opcode, subfunc))
                        }
                    };

                    sstack.push(string);
                },

                0x1c => { // sli
                    let address = self.stack_pop(&mut stack)? as usize;
                    stack.push(wram[address]);
                },


                0x28 => { // Output integer
                    let subfunc = self.code.buffer[pc];
                    pc += 1;

                    let base = self.code.buffer[pc] as usize;
                    pc += 1;

                    let name = self.stack_pop(&mut sstack)?;
                    let value = self.stack_pop(&mut stack)?;
                    let _orig_offset = self.stack_pop(&mut stack)?;

                    if y + 1 > height {
                        break;
                    }
                    display.set_cursor_pos(start_x, y);
                    display.clear_line();
                    y += 1;

                    let string = match subfunc {
                        0x00 => self.format_int(value, false, base), // osu
                        0x01 => self.format_int(value, true,  base), // osi

                        _ => {
                            return Err(format!("Unknown opcode {:x} {:x}",
                                               opcode, subfunc))
                        }
                    };

                    display.write(format!("{}: {}", name, string));

                    last_output_was_not_header = true;
                },

                0x2a => { // Output string
                    let subfunc = self.code.buffer[pc];
                    pc += 1;

                    let name = self.stack_pop(&mut sstack)?;
                    let value = self.stack_pop(&mut sstack)?;
                    let _orig_offset = self.stack_pop(&mut stack)?;

                    if y + 1 > height {
                        break;
                    }
                    display.set_cursor_pos(start_x, y);
                    display.clear_line();
                    y += 1;

                    match subfunc {
                        0x00 => display.write(format!("{}: {}", name, value)),

                        _ => {
                            return Err(format!("Unknown opcode {:x} {:x}",
                                               opcode, subfunc))
                        }
                    }

                    last_output_was_not_header = true;
                },

                0x2b => { // oh<level>
                    let level = self.code.buffer[pc];
                    pc += 1;

                    let title = self.stack_pop(&mut sstack)?;

                    if last_output_was_not_header {
                        if y + 1 > height {
                            break;
                        }
                        display.set_cursor_pos(start_x, y);
                        display.clear_line();
                        y += 1;
                    }

                    if y + 2 > height {
                        break;
                    }
                    display.set_cursor_pos(start_x, y);
                    display.clear_line();
                    y += 1;
                    display.set_cursor_pos(start_x, y);
                    display.clear_line();
                    y += 1;

                    display.set_cursor_pos(start_x, y - 2);

                    let color = if level == 0 {
                        Color::StructH0
                    } else if level == 1 {
                        Color::StructH1
                    } else if level == 2 {
                        Color::StructH2
                    } else {
                        Color::StructH3P
                    };
                    display.color_on_ref(&color);
                    display.write(title);
                    display.color_off_ref(&color);

                    last_output_was_not_header = false;

                    // TODO (Current: always display)
                    stack.push(1u64);
                },

                0x2c => { // ssi
                    let address = self.stack_pop(&mut stack)? as usize;
                    let value = self.stack_pop(&mut stack)?;

                    if address >= wram.len() {
                        wram.resize(address + 1, 0);
                    }
                    wram[address] = value;
                },


                0x80 => { // iswap
                    let x = self.stack_pop(&mut stack)?;
                    let y = self.stack_pop(&mut stack)?;
                    stack.push(x);
                    stack.push(y);
                },

                0x81 => { // idup
                    let x = self.stack_pop(&mut stack)?;
                    stack.push(x);
                    stack.push(x);
                },

                0x82 => { // idrop
                    self.stack_pop(&mut stack)?;
                },

                0x83 => { // ineg
                    let x = self.stack_pop(&mut stack)?;
                    stack.push((Wrapping(0u64) - Wrapping(x)).0);
                },

                0x84 => { // iadd
                    let x = self.stack_pop(&mut stack)?;
                    let y = self.stack_pop(&mut stack)?;
                    stack.push((Wrapping(x) + Wrapping(y)).0);
                },

                0x85 => { // iand
                    let x = self.stack_pop(&mut stack)?;
                    let y = self.stack_pop(&mut stack)?;
                    stack.push(x & y);
                },


                0xe0 => { // jmp <target>
                    let c = self.load_constant_u64(pc);
                    pc -= 1; // Go back before the instruction
                    pc = (Wrapping(pc as u64) + Wrapping(c)).0 as usize;
                },

                0xe1 => { // jz <target>
                    let c = self.load_constant_u64(pc);
                    pc += 8;

                    if self.stack_pop(&mut stack)? == 0 {
                        pc -= 9; // Go back before the instruction

                        pc = (Wrapping(pc as u64) + Wrapping(c)).0 as usize;
                    }
                },

                0xe2 => { // jnz <target>
                    let c = self.load_constant_u64(pc);
                    pc += 8;

                    if self.stack_pop(&mut stack)? != 0 {
                        pc -= 9; // Go back before the instruction

                        pc = (Wrapping(pc as u64) + Wrapping(c)).0 as usize;
                    }
                },

                0xe3 => { // jnn <target>
                    let c = self.load_constant_u64(pc);
                    pc += 8;

                    if self.stack_pop(&mut stack)? >> 63 == 0 {
                        pc -= 9; // Go back before the instruction

                        pc = (Wrapping(pc as u64) + Wrapping(c)).0 as usize;
                    }
                },


                0xff => { // panic
                    let mut string = String::from("Stack:");

                    while let Some(v) = stack.pop() {
                        string += format!(" {:#x}", v).as_ref();
                    }

                    return Err(string)
                },


                _ => {
                    return Err(format!("Unkown opcode {:x}", opcode))
                }
            }
        }

        Ok(())
    }

    fn assert(&self, res: bool, errstr: String) -> Result<(), String> {
        if res {
            Ok(())
        } else {
            Err(errstr)
        }
    }

    fn stack_pop<T>(&self, stack: &mut Vec<T>) -> Result<T, String> {
        match stack.pop() {
            Some(x) => Ok(x),
            None    => Err(String::from("Stack ran out"))
        }
    }

    fn format_int(&self, mut val: u64, signed: bool, base: usize) -> String {
        if base > 36 {
            panic!("Base must not exceed 36, but is {}", base);
        }

        if val == 0 {
            return String::from("0");
        }

        let sign = if signed {
            if val >> 63 != 0 {
                val = std::u64::MAX - val + 1;
                "-"
            } else {
                ""
            }
        } else {
            ""
        };

        let mut ret = String::new();
        while val > 0 {
            let digit = val % (base as u64);
            val /= base as u64;

            ret.insert(0, if digit < 10 {
                (digit as u8 + '0' as u8) as char
            } else {
                (digit as u8 - 10 + 'a' as u8) as char
            });
        }

        if base == 2 {
            ret.insert_str(0, "0b");
        } else if base == 8 {
            ret.insert_str(0, "0o");
        } else if base == 16 {
            ret.insert_str(0, "0x");
        } else if base != 10 {
            ret.insert_str(0, format!("0[{}]", base).as_ref());
        }
        ret.insert_str(0, sign);
        return ret;
    }

    fn load_constant_u64(&self, pc: usize) -> u64 {
        let mut val = 0u64;
        for i in 0..8 {
            val |= (self.code.buffer[pc + i] as u64) << (i * 8);
        }
        return val;
    }

    fn load_constant_utf8_string(&self, pc: usize, len: Option<u64>)
        -> Result<(String, usize), String>
    {
        let mut string = String::new();
        let mut rem = match len {
            Some(x) => x as usize,
            None    => std::usize::MAX,
        };
        let mut i = 0;

        while rem > 0 {
            let mut codepoint: u32;
            let mut tail_length: usize;

            let start = self.code.buffer[pc + i];
            i += 1;

            if start & 0x80 == 0x00 {
                codepoint = start as u32;
                tail_length = 0;
            } else if start & 0xe0 == 0xc0 {
                codepoint = ((start & 0x1f) as u32) << 6;
                tail_length = 1;
            } else if start & 0xf0 == 0xe0 {
                codepoint = ((start & 0x0f) as u32) << 12;
                tail_length = 2;
            } else if start & 0xf8 == 0xf0 {
                codepoint = ((start & 0x07) as u32) << 18;
                tail_length = 3;
            } else {
                return Err(String::from("Invalid utf-8 string constant"));
            }

            while tail_length > 0 {
                let byte = self.code.buffer[pc + i];
                self.assert(byte & 0xc0 == 0x80,
                            String::from("Invalid utf-8 string constant"))?;

                tail_length -= 1;
                codepoint |= ((byte & 0x3f) as u32) << (tail_length * 6);
                i += 1;
            }

            if len.is_none() && codepoint == 0 {
                break;
            }

            match std::char::from_u32(codepoint) {
                Some(c) => string.push(c),
                None    =>
                    return Err(String::from("Invalid utf-8 string constant")),
            };

            rem -= 1;
        }

        return Ok((string, i));
    }

    fn load_file_utf8_string(&self, file: &mut File, offset: u64,
                             len: Option<u64>, utf8: bool)
        -> Result<(String, usize), String>
    {
        let mut string = String::new();
        let mut rem = match len {
            Some(x) => x as usize,
            None    => std::usize::MAX,
        };
        let mut i = 0;

        while rem > 0 {
            let mut codepoint: u32;
            let mut tail_length: usize;

            let start = file.read_u8(offset + i)?;
            i += 1;

            if start & 0x80 != 0x00 && !utf8 {
                return Err(String::from("Invalid ASCII string"));
            }

            if start & 0x80 == 0x00 {
                codepoint = start as u32;
                tail_length = 0;
            } else if start & 0xe0 == 0xc0 {
                codepoint = ((start & 0x1f) as u32) << 6;
                tail_length = 1;
            } else if start & 0xf0 == 0xe0 {
                codepoint = ((start & 0x0f) as u32) << 12;
                tail_length = 2;
            } else if start & 0xf8 == 0xf0 {
                codepoint = ((start & 0x07) as u32) << 18;
                tail_length = 3;
            } else {
                return Err(String::from("Invalid utf-8 string"));
            }

            while tail_length > 0 {
                let byte = file.read_u8(offset + i)?;
                self.assert(byte & 0xc0 == 0x80,
                            String::from("Invalid utf-8 string"))?;

                tail_length -= 1;
                codepoint |= ((byte & 0x3f) as u32) << (tail_length * 6);
                i += 1;
            }

            if len.is_none() && codepoint == 0 {
                break;
            }

            match std::char::from_u32(codepoint) {
                Some(c) => string.push(c),
                None    =>
                    return Err(String::from("Invalid utf-8 string")),
            };

            rem -= 1;
        }

        return Ok((string, i as usize));
    }
}
