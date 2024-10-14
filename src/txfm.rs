// Forward and inverse DCT4 and DCT8 transforms

use crate::array2d::Array2D;
use crate::consts::*;
use crate::util::*;

fn cospi_arr(cos_bit: u32) -> &'static [i32; 64] {
  assert!(10 <= cos_bit && cos_bit <= 13);
  &av1_cospi_arr_data[(cos_bit - 10) as usize]
}

fn clamp_value(value: i32, range_bits: u32) -> i32 {
  assert!(0 < range_bits);
  assert!(range_bits <= 32);
  
  // When range_bits == 32, the intermediate value 1 << (range_bits - 1) doesn't fit into
  // an i32, so we need to use an i64. But the final values of min_ and max_ *will*
  // fit into an i32.
  let min_ = -(1i64 << (range_bits - 1));
  let max_ = (1i64 << (range_bits - 1)) - 1;
  clamp(value, min_ as i32, max_ as i32)
}

fn clamp_array(arr: &mut [i32], bits: u32) {
  for i in 0 .. arr.len() {
    arr[i] = clamp_value(arr[i], bits);
  }
}

// Divide elements of an array by 2^bits, with rounding
// bits is allowed to be negative, in which case the values are scaled up
fn round_shift_array(arr: &mut [i32], bits: i32) {
  if bits == 0 {
    return;
  } else if bits < 0 {
    let shift = (-bits) as u32;
    for i in 0 .. arr.len() {
      let tmp = (arr[i] as i64) << shift;
      arr[i] = clamp(tmp, i32::MIN as i64, i32::MAX as i64) as i32;
    }
  } else {
    let shift = bits as u32;
    for i in 0 .. arr.len() {
      arr[i] = round2(arr[i], shift);
    }
  }
}

// Calculate round2(w0 * in0 + w1 * in1, cos_bit)
// In theory this "should" require the intermediates to be converted to `i64`s,
// but per a helpful comment from a certain someone ( ;) ) in libaom, we can
// in fact use wrapping 32-bit arithmetic throughout. We just have to explicitly
// inline the round2() operation so that that can use wrapping too.
fn half_btf(w0: i32, in0: i32, w1: i32, in1: i32, cos_bit: u32) -> i32 {
  let tmp = (w0 * in0).wrapping_add(w1 * in1);
  let offset = 1 << (cos_bit - 1);
  return (tmp.wrapping_add(offset)) >> cos_bit;
}

// In-place 8-point forward DCT
fn fwd_dct8(arr: &mut [i32], cos_bit: u32, _stage_range: &[u32]) {
  assert!(arr.len() == 8);

  let cospi = cospi_arr(cos_bit);

  // TODO: Range checks

  let stage1 = [
    arr[0] + arr[7],
    arr[1] + arr[6],
    arr[2] + arr[5],
    arr[3] + arr[4],
    -arr[4] + arr[3],
    -arr[5] + arr[2],
    -arr[6] + arr[1],
    -arr[7] + arr[0],
  ];

  let stage2 = [
    stage1[0] + stage1[3],
    stage1[1] + stage1[2],
    -stage1[2] + stage1[1],
    -stage1[3] + stage1[0],
    stage1[4],
    half_btf(-cospi[32], stage1[5], cospi[32], stage1[6], cos_bit),
    half_btf(cospi[32], stage1[6], cospi[32], stage1[5], cos_bit),
    stage1[7],
  ];

  let stage3 = [
    half_btf(cospi[32], stage2[0], cospi[32], stage2[1], cos_bit),
    half_btf(-cospi[32], stage2[1], cospi[32], stage2[0], cos_bit),
    half_btf(cospi[48], stage2[2], cospi[16], stage2[3], cos_bit),
    half_btf(cospi[48], stage2[3], -cospi[16], stage2[2], cos_bit),
    stage2[4] + stage2[5],
    -stage2[5] + stage2[4],
    -stage2[6] + stage2[7],
    stage2[7] + stage2[6],
  ];

  let stage4 = [
    stage3[0],
    stage3[1],
    stage3[2],
    stage3[3],
    half_btf(cospi[56], stage3[4], cospi[8], stage3[7], cos_bit),
    half_btf(cospi[24], stage3[5], cospi[40], stage3[6], cos_bit),
    half_btf(cospi[24], stage3[6], -cospi[40], stage3[5], cos_bit),
    half_btf(cospi[56], stage3[7], -cospi[8], stage3[4], cos_bit),
  ];

  let stage5 = [
    stage4[0],
    stage4[4],
    stage4[2],
    stage4[6],
    stage4[1],
    stage4[5],
    stage4[3],
    stage4[7],
  ];

  arr.copy_from_slice(&stage5);
}

