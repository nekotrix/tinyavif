use byteorder::{BigEndian, WriteBytesExt};

pub struct ISOBMFFWriter {
  data: Vec<u8>
}

// Struct representing an open box, that's still being written
// This holds a mutable reference to the underlying data, ensuring that
// the box must be closed (by dropping it) before any data which is
// supposed to be outside the box can be written.
// It also holds the index for the 4-byte length field for this box,
// which will be filled in when the box is closed.
pub struct ISOBMFFBox<'a> {
  w: &'a mut ISOBMFFWriter,
  size_pos: usize
}

impl ISOBMFFWriter {
  pub fn new() -> Self {
    Self {
      data: Vec::new()
    }
  }

  pub fn open_box<'a>(&'a mut self, typ: &[u8]) -> ISOBMFFBox<'a> {
    let size_pos = self.data.len();
    
    // Write box header: 4 byte big-endian size, followed by 4-byte type
    assert!(typ.len() == 4);
    self.data.write_u32::<BigEndian>(0).unwrap();
    self.data.extend_from_slice(typ);

    return ISOBMFFBox {
      w: self,
      size_pos: size_pos
    };
  }

  pub fn open_box_with_version<'a>(&'a mut self, typ: &[u8], version: u8, flags: u32) -> ISOBMFFBox<'a> {
    let size_pos = self.data.len();

    // Write box header: 4 byte big-endian size, 4-byte type, 1-byte version,
    // 3-byte big-endian flags
    assert!(typ.len() == 4);
    assert!(flags < (1 << 24));
    self.data.write_u32::<BigEndian>(0).unwrap();
    self.data.extend_from_slice(typ);
    self.data.write_u32::<BigEndian>(((version as u32) << 24) | flags).unwrap();

    return ISOBMFFBox {
      w: self,
      size_pos: size_pos
    };
  }

  pub fn get_file_pos(&self) -> usize {
    self.data.len()
  }

  pub fn write_u32_at_marker(&mut self, pos: usize, value: u32) {
    assert!(self.data.len() >= pos + 4);
    self.data[pos]     = ((value >> 24) & 0xFF) as u8;
    self.data[pos + 1] = ((value >> 16) & 0xFF) as u8;
    self.data[pos + 2] = ((value >> 8)  & 0xFF) as u8;
    self.data[pos + 3] = (value         & 0xFF) as u8;
  }

  pub fn finalize(self) -> Box<[u8]> {
    return self.data.into_boxed_slice();
  }
}

impl<'a> ISOBMFFBox<'a> {
  pub fn open_box<'b>(&'b mut self, typ: &[u8]) -> ISOBMFFBox<'b> {
    let size_pos = self.w.data.len();
    
    // Write box header: 4 byte big-endian size, followed by 4-byte type
    assert!(typ.len() == 4);
    self.w.data.write_u32::<BigEndian>(0).unwrap();
    self.w.data.extend_from_slice(typ);

    return ISOBMFFBox {
      w: self.w,
      size_pos: size_pos
    };
  }

  pub fn open_box_with_version<'b>(&'b mut self, typ: &[u8], version: u8, flags: u32) -> ISOBMFFBox<'b> {
    let size_pos = self.w.data.len();

    // Write box header: 4 byte big-endian size, 4-byte type, 1-byte version,
    // 3-byte big-endian flags
    assert!(typ.len() == 4);
    assert!(flags < (1 << 24));
    self.w.data.write_u32::<BigEndian>(0).unwrap();
    self.w.data.extend_from_slice(typ);
    self.w.data.write_u32::<BigEndian>(((version as u32) << 24) | flags).unwrap();

    return ISOBMFFBox {
      w: self.w,
      size_pos: size_pos
    };
  }

  pub fn get_file_pos(&self) -> usize {
    self.w.data.len()
  }

  pub fn mark_u32(&mut self) -> usize {
    let marker = self.w.data.len();
    self.write_u32(0);
    return marker;
  }

  pub fn write_u8(&mut self, value: u8) {
    self.w.data.write_u8(value).unwrap();
  }

  pub fn write_u16(&mut self, value: u16) {
    self.w.data.write_u16::<BigEndian>(value).unwrap();
  }

  pub fn write_u32(&mut self, value: u32) {
    self.w.data.write_u32::<BigEndian>(value).unwrap();
  }

  pub fn write_bytes(&mut self, value: &[u8]) {
    self.w.data.extend_from_slice(value);
  }
}

impl<'a> Drop for ISOBMFFBox<'a> {
  fn drop(&mut self) {
    // Finalize size field
    // Note that for ISOBMFF boxes, the size written includes the box header,
    // i.e. it includes the size field itself, the type, and the version and flags if present
    let cur_pos = self.w.data.len();
    let total_size = cur_pos - self.size_pos;
    self.w.data[self.size_pos] = ((total_size >> 24) & 0xFF) as u8;
    self.w.data[self.size_pos + 1] = ((total_size >> 16) & 0xFF) as u8;
    self.w.data[self.size_pos + 2] = ((total_size >> 8) & 0xFF) as u8;
    self.w.data[self.size_pos + 3] = (total_size & 0xFF) as u8;
  }
}
