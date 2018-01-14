use config::ConfigFile;
use std;
use std::io::{Read,Seek,Write};


pub struct UndoFile {
    file: std::fs::File,
    loc: u64,
    lof: u64,
}

fn seek(file: &mut std::fs::File, pos: std::io::SeekFrom) -> Result<u64, String>
{
    match file.seek(pos) {
        Ok(r)   => Ok(r),
        Err(e)  => Err(format!("Failed to seek: {}", e))
    }
}

fn truncate(file: &mut std::fs::File, len: u64) -> Result<(), String> {
    match file.set_len(len) {
        Ok(_)   => Ok(()),
        Err(e)  => Err(format!("Failed to truncate: {}", e))
    }
}

fn flush(file: &mut std::fs::File) -> Result<(), String> {
    match file.flush() {
        Ok(_)   => Ok(()),
        Err(e)  => Err(format!("Failed to flush: {}", e))
    }
}

fn read8(file: &mut std::fs::File) -> Result<u8, String> {
    let mut buffer: [u8; 1] = [0];
    let read_len = match file.read(&mut buffer) {
        Ok(r)   => r,
        Err(e)  => return Err(format!("Failed to read: {}", e))
    };

    if read_len < 1 {
        return Err(String::from("Short read"));
    }
    Ok(buffer[0])
}

fn read64(file: &mut std::fs::File) -> Result<u64, String> {
    let mut buffer: [u8; 8] = [0; 8];
    let read_len = match file.read(&mut buffer) {
        Ok(r)   => r,
        Err(e)  => return Err(format!("Failed to read: {}", e))
    };

    if read_len < 8 {
        return Err(String::from("Short read"));
    }
    Ok(u64::from_le(unsafe { std::mem::transmute(buffer) }))
}

fn write8(file: &mut std::fs::File, val: u8) -> Result<(), String> {
    let buffer: [u8; 1] = [val];
    let written_len = match file.write(&buffer) {
        Ok(w)   => w,
        Err(e)  => return Err(format!("Failed to write: {}", e))
    };

    if written_len < 1 {
        return Err(String::from("Short write"));
    }
    Ok(())
}

fn write64(file: &mut std::fs::File, val: u64) -> Result<(), String> {
    let buffer: [u8; 8] = unsafe {
        std::mem::transmute(val.to_le())
    };
    let written_len = match file.write(&buffer) {
        Ok(w)   => w,
        Err(e)  => return Err(format!("Failed to write: {}", e))
    };

    if written_len < 8 {
        return Err(String::from("Short write"));
    }
    Ok(())
}

/*
 * Undo file structure (all little-endian):
 *
 * Offset 0x0: Header
 *   - +0x0: 4 bytes magic: "undo"
 *   - +0x4: u8 version: 0
 *   - +0x5: 3 bytes reserved
 *   - +0x8: u64 current position in file
 *           (must be aligned to 0x10, may not be less than 0x10, and may not
 *            exceed the file length)
 *
 * Offset 0x10: Data
 *   Every data block has a size of 16 bytes.
 *   - +0x0: u64 modified address
 *   - +0x8: u8 old byte
 *   - +0x9: u8 new byte
 *   - +0xa: 6 bytes reserved
 *
 * When performing a change, a new data block describing it is written at the
 * current position and the file is truncated beyond this block.  The position
 * is updated to point to the next block (the EOF).
 *
 * When performing an undo, the position is updated to point at the previous
 * block and the information therein as read and used to perform the undo.
 * If the position is 0x10, no undo is possible.
 *
 * When performing a redo, the block at the current position is read and the
 * information therein is used to perform the redo.  The position is then
 * updated to point to the next block.
 * If the position is the EOF, no redo is possible.
 */

impl UndoFile {
    pub fn new(config: &mut ConfigFile, for_filename: String) -> Result<Self, String> {
        let fname = config.get_undo_filename(&for_filename)?;
        let mut options = std::fs::OpenOptions::new();
        options.read(true).write(true).create(true);
        let mut file = match options.open(&fname) {
            Ok(f)   => f,
            Err(e)  => return Err(format!("{}: {}", fname, e))
        };

        let loc;
        let mut lof = seek(&mut file, std::io::SeekFrom::End(0))?;
        seek(&mut file, std::io::SeekFrom::Start(0))?;

        if lof == 0 {
            // Magic
            write8(&mut file, 'u' as u8)?;
            write8(&mut file, 'n' as u8)?;
            write8(&mut file, 'd' as u8)?;
            write8(&mut file, 'o' as u8)?;

            // Version
            write8(&mut file, 0)?;

            // Reserved
            write8(&mut file, 0)?;
            write8(&mut file, 0)?;
            write8(&mut file, 0)?;

            // Current position
            loc = 0x10;
            write64(&mut file, loc)?;

            flush(&mut file)?;

            lof = 0x10;
        } else {
            if lof % 0x10 != 0 {
                lof = (lof + 0xf) & !0xf;
                truncate(&mut file, lof)?;
            }

            if read8(&mut file)? != 'u' as u8 ||
               read8(&mut file)? != 'n' as u8 ||
               read8(&mut file)? != 'd' as u8 ||
               read8(&mut file)? != 'o' as u8
            {
                return Err(format!("{}: Not an undo file", fname));
            }

            let ver = read8(&mut file)?;
            if ver != 0 {
                return Err(format!("{}: Unsupported version {}", fname, ver));
            }

            seek(&mut file, std::io::SeekFrom::Start(0x8))?;
            loc = read64(&mut file)?;

            if loc % 0x10 != 0 || loc < 0x10 || loc > lof {
                return Err(format!("{}: Invalid position {:#x}", fname, loc));
            }
            seek(&mut file, std::io::SeekFrom::Start(loc))?;
        }

        Ok(UndoFile {
            file: file,
            loc: loc,
            lof: lof,
        })
    }


