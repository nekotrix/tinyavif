use std::io;
use std::io::prelude::*;

use crate::array2d::Array2D;
use crate::util::*;

pub struct Plane {
  // Pixel data
  // The width() / height() methods of this array give the padded size.
  // For the real size, use the .crop_width / .crop_height members below
  pixels: Array2D<u8>,

  crop_width: usize,
  crop_height: usize
}

impl Plane {
  pub fn pixels(&self) -> &Array2D<u8> {
    &self.pixels
  }

  pub fn pixels_mut(&mut self) -> &mut Array2D<u8> {
    &mut self.pixels
  }

  pub fn width(&self) -> usize {
    self.pixels.cols()
  }

  pub fn height(&self) -> usize {
    self.pixels.rows()
  }

  pub fn crop_width(&self) -> usize {
    self.crop_width
  }

  pub fn crop_height(&self) -> usize {
    self.crop_height
  }

  // Fill in the pixels outside the crop region, by copying the rightmost and
  // bottommost pixels from within the crop region
  // This *must* be called after any modification which may potentially affect
  // the last row/column of pixels, or which may disturb the padding region
  pub fn fill_padding(&mut self) {
    let crop_width = self.crop_width;
    let crop_height = self.crop_height;
    let width = self.width();
    let height = self.height();

    for row in 0..height {
      let rightmost_pixel = self.pixels[row][crop_width - 1];
      self.pixels[row][crop_width .. width].fill(rightmost_pixel);
    }

    // TODO: Check if this compiles down to a memcpy properly
    // If not, probably need to push this method down to some kind of copy_region()
    // method on Array2D, which can use slice::split_at_mut() to get properly
    // non-overlapping references to the last row and the padding region
    for row in crop_height .. height {
      for col in 0 .. width {
        self.pixels[row][col] = self.pixels[crop_height - 1][col];
      }
    }
  }

  pub fn read_from<R: Read>(&mut self, r: &mut R) -> Result<(), io::Error> {
    for row in 0 .. self.crop_height {
      r.read_exact(&mut self.pixels[row][0 .. self.crop_width])?;
    }
    self.fill_padding();
    Ok(())
  }

  pub fn write_to<W: Write>(&self, w: &mut W) -> Result<(), io::Error> {
    for row in 0 .. self.crop_height {
      w.write_all(&self.pixels[row][0 .. self.crop_width])?;
    }
    Ok(())
  }
}

pub struct Frame {
  planes: [Plane; 3]
}

impl Frame {
  pub fn new(y_crop_height: usize, y_crop_width: usize) -> Self {
    let y_width = y_crop_width.next_multiple_of(8);
    let y_height = y_crop_height.next_multiple_of(8);

    let uv_crop_width = round2(y_crop_width, 1);
    let uv_crop_height = round2(y_crop_height, 1);

    let uv_width = y_crop_width / 2;
    let uv_height = y_crop_height / 2;

    Self {
      planes: [
        Plane {
          pixels: Array2D::zeroed(y_height, y_width),
          crop_width: y_crop_width,
          crop_height: y_crop_height
        },
        Plane {
          pixels: Array2D::zeroed(uv_height, uv_width),
          crop_width: uv_crop_width,
          crop_height: uv_crop_height
        },
        Plane {
          pixels: Array2D::zeroed(uv_height, uv_width),
          crop_width: uv_crop_width,
          crop_height: uv_crop_height
        },
      ]
    }
  }

  pub fn plane(&self, idx: usize) -> &Plane {
    &self.planes[idx]
  }

  pub fn plane_mut(&mut self, idx: usize) -> &mut Plane {
    &mut self.planes[idx]
  }

  pub fn y(&self) -> &Plane {
    &self.planes[0]
  }

  pub fn y_mut(&mut self) -> &mut Plane {
    &mut self.planes[0]
  }

  pub fn u(&self) -> &Plane {
    &self.planes[1]
  }

  pub fn u_mut(&mut self) -> &mut Plane {
    &mut self.planes[1]
  }

  pub fn v(&self) -> &Plane {
    &self.planes[2]
  }

  pub fn v_mut(&mut self) -> &mut Plane {
    &mut self.planes[2]
  }
}
