// Copyright (c) 2024-2025, The tinyavif contributors. All rights reserved
//
// This source code is subject to the terms of the BSD 2 Clause License and
// the Alliance for Open Media Patent License 1.0. If the BSD 2 Clause License
// was not distributed with this source code in the LICENSE file, you can
// obtain it at www.aomedia.org/license/software. If the Alliance for Open
// Media Patent License 1.0 was not distributed with this source code in the
// PATENTS file, you can obtain it at www.aomedia.org/license/patent.

use bytemuck::Zeroable;
use std::io;
use std::fs::File;

use crate::array2d::Array2D;
use crate::bitcode::BitWriter;
use crate::cdf::*;
use crate::consts::*;
use crate::entropycode::EntropyWriter;
use crate::enums::*;
use crate::frame::Frame;
use crate::recon::*;
use crate::util::*;
use crate::y4m::*;

// Top-level encoder state
pub struct AV1Encoder {
  // Size used for encoding - always padded to a multiple of 8x8 luma pixels
  y_width: usize,
  y_height: usize,
  uv_width: usize,
  uv_height: usize,

  // Original image size
  y_crop_width: usize,
  y_crop_height: usize,
  uv_crop_width: usize,
  uv_crop_height: usize,
}

// "Mode info" unit - a struct representing the state of a single 4x4 luma pixel unit.
// The values in here can be used as contexts when encoding later blocks
#[derive(Zeroable, Clone)]
pub struct ModeInfo {
  // "Level context" for each plane
  // This is the sum of absolute values of the coefficients in each block,
  // capped at 63, and is used as part of the context for coefficient sizes
  //
  // Note: As we don't use transform partitioning, this is never actually
  // used for luma in this encoder. But it is required for chroma.
  level_ctx: [u8; 3],

  // Sign of the DC coefficient for each plane
  // This is stored differently to what the spec says: we store
  // -1 if the DC coefficient is negative, 0 if zero, 1 if positive.
  // This way, we can compare the number of nearby +ve and -ve DC coefficients by
  // simply summing this value over nearby blocks.
  dc_sign: [i8; 3],
}

// Mutable state used while encoding a single tile
pub struct TileEncoder<'a> {
  encoder: &'a AV1Encoder,
  bitstream: EntropyWriter,

  base_qindex: u8,

  // Mode info per 4x4 luma pixel unit
  mode_info: Array2D<ModeInfo>,

  // Source frame
  // This is the image we are trying to reproduce
  // This must be pre-padded to match encoder.y_{width/height}, not the crop size
  source: &'a Frame,

  // Reconstructed frame
  recon: Frame,
}

fn get_qctx(base_qindex: u8) -> usize {
  if base_qindex <= 20 {
    0
  } else if base_qindex <= 60 {
    1
  } else if base_qindex <= 120 {
    2
  } else {
    3
  }
}

impl AV1Encoder {
  pub fn new(y_crop_width: usize, y_crop_height: usize) -> Self {
    // Check limits imposed by AV1
    assert!(0 < y_crop_width && y_crop_width <= 65536);
    assert!(0 < y_crop_height && y_crop_height <= 65536);

    let y_width = y_crop_width.next_multiple_of(8);
    let y_height = y_crop_height.next_multiple_of(8);

    let uv_crop_width = round2(y_crop_width, 1);
    let uv_crop_height = round2(y_crop_height, 1);

    let uv_width = y_width / 2;
    let uv_height = y_height / 2;

    Self {
      y_width: y_width,
      y_height: y_height,
      uv_width: uv_width,
      uv_height: uv_height,
      y_crop_width: y_crop_width,
      y_crop_height: y_crop_height,
      uv_crop_width: uv_crop_width,
      uv_crop_height: uv_crop_height,
    }
  }

