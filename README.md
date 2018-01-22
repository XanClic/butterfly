butterfly
=========

One day I wanted to edit my md RAID's superblock, but my go-to hex editor at the
time (Okteta) did not want to allow me to edit more than 2 GB.

How hard can it be?


TODO
====

- [x] Simple status line (with different modes, i.e. "READ" and "REPLACE")
- [x] Cursor movement (with main loop)
- [x] Additional char-column cursor
- [x] Scrolling
- [x] Proper commands (e.g. for goto)
- [x] Actual replacement ("REPLACE") + Writing
      (No need for buffering with infinite undo, and performance-wise, who cares.
       User input is slow anyway.)
- [x] Jump stack (^T)
- [x] Infinite undo by writing modification steps into some file
- [x] Mouse support
- [ ] Data display: u8, i8, LE/BE, ... (hex in LE)
- [ ] Structures: User should be able to define structures in JSON files – this
      is considered complete when I have a usable qcow2 definition
      (for this, I will need links ("this value is an offset for that value"))
- [ ] Structure highlighting: When you click on a value, it should be
      highlighted in the data stream
- [ ] Be able to display the list of installed structs
- [ ] Proper command separation: Currently, all command logic and data is kept
      in src/buffer.rs.  That needs to change.
- [ ] Find things: Every website has this now, so we need that, too
- [ ] Overwrite unusable undo files: Instead of just aborting or doing random
      things when some undo file cannot be read, we should just overwrite it
      (or maybe tell the user where it is and create a new one, so if they want
       to debug the issue...)
- [ ] Make some things configurable (themes, scroll length, etc.)
- [ ] Proper terminfo support (for arbitrary escape sequences at least)
