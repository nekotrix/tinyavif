use std::io;
use std::io::prelude::*;

use byteorder::{ReadBytesExt, WriteBytesExt};

use crate::frame::Frame;

const Y4M_FILE_MAGIC: &str = "YUV4MPEG2 ";
const Y4M_FRAME_MAGIC: &str = "FRAME";

pub struct Y4MReader<R> {
  inner: R,
  width: usize,
  height: usize
}

pub struct Y4MWriter<W> {
  inner: W,
  width: usize,
  height: usize
}

fn read_decimal<R: Read>(r: &mut R) -> Result<(usize, u8), io::Error> {
  let mut v = 0;
  loop {
    let byte = r.read_u8()?;
    match byte {
      b'0' ..= b'9' => {
        v = 10*v + (byte - b'0') as usize;
      },
      _ => {
        // Non-digit, stop parsing
        // Return value and the character that wasn't part of this value
        return Ok((v, byte));
      }
    }
  }
}

// Read next character, expecting it to be whitespace
// Returns the character if it's whitespace, panics if not
fn expect_whitespace<R: Read>(r: &mut R) -> Result<u8, io::Error> {
  let byte = r.read_u8()?;
  match byte {
    b' ' | b'\t' | b'\n' => {
      return Ok(byte);
    },
    _ => {
      panic!("Unexpected byte {} in Y4M file", byte);
    }
  }
}

// Skip forward until we find a whitespace character
// Returns the first whitespace character found
fn find_whitespace<R: Read>(r: &mut R) -> Result<u8, io::Error> {
  loop {
    let byte = r.read_u8()?;
    match byte {
      b' ' | b'\t' | b'\n' => {
        return Ok(byte);
      },
      _ => {
        continue;
      }
    }
  }
}

impl<R: Read> Y4MReader<R> {
  pub fn new(mut inner: R) -> Result<Self, io::Error> {
    // Read header line
    let mut file_magic = [0u8; 10];
    inner.read_exact(&mut file_magic)?;
    if file_magic != Y4M_FILE_MAGIC.as_bytes() {
      panic!("Invalid file header");
    }

    let mut width = 0;
    let mut height = 0;

    // Parse parameter line
    loop {
      match inner.read_u8()? {
        b'\n' => {
          // End of parameter line
          break;
        },
        b' ' | b'\t' => {
          // Skip whitespace
          continue;
        },
        b'W' => {
          let byte;
          (width, byte) = read_decimal(&mut inner)?;
          match byte {
            b'\n' => { break; },
            b' ' | b'\t' | b'\r' => { continue; }
            _ => { panic!("Unexpected byte {} in Y4M file", byte); }
          }
        },
        b'H' => {
          let byte;
          (height, byte) = read_decimal(&mut inner)?;
          match byte {
            b'\n' => { break; },
            b' ' | b'\t' | b'\r' => { continue; }
            _ => { panic!("Unexpected byte {} in Y4M file", byte); }
          }
        },
        _ => {
          // Other parameters that we aren't parsing yet
          // Just skip until we find whitespace
          if find_whitespace(&mut inner)? == b'\n' {
            break;
          }
        }
      }
    }

    if width == 0 || height == 0 {
      // Didn't find a width/height parameter, or it was zero
      panic!("Invalid Y4M size {}x{}", width, height);
    }

    Ok(Y4MReader {
      inner: inner,
      width: width,
      height: height
    })
  }

  pub fn read_frame(&mut self) -> Result<Box<Frame>, io::Error> {
    // Read frame line
    // Technically this can have parameters, but they aren't useful to us.
    // So just check the magic number to ensure we're in the right place
    // and skip the rest of the line
    let mut frame_magic = [0u8; 5];
    self.inner.read_exact(&mut frame_magic)?;
    if frame_magic != Y4M_FRAME_MAGIC.as_bytes() {
      panic!("Invalid frame header");
    }
  
    while self.inner.read_u8()? != b'\n' {}
  
    // Read actual frame data
    let mut frame = Frame::new(self.height, self.width);
    frame.y_mut().read_from(&mut self.inner)?;
    frame.u_mut().read_from(&mut self.inner)?;
    frame.v_mut().read_from(&mut self.inner)?;

    Ok(Box::new(frame))
  }
}

impl<W: Write> Y4MWriter<W> {
  pub fn new(mut inner: W, width: usize, height: usize) -> Result<Self, io::Error> {
    inner.write_all(Y4M_FILE_MAGIC.as_bytes())?;
    write!(inner, "W{} H{}\n", width, height)?;

    Ok(Y4MWriter {
      inner: inner,
      width: width,
      height: height
    })
  }

  pub fn write_frame(&mut self, frame: &Frame) -> Result<(), io::Error> {
    assert!(frame.y().width() == self.width);
    assert!(frame.y().height() == self.height);

    self.inner.write_all(Y4M_FRAME_MAGIC.as_bytes())?;
    self.inner.write_u8(b'\n')?;
    frame.y().write_to(&mut self.inner)?;
    frame.u().write_to(&mut self.inner)?;
    frame.v().write_to(&mut self.inner)?;

    Ok(())
  }
}
