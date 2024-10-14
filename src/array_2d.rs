use bytemuck::Zeroable;
use bytemuck::allocation::zeroed_slice_box;

use std::ops::{Index, IndexMut};

// Two-dimensional array type
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
        let index = row * self.stride + col;
        self.data[index] = value.clone();
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
}

// Allow indexing by array[row][col]
// This is done by having array[row] return a normal slice which
// references the entire row in question. Then a normal slice index
// can pick out the desired element
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
