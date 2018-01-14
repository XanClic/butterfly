use std;
use std::io::{Read,Seek,Write};


pub struct File {
    file: std::fs::File,
    filename: String,
    writable: bool
}

impl File {
    pub fn new(filename: String) -> Result<Self, String> {
        // Holy shit this is stupid
        let mut options = std::fs::OpenOptions::new();
        let file = match options.read(true).open(&filename) {
            Ok(r)   => r,
            Err(e)  => return Err(format!("{}: {}", filename, e))
        };

        Ok(File {
            file: file,
            filename: filename,
            writable: false,
        })
    }

    pub fn read(&mut self, position: u64, buffer: &mut Vec<u8>)
        -> Result<(), String>
    {
        match self.file.seek(std::io::SeekFrom::Start(position)) {
            Ok(_)   => (),
            Err(e)  => return Err(format!("Failed to seek to {}: {}",
                                          position, e))
        };

        let read_len = match self.file.read(buffer.as_mut_slice()) {
            Ok(r)   => r,
            Err(e)  => return Err(format!("Failed to read: {}", e))
        };
        if read_len < buffer.len() {
            return Err(String::from("Short read"));
        }
        Ok(())
    }

    pub fn write_u8(&mut self, position: u64, byte: u8) -> Result<(), String> {
        if !self.writable {
            // Hoooly shit this is stupid
            let mut options = std::fs::OpenOptions::new();
            options.read(true).write(true);
            let file = match options.open(&self.filename) {
                Ok(r)   => r,
                Err(e)  =>
                    return Err(format!("Failed to make the file writable: {}",
                                       e))
            };

            self.file = file;
            self.writable = true;
        }

        match self.file.seek(std::io::SeekFrom::Start(position)) {
            Ok(_)   => (),
            Err(e)  => return Err(format!("Failed to seek to {}: {}",
                                          position, e))
        };

        let buffer: [u8; 1] = [byte];
        let written_len = match self.file.write(&buffer) {
            Ok(r)   => r,
            Err(e)  => return Err(format!("Failed to write: {}", e))
        };
        if written_len < 1 {
            return Err(String::from("Did not write anything"));
        }
        Ok(())
    }

    pub fn len(&mut self) -> Result<u64, String> {
        match self.file.seek(std::io::SeekFrom::End(0)) {
            Ok(r)   => Ok(r),
            Err(e)  => Err(format!("Failed to inquire file length: {}", e))
        }
    }
}
