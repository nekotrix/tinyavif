// Copyright (c) 2024-2025, The tinyavif contributors. All rights reserved
//
// This source code is subject to the terms of the BSD 2 Clause License and
// the Alliance for Open Media Patent License 1.0. If the BSD 2 Clause License
// was not distributed with this source code in the LICENSE file, you can
// obtain it at www.aomedia.org/license/software. If the Alliance for Open
// Media Patent License 1.0 was not distributed with this source code in the
// PATENTS file, you can obtain it at www.aomedia.org/license/patent.

use crate::bitcode::BitWriter;
use crate::cdf::*;
use crate::entropycode::EntropyWriter;
use crate::util::*;

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

// Mutable state used while encoding a single tile
pub struct TileEncoder<'a> {
  encoder: &'a AV1Encoder,
  bitstream: EntropyWriter,
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
  
    // We don't currently support lossless mode
    assert!(base_qindex != 0);
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

  pub fn encode_image(&self, y_width: usize, y_height: usize) -> Box<[u8]> {
    let mut tile = TileEncoder {
      encoder: &self,
      bitstream: EntropyWriter::new(),
    };

    // We only currently support sizes which are a multiple of 64x64
    assert!(y_width % 64 == 0);
    assert!(y_height % 64 == 0);

    let mi_rows = y_height / 4;
    let mi_cols = y_width / 4;

    tile.encode(mi_rows, mi_cols);
    return tile.bitstream.finalize();
  }
}

impl<'a> TileEncoder<'a> {
  pub fn encode(&mut self, mi_rows: usize, mi_cols: usize) {
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

    // For each partition symbol, the context depends on whether the above and/or left
    // blocks are partitioned to a size smaller than what we're currently considering.
    // For blocks at one of the frame edges, the missing neighbour is assumed to be
    // the maximum possible size.
    //
    // In this simplified encoder, the frame is a multiple of 64x64 pixels.
    // This means we never have to deal with the modified partition syntax or
    // forced partitioning which can happen at the frame edge.
    // Further, because we always use max-size partitions, this context is
    // always the same, and we always encode PARTITION_NONE (0)
    self.bitstream.write_symbol(0, &partition_64x64_cdf);

    self.encode_block(mi_row, mi_col, bsize);
  }

  fn encode_block(&mut self, mi_row: usize, mi_col: usize, bsize: usize) {
    assert!(bsize == 64);

    //println!("Encoding block at mi_row={:3}, mi_col={:3}", mi_row, mi_col);

    // For skip, the context depends on the above and left skip flags,
    // defaulting to false if those aren't present
    let has_above = (mi_row > 0) as usize;
    let has_left = (mi_col > 0) as usize;
    let skip_ctx = has_above + has_left;
    self.bitstream.write_symbol(1, &skip_cdf[skip_ctx]);
  
    // For intra_frame_y_mode, the context depends on the above and left Y modes,
    // defaulting to DC_PRED if those aren't present
    // As we always choose DC_PRED, this context is always 0
    // intra_frame_y_mode(context=0,0) = DC_PRED
    self.bitstream.write_symbol(0, &y_mode_cdf);

    // For uv_mode, the context is simply y_mode combined with whether CFL is allowed
    // Here the y mode is always DC_PRED and CFL is never allowed for 64x64 blocks,
    // so we always end up with the same context
    // uv_mode(context=0, CFL not allowed) = DC_PRED
    self.bitstream.write_symbol(0, &uv_mode_cdf);
  }
}
