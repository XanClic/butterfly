use std;
use std::error::Error;
use std::io::{Read,Seek};


pub struct File {
    file: std::fs::File,
}

impl File {
    pub fn new(filename: &std::string::String) -> Result<Self, String> {
        let file = match std::fs::File::open(filename) {
            Ok(r)   => r,
            Err(e)  => return Err(String::from(e.description()))
        };

        Ok(File {
            file: file,
        })
    }

    pub fn read(&mut self, position: u64, buffer: &mut Vec<u8>)
        -> Result<(), String>
    {
        match self.file.seek(std::io::SeekFrom::Start(position)) {
            Ok(_)   => (),
            Err(e)  => return Err(String::from(e.description()))
        };

        let read_len = match self.file.read(buffer.as_mut_slice()) {
            Ok(r)   => r,
            Err(e)  => return Err(String::from(e.description()))
        };
        if read_len < buffer.len() {
            return Err(String::from("Short read"));
        }
        Ok(())
    }

    pub fn len(&mut self) -> Result<u64, String> {
        match self.file.seek(std::io::SeekFrom::End(0)) {
            Ok(r)   => Ok(r),
            Err(e)  => Err(String::from(e.description()))
        }
    }
}
