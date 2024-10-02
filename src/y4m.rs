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

fn read_decimal<R: Read>(r: &mut R) -> usize {
  let mut v = 0;
  loop {
    let byte = r.read_u8().unwrap();
    match byte {
      b'0' ..= b'9' => {
        v = 10*v + (byte - b'0') as usize;
      },
      _ => {
        // Non-digit, stop parsing
        return v;
      }
    }
  }
}

// Read next character, expecting it to be whitespace
// Returns the character if it's whitespace, panics if not
// TODO: Return a Result type
fn expect_whitespace<R: Read>(r: &mut R) -> u8 {
  let byte = r.read_u8().unwrap();
  match byte {
    b' ' | b'\t' | b'\n' => {
      return byte;
    },
    _ => {
      panic!("Unexpected byte {} in Y4M file", byte);
    }
  }
}

// Skip forward until we find a whitespace character
// Returns the first whitespace character found
fn find_whitespace<R: Read>(r: &mut R) -> u8 {
  loop {
    let byte = r.read_u8().unwrap();
    match byte {
      b' ' | b'\t' | b'\n' => {
        return byte;
      },
      _ => {
        continue;
      }
    }
  }
}

impl<R: Read> Y4MReader<R> {
  pub fn new(mut inner: R) -> Self {
    // Read header line
    let mut file_magic = [0u8; 10];
    inner.read_exact(&mut file_magic).unwrap();
    if file_magic != Y4M_FILE_MAGIC.as_bytes() {
      panic!("Invalid file header");
    }

    let mut width = 0;
    let mut height = 0;

    // Parse parameter line
    // TODO: Handle params other than width/height
    loop {
      match inner.read_u8().unwrap() {
        b'\n' => {
          // End of parameter line
          break;
        },
        b' ' | b'\t' => {
          // Skip whitespace
          continue;
        },
        b'W' => {
          width = read_decimal(&mut inner);
          if expect_whitespace(&mut inner) == b'\n' {
            break;
          }
        },
        b'H' => {
          height = read_decimal(&mut inner);
          if expect_whitespace(&mut inner) == b'\n' {
            break;
          }
        },
        _ => {
          // Other parameters that we aren't parsing yet
          // Just skip until we find whitespace
          if find_whitespace(&mut inner) == b'\n' {
            break;
          }
        }
      }
    }

    if width == 0 || height == 0 {
      // Didn't find a width/height parameter, or it was zero
      panic!("Invalid Y4M size {}x{}", width, height);
    }

    Y4MReader {
      inner: inner,
      width: width,
      height: height
    }
  }

  pub fn read_frame(&mut self) -> Box<Frame> {
    // Read frame line
    // Technically this can have parameters, but they aren't useful to us.
    // So just check the magic number to ensure we're in the right place
    // and skip the rest of the line
    let mut frame_magic = [0u8; 5];
    self.inner.read_exact(&mut frame_magic).unwrap();
    if frame_magic != Y4M_FRAME_MAGIC.as_bytes() {
      panic!("Invalid frame header");
    }
  
    while self.inner.read_u8().unwrap() != b'\n' {}
  
    // Read actual frame data
    // TODO: Allow for non-contiguous rows
    let mut frame = Frame::new(self.width, self.height);
    self.inner.read_exact(&mut frame.y_mut().data).unwrap();
    self.inner.read_exact(&mut frame.u_mut().data).unwrap();
    self.inner.read_exact(&mut frame.v_mut().data).unwrap();

    return Box::new(frame);
  }
}

impl<W: Write> Y4MWriter<W> {
  pub fn new(mut inner: W, width: usize, height: usize) -> Self {
    inner.write_all(Y4M_FILE_MAGIC.as_bytes()).unwrap();
    write!(inner, "W{} H{}\n", width, height).unwrap();

    Y4MWriter {
      inner: inner,
      width: width,
      height: height
    }
  }

  pub fn write_frame(&mut self, frame: &Frame) {
    assert!(frame.y().width == self.width);
    assert!(frame.y().height == self.height);

    self.inner.write_all(Y4M_FRAME_MAGIC.as_bytes()).unwrap();
    self.inner.write_u8(b'\n').unwrap();

    // TODO: Allow for non-contiguous rows
    self.inner.write_all(&frame.y().data).unwrap();
    self.inner.write_all(&frame.u().data).unwrap();
    self.inner.write_all(&frame.v().data).unwrap();
  }
}
