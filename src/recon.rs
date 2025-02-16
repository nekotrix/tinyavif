// Copyright (c) 2024-2025, The tinyavif contributors. All rights reserved
//
// This source code is subject to the terms of the BSD 2 Clause License and
// the Alliance for Open Media Patent License 1.0. If the BSD 2 Clause License
// was not distributed with this source code in the LICENSE file, you can
// obtain it at www.aomedia.org/license/software. If the Alliance for Open
// Media Patent License 1.0 was not distributed with this source code in the
// PATENTS file, you can obtain it at www.aomedia.org/license/patent.

// Reconstruction functions

use crate::array2d::Array2D;
use crate::consts::*;
use crate::txfm::*;
use crate::util::*;

// Predictions - only DC_PRED for now
pub fn dc_predict(pixels: &mut Array2D<u8>, y0: usize, x0: usize, h: usize, w: usize) {
  // For now, as we only ever use one tile, we can infer the haveLeft and haveAbove flags as:
  let haveLeft = x0 > 0;
  let haveAbove = y0 > 0;

  let mut sum = 0usize;
  if haveAbove {
    for j in 0..w {
      sum += pixels[y0 - 1][x0 + j] as usize;
    }
  }
  if haveLeft {
    for i in 0..h {
      sum += pixels[y0 + i][x0 - 1] as usize;
    }
  }

  let avg = if haveAbove && haveLeft {
    (sum + (w + h)/2) / (w + h)
  } else if haveAbove {
    (sum + w/2) / w
  } else if haveLeft {
    (sum + h/2) / h
  } else {
    128
  };

  let pred = clamp(avg, 0, 255) as u8;
  pixels.fill_region(y0, x0, h, w, &pred);
}

// Transform pipeline:
// 2d forward transform -> quantize -> dequantize -> 2d inverse transform
// The logic here implements the "big picture" stuff, for individual transforms
// see txfm.rs

// Calculate the residual (forward-transformed difference) between a given source image
// and the corresponding prediction
pub fn compute_residual(source: &Array2D<u8>, pred: &Array2D<u8>,
                    y0: usize, x0: usize, h: usize, w: usize) -> Array2D<i32> {
  let mut residual = Array2D::new_with(
    h, w,
    |i, j| (source[y0 + i][x0 + j] as i32) - (pred[y0 + i][x0 + j] as i32)
  );

  fwd_txfm2d(&mut residual, h, w);

  return residual;
}

// Quantize the coefficients in a given transform block
pub fn quantize(residual: &mut Array2D<i32>, qindex: u8) {
  let dc_q = qindex_to_dc_q[qindex as usize];
  let ac_q = qindex_to_ac_q[qindex as usize];

  residual.map(|i, j, coeff| {
    let q = if i == 0 && j == 0 { dc_q } else { ac_q };
    // Divide coeff by q, with rounding to nearest, halves toward 0
    // A smaller bias can even be used, essentially rounding values slightly
    // above half toward zero as well, to improve the average rate-distortion tradeoff -
    // see for example QuantizationContext in rav1e.
    // But here we take the simplest option.
    let abs = abs(coeff);
    let sign = signum(coeff);
    sign * ((abs + (q-1)/2) / q)
  });
}

pub fn dequantize(residual: &mut Array2D<i32>, qindex: u8) {
  let dc_q = qindex_to_dc_q[qindex as usize];
  let ac_q = qindex_to_ac_q[qindex as usize];

  residual.map(|i, j, coeff| {
    let q = if i == 0 && j == 0 { dc_q } else { ac_q };
    // Simply scale the quantized coefficient by the appropriate Q
    coeff * q
  });
}

// Apply a residual to a prediction (in recon) to generate a fully reconstructed block
// Note: This consumes the residual array, pass in a clone if you want to keep
// the original array intact
pub fn apply_residual(recon: &mut Array2D<u8>, mut residual: Array2D<i32>,
                  y0: usize, x0: usize, h: usize, w: usize) {
  inv_txfm2d(&mut residual, h, w);

  for i in 0..h {
    for j in 0..w {
      recon[y0 + i][x0 + j] = clamp((recon[y0 + i][x0 + j] as i32) + residual[i][j], 0, 255) as u8;
    }
  }
}