// In-place 8-point inverse DCT
fn inv_dct8(arr: &mut [i32], cos_bit: u32, stage_range: &[u32]) {
  assert!(arr.len() == 8);

  let cospi = cospi_arr(cos_bit);
  // TODO: Range checks

  let stage1 = [
    arr[0],
    arr[4],
    arr[2],
    arr[6],
    arr[1],
    arr[5],
    arr[3],
    arr[7],
  ];

  let stage2 = [
    stage1[0],
    stage1[1],
    stage1[2],
    stage1[3],
    half_btf(cospi[56], stage1[4], -cospi[8], stage1[7], cos_bit),
    half_btf(cospi[24], stage1[5], -cospi[40], stage1[6], cos_bit),
    half_btf(cospi[40], stage1[5], cospi[24], stage1[6], cos_bit),
    half_btf(cospi[8], stage1[4], cospi[56], stage1[7], cos_bit)
  ];

  let stage3 = [
    half_btf(cospi[32], stage2[0], cospi[32], stage2[1], cos_bit),
    half_btf(cospi[32], stage2[0], -cospi[32], stage2[1], cos_bit),
    half_btf(cospi[48], stage2[2], -cospi[16], stage2[3], cos_bit),
    half_btf(cospi[16], stage2[2], cospi[48], stage2[3], cos_bit),
    clamp_value(stage2[4] + stage2[5], stage_range[3]),
    clamp_value(stage2[4] - stage2[5], stage_range[3]),
    clamp_value(-stage2[6] + stage2[7], stage_range[3]),
    clamp_value(stage2[6] + stage2[7], stage_range[3]),
  ];
  
  let stage4 = [
    clamp_value(stage3[0] + stage3[3], stage_range[4]),
    clamp_value(stage3[1] + stage3[2], stage_range[4]),
    clamp_value(stage3[1] - stage3[2], stage_range[4]),
    clamp_value(stage3[0] - stage3[3], stage_range[4]),
    stage3[4],
    half_btf(-cospi[32], stage3[5], cospi[32], stage3[6], cos_bit),
    half_btf(cospi[32], stage3[5], cospi[32], stage3[6], cos_bit),
    stage3[7],
  ];

  let stage5 = [
    clamp_value(stage4[0] + stage4[7], stage_range[5]),
    clamp_value(stage4[1] + stage4[6], stage_range[5]),
    clamp_value(stage4[2] + stage4[5], stage_range[5]),
    clamp_value(stage4[3] + stage4[4], stage_range[5]),
    clamp_value(stage4[3] - stage4[4], stage_range[5]),
    clamp_value(stage4[2] - stage4[5], stage_range[5]),
    clamp_value(stage4[1] - stage4[6], stage_range[5]),
    clamp_value(stage4[0] - stage4[7], stage_range[5]),
  ];

  arr.copy_from_slice(&stage5);
}

