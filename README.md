butterfly
=========

One day I wanted to edit my md RAID's superblock, but my go-to hex editor at the
time (Okteta) did not want to allow me to edit more than 2 GB.

How hard can it be?


“Notable” features
------------------

Note that all of the following are written in indicative mood, i.e. “does X”,
but they are actually meant as “should do X, and if it does not, that is a bug”.

- Works with arbitrarily large files while using few resources
- Infinite and persistent undo/redo (files are remembered based on their
  realpath)
- Will not modify a file until you explicitly change from the default “READ”
  mode into some other (currently “MODIFY” and “REPLACE”)
- Modifications are carried out instantly (not sure if that is a feature, but
  that is how it is right now)
- Structure definitions through a stupidly complicated turing-complete (I know
  this is a bad thing) byte code interpreter (op code list in
  `doc/struct-opcodes`)


Tips on using it
================

You may want to clone
[my dot-butterfly repository](https://github.com/XanClic/dot-butterfly), build
it and and move the result into `~/.butterfly` (or even better: link it) before
launching butterfly so that you have access to all of the structure definitions.


TODO
====

- [x] Simple status line (with different modes, i.e. “READ” and “REPLACE”)
- [x] Cursor movement (with main loop)
- [x] Additional char-column cursor
- [x] Scrolling
- [x] Proper commands (e.g. for goto)
- [x] Actual replacement (“REPLACE”) + Writing
      (No need for buffering with infinite undo, and performance-wise, who cares.
       User input is slow anyway.)
- [x] Jump stack (^T)
- [x] Infinite undo by writing modification steps into some file
- [x] Mouse support
- [x] Data display: u8, i8, LE/BE, ... (hex in LE) – this is the `scalars`
      structure now (use it through `:struct scalars`)
- [x] Structures: User should be able to define structures in JSON files – this
      is considered complete when I have a usable qcow2 definition
      (for this, I will need links (“this value is an offset for that value”))
- [x] Structure section folding
- [ ] Do not update structure views that do not depend on the cursor position
      when moving the cursor
- [x] Structure highlighting: When you click on a value, it should be
      highlighted in the data stream
- [ ] Be able to display the list of installed structs (this requires some way
      for commands to display a lengthy output, which would be quite nice to
      implement a :help also).
- [ ] Allow appending
- [ ] Proper command separation: Currently, all command logic and data is kept
      in src/buffer.rs.  That needs to change.
- [ ] Find things: Every website has this now, so we need that, too
- [ ] Overwrite unusable undo files: Instead of just aborting or doing random
      things when some undo file cannot be read, we should just overwrite it
      (or maybe tell the user where it is and create a new one, so if they want
       to debug the issue...)
- [ ] Also, allow the user to discard specific undo history (e.g. “everything
      more than 1000 steps ago”)
- [ ] Make some things configurable (themes, scroll length, etc.)
- [ ] Proper terminfo support (for arbitrary escape sequences at least)
- [ ] Compress prehistoric undo history to save space