  pub fn generate_sequence_header(&self) -> Box<[u8]> {
    let mut w = BitWriter::new();
    
    w.write_bits(0, 3); // "Main" profile: 8 or 10 bits, YUV 4:2:0 or monochrome
    w.write_bit(1); // Still picture
    w.write_bit(1); // with simplified headers
  
    w.write_bits(31, 5); // Level = 31, a special value meaning no level-based constraints apply
  
    // Width and height - we first code how many bits to use for each value (here just use 16,
    // for simplicity), then one less than the actual width and height
    w.write_bits(15, 4);
    w.write_bits(15, 4);
    w.write_bits((self.y_crop_width-1) as u64, 16);
    w.write_bits((self.y_crop_height-1) as u64, 16);
  
    // Now to disable a bunch of features we aren't going to use
    // 6 zero bits means:
    // * 64x64 superblocks
    // * Disable filter-intra and intra-edge-filter
    // * Disable superres, CDEF, and loop restoration
    w.write_bits(0, 6);
  
    // Colour configuration
    w.write_bit(0); // 8 bits per pixel
    w.write_bit(0); // Not monochrome, ie. we have chroma
    w.write_bit(0); // No colour info for now - we can put it in the AVIF headers later
    w.write_bit(0); // "TV" colour range
    w.write_bits(0, 2); // Unknown chroma sample position
    w.write_bit(0); // UV channels have shared delta-q values
  
    w.write_bit(0); // No film grain
  
    // Sequence headers always appear in their own OBU, so always add a trailing 1 bit
    return w.finalize(true);
  }
  
  pub fn generate_frame_header(&self, base_qindex: u8, add_trailing_one_bit: bool) -> Box<[u8]> {
    let mut w = BitWriter::new();
    
    w.write_bit(1); // Disable CDF updates
    w.write_bit(0); // Disable screen content tools
    w.write_bit(0); // Render size = frame size
  
    // Tile info
    // We need to code a tiling mode, then two zero bits to select 1x1 tiling.
    // However, if the width or height is less than one superblock (ie, 64 pixels), the
    // corresponding flag is implicitly set to 0 and doesn't need to be signalled.
    // So we need to add these conditionally
    w.write_bit(1); // Uniform tile mode - allows the cheapest signaling of 1x1 tile layout
    if self.y_width > 64 {
      w.write_bit(0); // 1 tile column
    }
    if self.y_height > 64 {
      w.write_bit(0); // 1 tile row
    }
  
    w.write_bits(base_qindex as u64, 8);
  
    w.write_bits(0, 3); // No frame-level delta-qs (three bits: Y DC, UV DC, UV AC)
    w.write_bit(0); // Don't use quantizer matrices
    w.write_bit(0); // No segmentation
    w.write_bit(0); // No superblock-level delta-q (=> no superblock-level delta-lf)
  
    // Deblocking params
    w.write_bits(0, 6); // Strength 0 = 0
    w.write_bits(0, 6); // Strength 1 = 0
    w.write_bits(0, 3); // Sharpness = 0
    w.write_bit(0); // No per-ref delta-lf (present even though it's irrelevant for still images)
  
    // Transforms
    w.write_bit(0); // Always use largest possible TX size for each block
    w.write_bit(1); // Use reduced TX type selection
  
    // Frame header needs a trailing 1 bit if it's in a standalone FRAME_HEADER OBU, but *not*
    // if it's in an OBU_FRAME
    return w.finalize(add_trailing_one_bit);
  }

  pub fn encode_image(&self, source: &Frame, base_qindex: u8) -> Box<[u8]> {
    // Encode a single tile for now
    assert!(source.y().width() == self.y_width);
    assert!(source.y().height() == self.y_height);

    // We don't currently support lossless mode
    assert!(base_qindex != 0);

    // Allocate MI array
    let mi_rows = self.y_height / 4;
    let mi_cols = self.y_width / 4;

    let mut tile = TileEncoder {
      encoder: &self,
      bitstream: EntropyWriter::new(),
      base_qindex: base_qindex,
      mode_info: Array2D::zeroed(mi_rows, mi_cols),
      source: source,
      recon: Frame::new(self.y_height, self.y_width),
    };

    tile.encode();
    //tile.dump_recon("recon.y4m").unwrap();
    return tile.bitstream.finalize();
  }
}

impl<'a> TileEncoder<'a> {
  pub fn encode(&mut self) {
    let mi_rows = self.mode_info.rows();
    let mi_cols = self.mode_info.cols();
    let sb_rows = mi_rows.div_ceil(16);
    let sb_cols = mi_cols.div_ceil(16);

    for sb_row in 0..sb_rows {
      for sb_col in 0..sb_cols {
        self.encode_superblock(sb_row, sb_col);
      }
    }
  }

  fn encode_superblock(&mut self, sb_row: usize, sb_col: usize) {
    let mi_row = sb_row * 16;
    let mi_col = sb_col * 16;
    self.encode_partition(mi_row, mi_col, 64);
  }

