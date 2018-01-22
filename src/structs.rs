use config::{self, ConfigFile};
use display::{Color, Display};
use file::File;
use serde_json;
use std;
use std::ops::{Deref, DerefMut};


#[derive(Serialize, Deserialize)]
#[serde(tag = "type")]
enum StructDefinition {
    Folder(StructFolder),
    Value(StructValue),
}

enum StateDefinition {
    Folder(StateFolder),
    Value(StateValue),
}

#[derive(Serialize, Deserialize)]
struct StructFolder {
    name: String,
    content: Vec<Box<StructDefinition>>,
}

struct StateFolder {
    name: String,
    content: Vec<Box<StateDefinition>>,
}

#[derive(Serialize, Deserialize, Clone)]
struct StructValue {
    name: String,
    offset: StructValueOffset,
    kind: StructValueKind,
}

struct StateValue {
    value: Option<StateActualValue>,
    struct_corr: StructValue,
}

enum StateActualValue {
    Unsigned(u64),
    Signed(i64),
}

#[derive(Serialize, Deserialize, Clone)]
#[serde(tag = "type")]
enum StructValueOffset {
    Abs(StructValueOffsetAbs),
    Loc(StructValueOffsetLoc),
}

#[derive(Serialize, Deserialize, Clone)]
struct StructValueOffsetAbs {
    offset: u64,
}

#[derive(Serialize, Deserialize, Clone)]
struct StructValueOffsetLoc {
    offset: i64,
}

#[derive(Serialize, Deserialize, Clone)]
#[serde(tag = "type")]
enum StructValueKind {
    Integer(StructValueKindInteger),
}

#[derive(Serialize, Deserialize, Clone)]
struct StructValueKindInteger {
    width: usize,
    endianness: StructValueKindIntegerEndianness,
    sign: StructValueKindIntegerSign,
    base: usize,
}

#[derive(Serialize, Deserialize, Clone)]
enum StructValueKindIntegerEndianness {
    Little,
    Big,
}

#[derive(Serialize, Deserialize, Clone)]
enum StructValueKindIntegerSign {
    Unsigned,
    SignTwoCompl,
    SignOneCompl,
    SignBitValue,
    SignOffset(u64),
}


pub struct Struct {
    root: StructFolder,
    state: StateFolder,
}

pub struct Structs {
    list: Vec<Struct>,
}


