use std::io::prelude::*;

use byteorder::WriteBytesExt;

// Write 0-8 bytes from a u64 value in big-endian order
pub fn write_be_bytes<W: Write>(w: &mut W, value: u64, nbytes: usize) {
  assert!(nbytes <= 8);
  assert!(nbytes == 8 || (value >> (8*nbytes)) == 0);

  for i in (0..nbytes).rev() {
    let byte = (value >> (8 * i)) & 0xFF;
    w.write_u8(byte as u8).unwrap();
  }
}

// Write a value in AV1's LEB128 format
// In this format, each byte provides 7 bits of the value,
// along with a flag bit which indicates whether there are more bytes to read
// Also, in contrast to everything else here, this value is little-endian
pub fn write_leb128<W: Write>(w: &mut W, mut value: usize) {
  if value == 0 {
    w.write_u8(0).unwrap();
    return;
  }

  while value != 0 {
    let more_flag = if (value >> 7) > 0 { 0x80 } else { 0x00 };
    w.write_u8(more_flag | (value & 0x7F) as u8).unwrap();
    value >>= 7;
  }
}
