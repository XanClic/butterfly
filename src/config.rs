use serde_json;
use std;
use std::collections::HashMap;
use std::io::Seek;


#[derive(Serialize, Deserialize)]
struct Config {
    files: HashMap<String, CfgEntryFile>,
    structs: Vec<CfgEntryStruct>,
}

#[derive(Serialize, Deserialize)]
struct CfgEntryFile {
    undo_file_name: String,
}

#[derive(Serialize, Deserialize)]
pub struct CfgEntryStruct {
    pub path: String,
}


pub struct ConfigFile {
    file: std::fs::File,
    config: Config,
}

const CONFIG_FILE: &'static str = "config.json";


impl Config {
    pub fn new() -> Self {
        Config {
            files: HashMap::<String, CfgEntryFile>::new(),
            structs: Vec::<CfgEntryStruct>::new(),
        }
    }
}


pub fn base_dir() -> Result<std::path::PathBuf, String> {
    let mut path = match std::env::home_dir() {
        Some(p) => p,
        None    => return Err(String::from("Home directory unknown"))
    };

    path.push(".butterfly");
    Ok(path)
}


impl CfgEntryFile {
    pub fn new() -> Result<Self, String> {
        let mut i = 0u64;

        // Maybe we should skip through the config file values instead of
        // testing which files exist
        let mut base_path = base_dir()?;
        while i < 0xffffffffffffffffu64 {
            base_path.push(format!("undo-{}", i));
            if !base_path.as_path().exists() {
                break;
            }
            base_path.pop();

            i += 1;
        }

        if i == 0xffffffffffffffffu64 {
            panic!("undo file name space exhausted");
        }

        Ok(CfgEntryFile {
            undo_file_name: format!("undo-{}", i),
        })
    }
}


impl ConfigFile {
    pub fn new() -> Result<Self, String> {
        let mut path = base_dir()?;
        match std::fs::create_dir_all(std::path::Path::new(&path)) {
            Ok(_)   => (),
            Err(e)  => return Err(format!("Failed to create {:?}: {}", path, e))
        };

        path.push(CONFIG_FILE);
        let mut options = std::fs::OpenOptions::new();
        options.read(true).write(true).create(true);
        let mut file = match options.open(&path) {
            Ok(f)   => f,
            Err(e)  => return Err(format!("{:?}: {}", path, e))
        };

        let config: Config;
        if match file.seek(std::io::SeekFrom::End(0)) {
                Ok(r)   => r,
                Err(e)  => return Err(format!("{:?}: {}", path, e))
            } == 0
        {
            config = Config::new();
            if let Err(e) = serde_json::to_writer_pretty(&mut file, &config) {
                return Err(format!("{:?}: {}", path, e));
            }
        } else {
            if let Err(e) = file.seek(std::io::SeekFrom::Start(0)) {
                return Err(format!("{:?}: {}", path, e));
            }

            config = match serde_json::from_reader(&mut file) {
                Ok(c)   => c,
                Err(e)  => return Err(format!("{:?}: {}", path, e))
            };
        }

        Ok(ConfigFile {
            file: file,
            config: config,
        })
    }

    fn update(&mut self) -> Result<(), String> {
        if let Err(e) = self.file.seek(std::io::SeekFrom::Start(0)) {
            return Err(format!("Config file seek: {}", e));
        }

        // Write first, truncate later (should be safer)
        if let Err(e) = serde_json::to_writer_pretty(&mut self.file,
                                                     &self.config)
        {
            return Err(format!("Updating config file: {}", e));
        }

        let loc = match self.file.seek(std::io::SeekFrom::Current(0)) {
            Ok(loc) => loc,
            Err(e)  => return Err(format!("Inquiring end of config file: {}",
                                          e))
        };

        if let Err(e) = self.file.set_len(loc) {
            return Err(format!("Truncating config file: {}", e));
        }

        Ok(())
    }

    pub fn get_undo_filename(&mut self, for_filename: &String)
        -> Result<String, String>
    {
        if !self.config.files.contains_key(for_filename) {
            self.config.files.insert(for_filename.clone(),
                                     CfgEntryFile::new()?);
            self.update()?;
        }

        let file_entry = &self.config.files.get(for_filename).unwrap();
        let mut base_path = base_dir()?;
        base_path.push(&file_entry.undo_file_name);
        Ok(base_path.as_path().to_string_lossy().into_owned())
    }

    pub fn get_structs(&self) -> &Vec<CfgEntryStruct> {
        &self.config.structs
    }
}
