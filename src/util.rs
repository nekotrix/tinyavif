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

// Expose min/max as binary functions, rather than as methods
pub fn min<T: Ord>(a: T, b: T) -> T {
  a.min(b)
}

pub fn max<T: Ord>(a: T, b: T) -> T {
  a.max(b)
}

// Wrapper traits for signed/unsigned integer types
// The methods on these traits aren't intended to be used directly, but instead
// they exist to allow "normal function" versions to be implemented below
// eg. instead of value.abs() you can use abs(value)
pub trait Int {
  // Divide by 2^n with rounding to nearest, halves toward +infinity
  fn round2(self, n: u32) -> Self;
}
pub trait SignedInt {
  type Unsigned;
  fn abs_(self) -> Self;
  fn unsigned_abs_(self) -> Self::Unsigned;
  fn signum_(self) -> Self;

  // Divide by 2^n with rounding to nearest, halves away from zero
  fn round2_signed(self, n: u32) -> Self;
}
pub trait UnsignedInt: Int {
  // Floor and ceiling of log2(self)
  // Both these functions panic if `self == 0`
  fn floor_log2(self) -> u32;
  fn ceil_log2(self) -> u32;
}

impl Int for i8 {
  fn round2(self, n: u32) -> Self {
    let offset = (1 << n) >> 1;
    (self + offset) >> n
  }
}
impl Int for u8 {
  fn round2(self, n: u32) -> Self {
    let offset = (1 << n) >> 1;
    (self + offset) >> n
  }
}
impl Int for i16 {
  fn round2(self, n: u32) -> Self {
    let offset = (1 << n) >> 1;
    (self + offset) >> n
  }
}
impl Int for u16 {
  fn round2(self, n: u32) -> Self {
    let offset = (1 << n) >> 1;
    (self + offset) >> n
  }
}
impl Int for i32 {
  fn round2(self, n: u32) -> Self {
    let offset = (1 << n) >> 1;
    (self + offset) >> n
  }
}
impl Int for u32 {
  fn round2(self, n: u32) -> Self {
    let offset = (1 << n) >> 1;
    (self + offset) >> n
  }
}
impl Int for i64 {
  fn round2(self, n: u32) -> Self {
    let offset = (1 << n) >> 1;
    (self + offset) >> n
  }
}
impl Int for u64 {
  fn round2(self, n: u32) -> Self {
    let offset = (1 << n) >> 1;
    (self + offset) >> n
  }
}
impl Int for isize {
  fn round2(self, n: u32) -> Self {
    let offset = (1 << n) >> 1;
    (self + offset) >> n
  }
}
impl Int for usize {
  fn round2(self, n: u32) -> Self {
    let offset = (1 << n) >> 1;
    (self + offset) >> n
  }
}

impl SignedInt for i8 {
  type Unsigned = u8;
  fn abs_(self) -> Self { self.abs() }
  fn unsigned_abs_(self) -> Self::Unsigned { self.unsigned_abs() }
  fn signum_(self) -> Self { self.signum() }
  fn round2_signed(self, n: u32) -> Self {
    if self < 0 { -round2(-self, n) } else { round2(self, n) }
  }
}
impl SignedInt for i16 {
  type Unsigned = u16;
  fn abs_(self) -> Self { self.abs() }
  fn unsigned_abs_(self) -> Self::Unsigned { self.unsigned_abs() }
  fn signum_(self) -> Self { self.signum() }
  fn round2_signed(self, n: u32) -> Self {
    if self < 0 { -round2(-self, n) } else { round2(self, n) }
  }
}
impl SignedInt for i32 {
  type Unsigned = u32;
  fn abs_(self) -> Self { self.abs() }
  fn unsigned_abs_(self) -> Self::Unsigned { self.unsigned_abs() }
  fn signum_(self) -> Self { self.signum() }
  fn round2_signed(self, n: u32) -> Self {
    if self < 0 { -round2(-self, n) } else { round2(self, n) }
  }
}
impl SignedInt for i64 {
  type Unsigned = u64;
  fn abs_(self) -> Self { self.abs() }
  fn unsigned_abs_(self) -> Self::Unsigned { self.unsigned_abs() }
  fn signum_(self) -> Self { self.signum() }
  fn round2_signed(self, n: u32) -> Self {
    if self < 0 { -round2(-self, n) } else { round2(self, n) }
  }
}
impl SignedInt for isize {
  type Unsigned = usize;
  fn abs_(self) -> Self { self.abs() }
  fn unsigned_abs_(self) -> Self::Unsigned { self.unsigned_abs() }
  fn signum_(self) -> Self { self.signum() }
  fn round2_signed(self, n: u32) -> Self {
    if self < 0 { -round2(-self, n) } else { round2(self, n) }
  }
}

impl UnsignedInt for u8 {
  fn floor_log2(self) -> u32 {
    self.ilog2()
  }

  fn ceil_log2(self) -> u32 {
    if self == 0 {
      panic!("ceil_log2: Cannot take log2(0)");
    } else if self == 1 {
      return 0;
    } else {
      return (self - 1).ilog2() + 1;
    }
  }
}
impl UnsignedInt for u16 {
  fn floor_log2(self) -> u32 {
    self.ilog2()
  }

  fn ceil_log2(self) -> u32 {
    if self == 0 {
      panic!("ceil_log2: Cannot take log2(0)");
    } else if self == 1 {
      return 0;
    } else {
      return (self - 1).ilog2() + 1;
    }
  }
}
impl UnsignedInt for u32 {
  fn floor_log2(self) -> u32 {
    self.ilog2()
  }

  fn ceil_log2(self) -> u32 {
    if self == 0 {
      panic!("ceil_log2: Cannot take log2(0)");
    } else if self == 1 {
      return 0;
    } else {
      return (self - 1).ilog2() + 1;
    }
  }
}
impl UnsignedInt for u64 {
  fn floor_log2(self) -> u32 {
    self.ilog2()
  }

  fn ceil_log2(self) -> u32 {
    if self == 0 {
      panic!("ceil_log2: Cannot take log2(0)");
    } else if self == 1 {
      return 0;
    } else {
      return (self - 1).ilog2() + 1;
    }
  }
}
impl UnsignedInt for usize {
  fn floor_log2(self) -> u32 {
    self.ilog2()
  }

  fn ceil_log2(self) -> u32 {
    if self == 0 {
      panic!("ceil_log2: Cannot take log2(0)");
    } else if self == 1 {
      return 0;
    } else {
      return (self - 1).ilog2() + 1;
    }
  }
}

pub fn round2<T: Int>(value: T, n: u32) -> T {
  value.round2(n)
}

pub fn abs<T: SignedInt>(value: T) -> T {
  value.abs_()
}

pub fn unsigned_abs<T: SignedInt>(value: T) -> T::Unsigned {
  value.unsigned_abs_()
}

pub fn signum<T: SignedInt>(value: T) -> T {
  value.signum_()
}

pub fn round2_signed<T: SignedInt>(value: T, n: u32) -> T {
  value.round2_signed(n)
}

pub fn floor_log2<T: UnsignedInt>(value: T) -> u32 {
  value.floor_log2()
}

pub fn ceil_log2<T: UnsignedInt>(value: T) -> u32 {
  value.ceil_log2()
}

// Extract the probability of a single symbol from a CDF
pub fn get_prob(symbol: usize, cdf: &[u16]) -> u16 {
  if symbol == 0 {
    cdf[0]
  } else if symbol == cdf.len() {
    // Account for the implicit extra element 32768 on the end of the CDF
    32768 - cdf[symbol - 1]
  } else {
    cdf[symbol] - cdf[symbol - 1]
  }
}
