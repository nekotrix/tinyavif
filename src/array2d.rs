use bytemuck::Zeroable;
use bytemuck::allocation::zeroed_slice_box;

use std::ops::{Index, IndexMut};

// Two-dimensional array type
#[derive(Clone, Debug)]
pub struct Array2D<T> {
  rows: usize,
  cols: usize,
  stride: usize,
  data: Box<[T]>, // TODO: Use a raw pointer to reduce overhead?
}

impl<T> Array2D<T> {
  pub fn rows(&self) -> usize {
    self.rows
  }

  pub fn cols(&self) -> usize {
    self.cols
  }
}

impl<T> Array2D<T> {
  pub fn fill_with<F: FnMut(usize, usize) -> T>(&mut self, mut f: F) {
    for i in 0..self.rows {
      for j in 0..self.cols {
        self[i][j] = f(i, j);
      }
    }
  }
}

impl<T: Clone> Array2D<T> {
  // Fill a region of a given size with (cloned) copies of `value`
  pub fn fill_region(&mut self, row_start: usize, col_start: usize, rows: usize, cols: usize, value: &T) {
    let row_end = row_start.checked_add(rows).unwrap();
    let col_end = col_start.checked_add(cols).unwrap();

    if row_end > self.rows {
      panic!("Array2D row indices out of bounds (index {}..{} vs. size {})", row_start, row_end, self.rows);
    }
    if col_end > self.cols {
      panic!("Array2D column indices out of bounds (index {}..{} vs. size {})", col_start, col_end, self.cols);
    }

    for row in row_start .. row_end {
      for col in col_start .. col_end {
        // Due to the above checks, this calculation should never overflow
        self[row][col] = value.clone();
      }
    }
  }
}

impl<T: Zeroable> Array2D<T> {
  pub fn zeroed(rows: usize, cols: usize) -> Self {
    let stride = cols;
    let num_elements = rows.checked_mul(stride).unwrap();
    let data = zeroed_slice_box(num_elements);

    Self {
      rows: rows,
      cols: cols,
      stride: stride,
      data: data
    }
  }

  // TODO: Figure out how to make this not require Zeroable
  pub fn new_with<F: FnMut(usize, usize) -> T>(rows: usize, cols: usize, f: F) -> Self {
    let mut result = Array2D::zeroed(rows, cols);
    result.fill_with(f);
    return result;
  }
}

impl<T: Zeroable + Copy> Array2D<T> {
  pub fn transpose_into(&self, dst: &mut Self) {
    assert!(self.rows == dst.cols);
    assert!(self.cols == dst.rows);
    for i in 0..self.cols {
      for j in 0..self.rows {
        dst[i][j] = self[j][i];
      }
    }
  }

  pub fn transpose(&self) -> Self {
    let mut dst = Array2D::zeroed(self.cols, self.rows);
    self.transpose_into(&mut dst);
    return dst;
  }

  pub fn map<F: FnMut(usize, usize, T) -> T>(&mut self, mut f: F) {
    for i in 0..self.rows {
      for j in 0..self.cols {
        self[i][j] = f(i, j, self[i][j]);
      }
    }
  }
}

// Allow indexing by array[row][col]
// This is done by having array[row] return a normal slice which
// references the entire row in question. Then a normal slice index
// can pick out the desired element
// TODO: Change this to index with (usize, usize) directly,
// and then extend to allow ranges in both arguments, returning a new Slice2D type
impl<T> Index<usize> for Array2D<T> {
  type Output = [T];
  fn index(&self, index: usize) -> &[T] {
    if index >= self.rows {
      panic!("Array2D row index out of bounds (index {} vs. size {})", index, self.rows);
    }
    // Due to the above check, these calculations should never overflow
    let start_index = index * self.stride;
    let end_index = start_index + self.cols;
    &self.data[start_index .. end_index]
  }
}

impl<T> IndexMut<usize> for Array2D<T> {
  fn index_mut(&mut self, index: usize) -> &mut [T] {
    if index >= self.rows {
      panic!("Array2D row index out of bounds (index {} vs. size {})", index, self.rows);
    }
    // Due to the above check, these calculations should never overflow
    let start_index = index * self.stride;
    let end_index = start_index + self.cols;
    &mut self.data[start_index .. end_index]
  }
}