impl Structs {
    pub fn load(cfg: &ConfigFile) -> Result<Self, String> {
        let mut structs = Vec::<Struct>::new();

        for cs in cfg.get_structs() {
            let folder = StructFolder::load(cs.path.as_ref())?;
            let state = StateFolder::new(&folder);

            let s = Struct {
                root: folder,
                state: state,
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


impl StructFolder {
    pub fn load(path: &str) -> Result<Self, String> {
        let mut full_path = config::base_dir()?;
        full_path.push(std::path::PathBuf::from(path));
        let mut options = std::fs::OpenOptions::new();
        let mut file = match options.read(true).open(&full_path) {
            Ok(f)   => f,
            Err(e)  => return Err(format!("Failed to load struct from {:?}: {}",
                                          full_path, e))
        };

        match serde_json::from_reader(&mut file) {
            Ok(s)   => Ok(s),
            Err(e)  => Err(format!("Failed to load struct from {:?}: {}",
                                   full_path, e))
        }
    }
}


impl Struct {
    pub fn get_name(&self) -> &str {
        self.root.name.as_ref()
    }

    pub fn update(&mut self, file: &mut File, loc: u64,
                  display: &mut Display, start_x: usize, mut start_y: usize)
        -> Result<(), String>
    {
        self.state.update(file, loc)?;
        self.state.print(display, start_x, &mut start_y, 0, true)?;

        Ok(())
    }
}


impl StateFolder {
    pub fn new(s: &StructFolder) -> Self {
        let mut content = Vec::<Box<StateDefinition>>::new();
        for def in &s.content {
            let element = match def.deref() {
                &StructDefinition::Folder(ref sf) =>
                    StateDefinition::Folder(StateFolder::new(sf)),

                &StructDefinition::Value(ref sv) =>
                    StateDefinition::Value(StateValue {
                        value: None,
                        struct_corr: sv.clone(),
                    }),
            };
            content.push(Box::new(element));
        }

        StateFolder {
            name: s.name.clone(),
            content: content,
        }
    }

    fn update(&mut self, file: &mut File, loc: u64) -> Result<(), String> {
        for state in &mut self.content {
            match state.deref_mut() {
                &mut StateDefinition::Folder(ref mut sf) =>
                    sf.update(file, loc)?,

                &mut StateDefinition::Value(ref mut val) => {
                    let corr = &val.struct_corr;
                    let mut offset = match corr.offset {
                        StructValueOffset::Abs(ref o)  => o.offset,
                        StructValueOffset::Loc(ref o)  => {
                            if o.offset < 0 {
                                loc + (-o.offset as u64)
                            } else {
                                loc + ( o.offset as u64)
                            }
                        },
                    };

                    match corr.kind {
                        StructValueKind::Integer(ref i) => {
                            if i.width > 8 {
                                return Err(format!("Integer width may not 
                                                    exceed 8"));
                            }

                            let mut raw = 0u64;
                            // OH GOD FIXME
                            for bi in 0..i.width {
                                let byte = file.read_u8(offset)? as u64;
                                offset += 1;
                                raw |= match i.endianness {
                                    StructValueKindIntegerEndianness::Little =>
                                        (byte as u64) << (bi * 8),

                                    StructValueKindIntegerEndianness::Big =>
                                        (byte as u64) <<
                                            ((i.width - bi - 1) * 8),
                                };
                            }

                            let msb_shift = i.width * 8 - 1;

                            val.value = Some(match i.sign {
                                StructValueKindIntegerSign::Unsigned =>
                                    StateActualValue::Unsigned(raw),

                                StructValueKindIntegerSign::SignTwoCompl => {
                                    let pv = if raw >> msb_shift == 0 {
                                        raw as i64
                                    } else {
                                        if msb_shift == 63 {
                                            -((0xffffffffffffffffu64 -
                                               raw) as i64) - 1
                                        } else {
                                            -(((1u64 << (msb_shift + 1)) -
                                               raw) as i64)
                                        }
                                    };
                                    StateActualValue::Signed(pv)
                                },

                                StructValueKindIntegerSign::SignOneCompl => {
                                    let pv = if raw >> msb_shift == 0 {
                                        raw as i64
                                    } else {
                                        if msb_shift == 63 {
                                            -((0xffffffffffffffffu64 -
                                               raw) as i64)
                                        } else {
                                            -(((1u64 << (msb_shift + 1)) -
                                               raw) as i64) + 1
                                        }
                                    };
                                    StateActualValue::Signed(pv)
                                },

                                StructValueKindIntegerSign::SignBitValue => {
                                    let pv = if raw >> msb_shift == 0 {
                                        raw as i64
                                    } else {
                                        -((raw & !(1u64 << msb_shift))
                                          as i64)
                                    };
                                    StateActualValue::Signed(pv)
                                },

                                StructValueKindIntegerSign::SignOffset(c) => {
                                    let pv = if raw >= c {
                                        (raw - c) as i64
                                    } else {
                                        -((c - raw) as i64)
                                    };
                                    StateActualValue::Signed(pv)
                                },
                            });
                        },
                    }
                },
            };
        }

        Ok(())
    }

    fn format_int(&self, val: &StateActualValue, base: usize) -> String {
        if base > 36 {
            panic!("Base must not exceed 36, but is {}", base);
        }

        let (sign, mut uval) = match val {
            &StateActualValue::Unsigned(ref u) => ("", *u),
            &StateActualValue::Signed(ref s) =>
                (if *s < 0 { "-" } else { "" },
                 if *s >= 0 {
                     *s as u64
                 } else if *s == -0x8000000000000000i64 {
                     0x8000000000000000u64
                 } else {
                     -*s as u64
                 }),
        };
        if uval == 0 {
            return String::from("0");
        }

        let mut ret = String::new();
        while uval > 0 {
            let digit = uval % (base as u64);
            uval /= base as u64;

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

    fn print(&self, display: &mut Display, start_x: usize, start_y: &mut usize,
             level: usize, first_child: bool)
        -> Result<(), String>
    {
        let height = display.h() as usize;

        if !first_child {
            *start_y += 1;
        }

        if *start_y + 2 > height {
            return Ok(());
        }
        display.set_cursor_pos(start_x, *start_y);
        display.clear_line();
        *start_y += 2;

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
        display.write_static(self.name.as_ref());
        display.color_off_ref(&color);

        let mut child_first_child = true;
        for state in &self.content {
            if *start_y + 1 > height {
                return Ok(());
            }

            match state.deref() {
                &StateDefinition::Folder(ref sf) =>
                    sf.print(display, start_x, start_y,
                             level + 1, child_first_child)?,

                &StateDefinition::Value(ref val) => {
                    display.set_cursor_pos(start_x, *start_y);
                    display.clear_line();
                    *start_y += 1;

                    let disp = match &val.struct_corr.kind {
                        &StructValueKind::Integer(ref i) =>
                            self.format_int(val.value.as_ref().unwrap(),
                                            i.base),
                    };
                    display.write(format!("{}: {}",
                                          val.struct_corr.name, disp));
                },
            }

            child_first_child = false;
        }

        Ok(())
    }
}