    /*
     * OK, let me explain.
     *
     * For normal operations, you do this:
     *   1. undo_file.enter(addr, old, new)?;
     *   2. binary_file_modify()?;
     *   3. undo_file.settle()?;
     *
     * For undos, you do this:
     *   1. (addr, old) = undo_file.undo()?;
     *   2. binary_file_modify()?;
     *   3. undo_file.settle()?;
     *
     * For redos, you do this:
     *   1. (addr, new) = undo_file.redo()?;
     *   2. binary_file_modify()?;
     *   3. undo_file.settle()?;
     *
     * This should allow the undo file to generally stay consistent even in case
     * of errors, and allow the user to undo/redo things if modifying the file
     * itself failed somehow.
     *
     * Note that the "normal operation" order is different from what has been
     * explained above: Here we enter the operation before actually doing this.
     * That is better because if 2 fails, the user can still redo it from the
     * undo log.  If it were the other way around and 1 failed (after 2), you
     * would not be able to undo that operation (and thus the log would be
     * inconsistent).
     *
     * Generally, if 1 or 2 fails, that is fine, the log stays consistent.  If
     * 3 fails, that is different.  You can pretty much only tell the user that
     * the log is inconsistent and that they should proceed with extra care.
     * (Maybe throw it away if they can afford it.)
     */
    pub fn settle(&mut self) -> Result<(), String> {
        match self.do_settle() {
            Ok(_)   => Ok(()),
            Err(e)  =>
                Err(format!("{} – the log is inconsistent now, proceed with \
                             care!", e))
        }
    }

    fn do_settle(&mut self) -> Result<(), String> {
        seek(&mut self.file, std::io::SeekFrom::Start(0x8))?;
        write64(&mut self.file, self.loc)?;
        flush(&mut self.file)?;

        Ok(())
    }

    pub fn enter(&mut self, address: u64, old: u8, new: u8)
        -> Result<(), String>
    {
        match self.do_enter(address, old, new) {
            Ok(_)   => Ok(()),
            Err(e)  => Err(format!("{} (redo may be garbage)", e))
        }
    }

    fn do_enter(&mut self, address: u64, old: u8, new: u8)
        -> Result<(), String>
    {
        seek(&mut self.file, std::io::SeekFrom::Start(self.loc))?;
        write64(&mut self.file, address)?;
        write8(&mut self.file, old)?;
        write8(&mut self.file, new)?;

        self.loc += 0x10;
        self.lof = self.loc;
        truncate(&mut self.file, self.lof)?;

        Ok(())
    }

    pub fn undo(&mut self) -> Result<Option<(u64, u8)>, String> {
        match self.do_undo() {
            Ok(r)   => Ok(r),
            Err(e)  => Err(format!("{} (log is unchanged)", e))
        }
    }

    fn do_undo(&mut self) -> Result<Option<(u64, u8)>, String> {
        if self.loc == 0x10 {
            return Ok(None);
        }

        self.loc -= 0x10;
        seek(&mut self.file, std::io::SeekFrom::Start(self.loc))?;
        let address = read64(&mut self.file)?;
        let old = read8(&mut self.file)?;

        Ok(Some((address, old)))
    }

    pub fn redo(&mut self) -> Result<Option<(u64, u8)>, String> {
        match self.do_redo() {
            Ok(r)   => Ok(r),
            Err(e)  => Err(format!("{} (log is unchanged)", e))
        }
    }

    pub fn do_redo(&mut self) -> Result<Option<(u64, u8)>, String> {
        if self.loc == self.lof {
            return Ok(None);
        }

        seek(&mut self.file, std::io::SeekFrom::Start(self.loc))?;
        let address = read64(&mut self.file)?;
        read8(&mut self.file)?;
        let new = read8(&mut self.file)?;
        self.loc += 0x10;

        Ok(Some((address, new)))
    }
}
