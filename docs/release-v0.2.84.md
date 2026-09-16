# Agel v0.2.84 — Programs installed from files

The step the backend road needs. A program the OS runs comes from the
program region, which only the host's installer wrote; a program in the
OS could write a file and could not make it run. Now the desktop's
`:install NAME PATH` decodes hex text the filesystem service can read —
a file in the region, or under `/data` — into the program region, where
`:exec NAME` finds it. What a program in the OS writes can be a program
the OS runs; the Agel-written backend that writes such a file is the
rung after.

## What changed

- **`Region::install`** in the kernel: given a name and a source of hex
  text, it reads the table, finds the first sector past every entry,
  decodes the text sector by sector into those sectors (whitespace
  ignored, anything else refused), checksums the bytes with CRC-32 and
  writes the table last — a new row, or the row of that name replaced in
  place, its old sectors left as a hole the host's installer compacts. It
  holds one sector and one chunk of text, whatever the file's size. Its
  I/O is a trait: the sectors through the storage driver, the text
  through whatever reads it.
- **`:install NAME PATH`** on the desktop implements that trait over the
  effect host: the text through the filesystem service in chunks
  (`read_file_from`, a file read from an offset), the sectors through the
  driver. It answers `INSTALLED NAME: N BYTES AT SECTOR S`, `NO SUCH
  FILE`, or the refusal's reason.
- **Hex text** is the form because the language's texts are UTF-8 and a
  program is bytes: two digits a byte, lines of any length.

## Proof

`scripts/test-install.sh`: the host writes the `hello` program's ELF as
hex into the data region; on the desktop `:install hello2
/data/hello.hex` answers `INSTALLED hello2: 8920 BYTES AT SECTOR …` and
`:exec hello2` prints `hello from a loaded process` and exits 42; a
second `:install hello2` replaces it in place and it still runs; a
missing file is `NO SUCH FILE`, a name too long is refused by the rule
that names it, and `zz` written to the region is `the file is not hex
text`, after which `:exec junk` is not started. The full regression
passes; the kernel stays inside its slot.

## Not claimed

Nothing is signed: the program region has a CRC and the install writes
what the file held, with the operator's authority. There is no
`:uninstall` and no compaction in the OS; the table holds fifteen names.
No Agel word installs: a program writes the file and the operator installs
it. The backend that would write such a file from the IR is not here yet.