  fn encode_partition(&mut self, mi_row: usize, mi_col: usize, bsize: usize) {
    //println!("Encoding {:2}x{:2} partition at mi_row={:3}, mi_col={:3}", bsize, bsize, mi_row, mi_col);
    // Always split down to 8x8 blocks
    // For each partition symbol, the context depends on whether the above and/or left
    // blocks are partitioned to a size smaller than what we're currently considering.
    // For blocks at one of the frame edges, the missing neighbour is assumed to be
    // the maximum possible size.
    //
    // Because we always split down to the same size, this ends up implying that the
    // context is:
    //
    // Current partition is 8x8: context = 0
    // Otherwise:
    //   Top-left corner: context = 0
    //   Left edge: context = 1
    //   Top edge: context = 2
    //   Everywhere else: context = 3
    if bsize == 8 {
      self.bitstream.write_symbol(0, &partition_8x8_cdf); // PARTITION_NONE
      self.encode_block(mi_row, mi_col, bsize);
    } else {
      let mi_rows = self.mode_info.rows();
      let mi_cols = self.mode_info.cols();

      let sub_rows = if (mi_row + bsize/8) < mi_rows { 2 } else { 1 };
      let sub_cols = if (mi_col + bsize/8) < mi_cols { 2 } else { 1 };

      let above_ctx = if mi_row > 0 { 1 } else { 0 };
      let left_ctx = if mi_col > 0 { 1 } else { 0 };
      let ctx = 2 * left_ctx + above_ctx;

      let cdf = match bsize {
        16 => &partition_16x16_cdf[ctx],
        32 => &partition_32x32_cdf[ctx],
        64 => &partition_64x64_cdf[ctx],
        _ => panic!("Reached an unexpected partition size")
      };

      if sub_rows > 1 && sub_cols > 1 {
        // Normal case, all partitions are available
        // Always choose PARTITION_SPLIT
        self.bitstream.write_symbol(3, cdf);
      } else if sub_cols > 1 {
        // The bottom edge of the frame falls in the top half of this partition, so
        // we must split horizontally. The only useful choice is whether to split the
        // in-bounds part in half vertically.
        //
        // Thus we use a binary CDF to pick between PARTITION_HORZ (0) or PARTITION_SPLIT (1).
        // The probability of PARTITION_SPLIT is calculated by summing the probabilities
        // of the following options using the original CDF:
        let p_split = get_prob(Partition::VERT as usize, cdf) +
                      get_prob(Partition::SPLIT as usize, cdf) +
                      get_prob(Partition::HORZ_A as usize, cdf) +
                      get_prob(Partition::VERT_A as usize, cdf) +
                      get_prob(Partition::VERT_B as usize, cdf) +
                      get_prob(Partition::VERT_4 as usize, cdf);
        self.bitstream.write_bit(1, 32768 - p_split);
      } else if sub_rows > 1 {
        // The right edge of the frame falls in the left half of this partition, so
        // we must split vertically. The only useful choice is whether to split the
        // in-bounds part in half horizontally.
        //
        // Thus we use a binary CDF to pick between PARTITION_VERT (0) or PARTITION_SPLIT (1).
        // The probability of PARTITION_SPLIT is calculated by summing the probabilities
        // of the following options using the original CDF:
        let p_split = get_prob(Partition::HORZ as usize, cdf) +
                      get_prob(Partition::SPLIT as usize, cdf) +
                      get_prob(Partition::HORZ_A as usize, cdf) +
                      get_prob(Partition::HORZ_B as usize, cdf) +
                      get_prob(Partition::VERT_A as usize, cdf) +
                      get_prob(Partition::HORZ_4 as usize, cdf);
        self.bitstream.write_bit(1, 32768 - p_split);
      } else {
        // The bottom-right corner of the frame falls in the top-left quadrant of this partition,
        // so PARTITION_SPLIT is forced. Therefore we don't need to signal anything.
      }

      let offset = bsize / 8;
      for i in 0..sub_rows {
        for j in 0..sub_cols {
          self.encode_partition(mi_row + i*offset, mi_col + j*offset, bsize/2);
        }
      }
    }
  }

