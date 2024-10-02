// TODO: Align rows to a convenient byte alignment
// TODO: Add padding all around each plane
pub struct Plane {
  pub width: usize,
  pub height: usize,
  pub stride: usize,
  pub data: Box<[u8]>
}

impl Plane {
  pub fn new(width: usize, height: usize) -> Self {
    Self {
      width: width,
      height: height,
      stride: width,
      data: vec![128u8; width*height].into_boxed_slice()
    }
  }
}

pub struct Frame {
  planes: [Plane; 3]
}

impl Frame {
  pub fn new(y_width: usize, y_height: usize) -> Self {
    let uv_width = (y_width + 1)/2;
    let uv_height = (y_height + 1)/2;

    Self {
      planes: [
        Plane::new(y_width, y_height),
        Plane::new(uv_width, uv_height),
        Plane::new(uv_width, uv_height)
      ]
    }
  }

  pub fn plane(&self, idx: usize) -> &Plane {
    &self.planes[idx]
  }

  pub fn plane_mut(&mut self, idx: usize) -> &Plane {
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