// Perform a 2D forward transform composed of two 1D transforms
// R = row transform (applied first)
// C = col transform (applied second)
pub fn fwd_txfm2d(residual: &mut Array2D<i32>, txh: usize, txw: usize) {
  assert!(residual.rows() == txh);
  assert!(residual.cols() == txw);

  let txsz_idx;
  let fwd_txfm;
  if txh == 8 && txw == 8 {
    txsz_idx = 1;
    fwd_txfm = &fwd_dct8;
  } else if txh == 4 && txw == 4 {
    //txsz_idx = 0;
    //fwd_txfm = &fwd_dct4;
    todo!();
  } else {
    todo!();
  }

  let cos_bit_col = 13; // For both 4x4 and 8x8 forward transforms, less for some other sizes
  let cos_bit_row = 13; // For both 4x4 and 8x8 forward transforms, less for some other sizes

  let bd = 8;
  let stages = av1_txfm_stages[txsz_idx];
  let shift = &av1_txfm_fwd_shift[txsz_idx];
  let stage_ranges = &av1_txfm_fwd_range_mult2[txsz_idx];

  let mut stage_range_col = vec![0u32; stages];
  let mut stage_range_row = vec![0u32; stages];

  for i in 0..stages {
    stage_range_col[i] = (round2(stage_ranges[i], 1) + shift[0] + bd + 1) as u32;
  }
  for i in 0..stages {
    stage_range_row[i] = (round2(stage_ranges[stages - 1] + stage_ranges[i], 1) + shift[0] + shift[1] + bd + 1) as u32;
  }

  // Column transforms
  let mut transposed = residual.transpose();
  for j in 0..txw {
    let col = &mut transposed[j];
    round_shift_array(col, -shift[0]);
    fwd_txfm(col, cos_bit_col, &stage_range_col);
    round_shift_array(col, -shift[1]);
  }

  // Row transforms
  transposed.transpose_into(residual);
  for i in 0..txh {
    let row = &mut residual[i];
    fwd_txfm(row, cos_bit_row, &stage_range_row);
    round_shift_array(row, -shift[2]);
  }
}

// Perform a 2D forward transform composed of two 1D transforms
// R = row transform (applied first)
// C = col transform (applied second)
pub fn inv_txfm2d(residual: &mut Array2D<i32>, txh: usize, txw: usize) {
  assert!(residual.rows() == txh);
  assert!(residual.cols() == txw);

  let txsz_idx;
  let inv_txfm;
  if txh == 8 && txw == 8 {
    txsz_idx = 1;
    inv_txfm = &inv_dct8;
  } else if txh == 4 && txw == 4 {
    //txsz_idx = 0;
    //inv_txfm = &inv_dct4;
    todo!();
  } else {
    todo!();
  }

  let cos_bit_col = 12; // For all inverse transform sizes
  let cos_bit_row = 12; // For all inverse transform sizes

  let bd = 8;
  let opt_range_row = 16;
  let opt_range_col = 16;
  let stages = av1_txfm_stages[txsz_idx];
  let shift = &av1_txfm_inv_shift[txsz_idx];
  // TODO: I think this is just all zeros?
  //let stage_ranges = &av1_txfm_inv_range_mult2[txsz_idx];

  let mut stage_range_row = vec![0u32; stages];
  let mut stage_range_col = vec![0u32; stages];

  for i in 0..stages {
    stage_range_row[i] = (/*stage_ranges[i] + */ av1_txfm_inv_start_range[txsz_idx] + (bd as i32) + 1) as u32;
  }
  for i in 0..stages {
    stage_range_col[i] = (/*stage_ranges[i] + */ av1_txfm_inv_start_range[txsz_idx] + shift[0] + (bd as i32) + 1) as u32;
  }

  // Row transforms
  for i in 0..txh {
    let row = &mut residual[i];
    clamp_array(row, bd + 8);
    inv_txfm(row, cos_bit_col, &stage_range_col);
    round_shift_array(row, -shift[0]);
  }

  // Column transforms
  let mut transposed = residual.transpose();
  for j in 0..txw {
    let col = &mut transposed[j];
    clamp_array(col, max(bd + 6, 16));
    inv_txfm(col, cos_bit_row, &stage_range_row);
    round_shift_array(col, -shift[1]);
  }

  transposed.transpose_into(residual);
}