  fn encode_coeffs(&mut self, plane: usize, mi_row: usize, mi_col: usize, bsize: usize, this_mi: &mut ModeInfo,
                   coeffs: &Array2D<i32>) {
    if bsize != 8 {
      todo!();
    }

    // Make sure there are the right number of coefficients
    let txsize = if plane > 0 { bsize/2 } else { bsize };
    let txs_ctx = if txsize == 8 { 1 } else { 0 };
    let num_coeffs = txsize * txsize;
    assert!(coeffs.rows() == txsize);
    assert!(coeffs.cols() == txsize);

    let scan: &[(u8, u8)] = scan_order_2d[txs_ctx];

    let qctx = get_qctx(self.base_qindex);

    let ptype = if plane == 0 { 0 } else { 1 };

    // Find the "end of block" location
    // This is one past the last nonzero coefficient, or 0 if all coeffs are zero
    let mut eob = 0;
    let mut culLevel = 0; // "Cumulative level", gets stored into this_mi.level_ctx
    for c in 0..num_coeffs {
      let (row, col) = scan[c];
      let coeff = coeffs[row as usize][col as usize];
      culLevel += abs(coeff);
      if coeff != 0 {
        eob = c + 1;
      }
    }
    this_mi.level_ctx[plane] = min(culLevel, 63) as u8;

    let all_zero = eob == 0;

    // The all_zero symbol has a complex dependency on the nearby transform coefficients.
    // For luma, there is a special case where this is short-circuited to 0 for max-size
    // transforms (ie, transform size == block size), so we can ignore the complex logic.
    // But for chroma it is mandatory.
    let all_zero_ctx = if plane == 0 {
      0
    } else {
      let mut above = false;
      let mut left = false;
      // In theory we need to scan all blocks above and left of the current block here
      // However, because all blocks are currently 8x8, there's always exactly one
      // block above and one block left
      if mi_row > 0 {
        let above_block = &self.mode_info[mi_row - 1][mi_col];
        above |= above_block.level_ctx[plane] != 0;
        above |= above_block.dc_sign[plane] != 0;
      }
      if mi_col > 0 {
        let left_block = &self.mode_info[mi_row][mi_col - 1];
        left |= left_block.level_ctx[plane] != 0;
        left |= left_block.dc_sign[plane] != 0;
      }
      7 + (above as usize) + (left as usize)
    };

    self.bitstream.write_symbol(all_zero as usize, &all_zero_cdf[qctx][txs_ctx][all_zero_ctx]);
    if all_zero {
      return;
    }

    // Transform type - only coded for luma
    // As we selected the reduced transform set in the frame header,
    // we end up looking at the TX_SET_INTRA_2 set, which consists of
    // { IDTX, DCT_DCT, ADST_ADST, ADST_DCT, DCT_ADST }, in that order.
    // We want DCT_DCT, so we want to encode index 1.
    if plane == 0 {
      self.bitstream.write_symbol(1, &tx_type_cdf);
    }

    // Number of coefficients, encoded as a logarithmic class + value within that class
    // Here, the contexts are qindex, plane type, and (for 16x16 and smaller)
    // whether the selected transform type is 1D (last context = 1) or 2D
    // (last context = 0). We always choose DCT_DCT, which counts as a 2D transform
    //
    // The EOB is split into a class plus optional extra bits. Each class has the following range:
    // Class 0 => EOB = 1
    // Class 1 => EOB = 2
    // Class 2 => EOB = 3-4
    // Class 3 => EOB = 5-8
    // ...
    // up to a maximum class which depends on the transform size
    // For 4x4 the largest class is class 4 (EOB = 9-16), for 8x8 it's class 6 (EOB = 33-64)
    let eob_class = ceil_log2(eob) as usize;
    let eob_class_cdf: &[u16] = if plane == 0 {
      &eob_class_64_cdf[qctx][ptype]
    } else {
      &eob_class_16_cdf[qctx][ptype]
    };
    self.bitstream.write_symbol(eob_class, eob_class_cdf);

    if eob_class > 1 {
      let eob_class_low = (1 << (eob_class - 1)) + 1;
      let eob_class_hi = 1 << eob_class;
      assert!(eob_class_low <= eob && eob <= eob_class_hi);

      // EOB classes 2+ require extra bits
      // The first extra bit is coded with a special CDF, the rest are literal bits
      // Context = (qctx, tx size, ptype, eob_class - 2)
      // For 8x8 and luma, this gives:
      let first_extra_bit_cdf = if plane == 0 {
        &eob_extra_8x8_cdf[qctx][ptype][eob_class - 2]
      } else {
        &eob_extra_4x4_cdf[qctx][ptype][eob_class - 2]
      };
      let eob_shift = eob_class - 2;
      let extra_bit = ((eob - eob_class_low) >> eob_shift) & 1;
      self.bitstream.write_symbol(extra_bit, first_extra_bit_cdf);

      // Write any remaining bits as a literal
      // Note: The AV1 decoder spec gives a more detailed process here,
      // but it's just writing individual bits from high to low,
      // which is exactly what write_literal() does
      let remainder = eob - eob_class_low - (extra_bit << eob_shift);
      let remainder_bits = eob_class - 2;
      self.bitstream.write_literal(remainder as u32, remainder_bits as u32);
    }

    // Write "base range" for each coefficient, in high-to-low index order
    for c in (0..eob).rev() {
      // Split coefficient into absolute value and sign, as these are coded separately
      let (row, col) = scan[c];
      let coeff = coeffs[row as usize][col as usize];
      let abs_value = unsigned_abs(coeff) as usize;

      // Code coeff_base symbol, which can indicate values 0, 1, 2, or 3+
      if c == eob - 1 {
        // Last nonzero coefficient, so we know this can't be zero
        // Therefore we use a separate set of CDFs and contexts
        let base_eob_ctx = if c == 0 {
          0
        } else if c <= num_coeffs/8 {
          1
        } else if c <= num_coeffs/4 {
          2
        } else {
          3
        };
        assert!(abs_value >= 1);
        let coded_value = min(abs_value - 1, 2);
        self.bitstream.write_symbol(coded_value, &coeff_base_eob_cdf[qctx][txs_ctx][ptype][base_eob_ctx]);
      } else {
        // Context depends on the base values of coefficients below and to the right,
        // which have already been encoded
        let base_ctx = if c == 0 {
          0
        } else {
          let mut mag = 0;

          for (row_off, col_off) in Sig_Ref_Diff_Offset {
            let ref_row = (row + row_off) as usize;
            let ref_col = (col + col_off) as usize;
            if ref_row < txsize && ref_col < txsize {
              mag += min(abs(coeffs[ref_row][ref_col]), 3);
            }
          }

          let mag_part = min(round2(mag, 1), 4) as usize;
          let loc_part = Coeff_Base_Ctx_Offset_8x8[min(row, 4) as usize][min(col, 4) as usize] as usize;
          mag_part + loc_part
        };

        let coded_value = min(abs_value, 3);
        self.bitstream.write_symbol(coded_value, &coeff_base_cdf[qctx][txs_ctx][ptype][base_ctx]);
      }

      // If coeff_base is 3, we can encode up to 4 symbols to increment the
      // absolute value further. This can directly encode values up to 14,
      // or the value 15 for all larger coefficients, in which case the remainder
      // is Golomb encoded in a separate pass
      if abs_value > 2 {
        // All four coeff_br symbols use the same context and CDF, so compute that first
        let br_ctx = {
          let mut mag = 0;

          for (row_off, col_off) in Mag_Ref_Offset {
            let ref_row = (row + row_off) as usize;
            let ref_col = (col + col_off) as usize;
            if ref_row < txsize && ref_col < txsize {
              mag += min(abs(coeffs[ref_row][ref_col]), 15);
            }
          }

          let mag_part = min(round2(mag, 1), 6) as usize;
          let loc_part = if c == 0 {
            0
          } else if row < 2 && col < 2 {
            7
          } else {
            14
          };
          mag_part + loc_part
        };

        // Now encode the coeff_br symbols
        let mut level = 3;
        for _ in 0..4 {
          let coeff_br = min(abs_value - level, 3);
          self.bitstream.write_symbol(coeff_br as usize, &coeff_br_cdf[qctx][txs_ctx][ptype][br_ctx]);
          level += coeff_br;
          if coeff_br < 3 {
            break;
          }
        }
      }
    }

    // Code DC sign + golomb bits
    let dc_coeff = coeffs[0][0];
    if dc_coeff != 0 {
      // The DC sign context depends on whether there are more +ve signs, more -ve signs,
      // or an equal number, among all above and left 4x4 units. Since we always use 8x8
      // blocks, there is exactly one above and one left neighbour.
      //
      // Also, for the chroma planes, in theory we're only meant to look at the blocks which are "chroma references",
      // i.e. the ones which contain an MI unit with odd mi_row and mi_col. This matters if we ever support 4x4
      // block sizes, but as we currently don't, that's just every block.
      //
      // Therefore we can simplify the scan given in the spec, into just looking at the single above and single left
      // block, if they exist.
      //
      // As we store the DC sign in ModeInfo::dc_sign as -1 / 0 / +1, we can do this by
      // simply summing the DC signs of all surrounding blocks
      let mut net_neighbour_sign = 0;
      if mi_row > 0 {
        net_neighbour_sign += self.mode_info[mi_row - 1][mi_col].dc_sign[plane];
      }
      if mi_col > 0 {
        net_neighbour_sign += self.mode_info[mi_row][mi_col - 1].dc_sign[plane];
      }
  
      // Map result to the appropriate context
      let dc_sign_ctx = if net_neighbour_sign == 0 {
        0
      } else if net_neighbour_sign < 0 {
        1
      } else {
        2
      };

      let sign = if dc_coeff < 0 { 1 } else { 0 };
      self.bitstream.write_symbol(sign, &dc_sign_cdf[qctx][ptype][dc_sign_ctx]);
    }
    if abs(dc_coeff) >= 15 {
      self.bitstream.write_golomb(unsigned_abs(dc_coeff) - 15);
    }

    // Store DC sign for reference by later blocks
    this_mi.dc_sign[plane] = signum(dc_coeff) as i8;

    // Code sign + golomb bits for the rest of coefficients
    // Note that this is done in low-to-high index order, in contrast to the earlier loop
    for c in 1..eob {
      let (row, col) = scan[c];
      let coeff = coeffs[row as usize][col as usize];
      if coeff != 0 {
        let sign = if coeff < 0 { 1 } else { 0 };
        self.bitstream.write_literal(sign, 1);
      }

      if abs(coeff) >= 15 {
        self.bitstream.write_golomb(unsigned_abs(coeff) - 15);
      }
    }
  }

  fn encode_block(&mut self, mi_row: usize, mi_col: usize, bsize: usize) {
    assert!(bsize == 8);

    //println!("Encoding 8x8 block at mi_row={:3}, mi_col={:3}", mi_row, mi_col);

    // Allocate a ModeInfo struct to hold information about the current block
    let mut this_mi = ModeInfo::zeroed();

    // For skip, the context depends on the above and left skip flags,
    // defaulting to false if those aren't present
    // As we always set skip = false, this context is always 0
    // skip = false
    self.bitstream.write_symbol(0, &skip_cdf);
  
    // For intra_frame_y_mode, the context depends on the above and left Y modes,
    // defaulting to DC_PRED if those aren't present
    // As we always choose DC_PRED, this context is always 0
    // intra_frame_y_mode(context=0,0) = DC_PRED
    self.bitstream.write_symbol(0, &y_mode_cdf);

    // For uv_mode, the context is simply y_mode combined with whether CFL is allowed
    // Here the y mode is always DC_PRED and CFL is always allowed for 8x8 blocks,
    // so we always end up with the same context
    // uv_mode(context=0, CFL allowed) = DC_PRED
    self.bitstream.write_symbol(0, &uv_mode_cdf);

    // Encode residuals
    for plane in 0..3 {
      let subsampling = if plane > 0 { 1 } else { 0 };
      let y0 = (mi_row * 4) >> subsampling;
      let x0 = (mi_col * 4) >> subsampling;
      let h = bsize >> subsampling;
      let w = bsize >> subsampling;

      dc_predict(self.recon.plane_mut(plane).pixels_mut(), y0, x0, h, w);
      let mut residual = compute_residual(self.source.plane(plane).pixels(),
                                          self.recon.plane(plane).pixels(),
                                          y0, x0, h, w);
      quantize(&mut residual, self.base_qindex);

      // Encode the quantized coefficients while we have them,
      // before we consume them to finalize the reconstructed image
      self.encode_coeffs(plane, mi_row, mi_col, bsize, &mut this_mi, &residual);

      dequantize(&mut residual, self.base_qindex);
      apply_residual(self.recon.plane_mut(plane).pixels_mut(), residual, y0, x0, h, w);
    }

    // Save mode info
    self.mode_info.fill_region(mi_row, mi_col, bsize/4, bsize/4, &this_mi);
  }

  fn dump_recon(&mut self, path: &str) -> Result<(), io::Error> {
    let mut y4m = Y4MWriter::new(File::create(path)?, self.encoder.y_width, self.encoder.y_height)?;
    y4m.write_frame(&self.recon)?;
    Ok(())
  }
}
