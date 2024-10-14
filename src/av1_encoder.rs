use bytemuck::Zeroable;

use crate::array_2d::Array2D;
use crate::bitcode::BitWriter;
use crate::consts::*;
use crate::entropycode::EntropyWriter;
use crate::enums::*;
use crate::frame::Frame;
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

  qindex: u8
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
  mode_info: Array2D<ModeInfo>,
  frame: Frame
}

impl AV1Encoder {
  pub fn new(y_crop_width: usize, y_crop_height: usize, qindex: u8) -> Self {
    // Check limits imposed by AV1
    assert!(0 < y_crop_width && y_crop_width <= 65536);
    assert!(0 < y_crop_height && y_crop_height <= 65536);

    // We don't currently support lossless mode
    assert!(qindex != 0);

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
      qindex: qindex
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
    w.write_bits(2, 2); // Chroma co-located with top-left luma pixel - TODO check what default is for "real" images
    w.write_bit(0); // UV channels have shared delta-q values
  
    w.write_bit(0); // No film grain
  
    // Sequence headers always appear in their own OBU, so always add a trailing 1 bit
    return w.finalize(true);
  }
  
  pub fn generate_frame_header(&self, add_trailing_one_bit: bool) -> Box<[u8]> {
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
  
    w.write_bits(self.qindex as u64, 8);
  
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

  pub fn encode_image(&self) -> Box<[u8]> {
    // Encode a single tile for now

    // Allocate MI array
    let mi_rows = self.y_height / 4;
    let mi_cols = self.y_width / 4;

    let mut tile = TileEncoder {
      encoder: &self,
      bitstream: EntropyWriter::new(),
      mode_info: Array2D::zeroed(mi_rows, mi_cols),
      frame: Frame::new(self.y_crop_width, self.y_crop_height)
    };

    tile.encode();
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
      let cdf = [19132, 25510, 30392];
      self.bitstream.write_symbol(0, &cdf); // PARTITION_NONE
      self.encode_block(mi_row, mi_col, bsize);
    } else {
      let mi_rows = self.mode_info.rows();
      let mi_cols = self.mode_info.cols();

      let sub_rows = if (mi_row + bsize/8) < mi_rows { 2 } else { 1 };
      let sub_cols = if (mi_col + bsize/8) < mi_cols { 2 } else { 1 };

      let all_cdfs = [
        // 16x16
        [
          [15597, 20929, 24571, 26706, 27664, 28821, 29601, 30571, 31902],
          [7925, 11043, 16785, 22470, 23971, 25043, 26651, 28701, 29834],
          [5414, 13269, 15111, 20488, 22360, 24500, 25537, 26336, 32117],
          [2662, 6362, 8614, 20860, 23053, 24778, 26436, 27829, 31171]
        ],
        // 32x32
        [
          [18462, 20920, 23124, 27647, 28227, 29049, 29519, 30178, 31544],
          [7689, 9060, 12056, 24992, 25660, 26182, 26951, 28041, 29052],
          [6015, 9009, 10062, 24544, 25409, 26545, 27071, 27526, 32047],
          [1394, 2208, 2796, 28614, 29061, 29466, 29840, 30185, 31899]
        ],
        // 64x64
        [
          [20137, 21547, 23078, 29566, 29837, 30261, 30524, 30892, 31724],
          [6732, 7490, 9497, 27944, 28250, 28515, 28969, 29630, 30104],
          [5945, 7663, 8348, 28683, 29117, 29749, 30064, 30298, 32238],
          [870, 1212, 1487, 31198, 31394, 31574, 31743, 31881, 32332]
        ]
      ];
      let bsize_ctx = match bsize {
        16 => 0,
        32 => 1,
        64 => 2,
        _ => panic!("Reached an unexpected partition size")
      };
      let above_ctx = if mi_row > 0 { 1 } else { 0 };
      let left_ctx = if mi_col > 0 { 1 } else { 0 };
      let ctx = 2 * left_ctx + above_ctx;

      let cdf = &all_cdfs[bsize_ctx][ctx];

      if sub_rows > 1 && sub_cols > 1 {
        self.bitstream.write_symbol(3, cdf); // PARTITION_SPLIT
      } else if sub_cols > 1 {
        // Derive a binary CDF to pick between PARTITION_HORZ (0) or PARTITION_SPLIT (1)
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
        // Derive a binary CDF to pick between PARTITION_VERT (0) or PARTITION_SPLIT (1)
        let p_split = get_prob(Partition::HORZ as usize, cdf) +
                      get_prob(Partition::SPLIT as usize, cdf) +
                      get_prob(Partition::HORZ_A as usize, cdf) +
                      get_prob(Partition::HORZ_B as usize, cdf) +
                      get_prob(Partition::VERT_A as usize, cdf) +
                      get_prob(Partition::HORZ_4 as usize, cdf);
        self.bitstream.write_bit(1, 32768 - p_split);
      } else {
        // PARTITION_SPLIT is forced, so no need to encode anything
      }

      let offset = bsize / 8;
      for i in 0..sub_rows {
        for j in 0..sub_cols {
          self.encode_partition(mi_row + i*offset, mi_col + j*offset, bsize/2);
        }
      }
    }
  }

  fn dc_sign_ctx(&self, plane: usize, mi_row: usize, mi_col: usize, bsize: usize) -> usize {
    if plane != 0 {
      todo!();
    }

    let mi_rows = self.mode_info.rows();
    let mi_cols = self.mode_info.cols();

    // The DC sign context depends on whether there are more +ve signs, more -ve signs,
    // or an equal number, among all above and left 4x4 units.
    // As we store the DC sign in ModeInfo::dc_sign as -1 / 0 / +1, we can do this by
    // simply summing the DC signs of all surrounding blocks
    let mut net_neighbour_sign = 0;
    if mi_row > 0 {
      for above_col in mi_col .. min(mi_col + bsize/4, mi_cols) {
        net_neighbour_sign += self.mode_info[mi_row - 1][above_col].dc_sign[plane]
      }
    }
    if mi_col > 0 {
      for left_row in mi_row .. min(mi_row + bsize/4, mi_rows) {
        net_neighbour_sign += self.mode_info[left_row][mi_col - 1].dc_sign[plane]
      }
    }

    // Map result to the appropriate context
    if net_neighbour_sign == 0 {
      return 0;
    } else if net_neighbour_sign < 0 {
      return 1;
    } else {
      return 2;
    }
  }

  fn encode_coeffs(&mut self, plane: usize, mi_row: usize, mi_col: usize, bsize: usize, this_mi: &mut ModeInfo,
                   coeffs: &Array2D<i32>) {
    // We only handle 8x8 luma blocks for now
    if bsize != 8 {
      todo!();
    }

    // Make sure there are the right number of coefficients
    let txsize = if plane > 0 { bsize/2 } else { bsize };
    let num_coeffs = txsize * txsize;
    assert!(coeffs.rows() == txsize);
    assert!(coeffs.cols() == txsize);

    let scan: &[(u8, u8)] = if txsize == 4 { &default_scan_4x4 } else { &default_scan_8x8 };

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

    if plane != 0 {
      // For chroma, we don't signal any coefficients for now
      // This ends up meaning that the main context is always 7
      if ! all_zero {
        todo!();
      }
      // all_zero(u/v, context=3,0,7) = 1
      self.bitstream.write_bool(all_zero, 2713);
      return;
    }

    // Note on contexts:
    // Coeff symbols have an implicit qindex-based context, which is:
    //  if   qindex <= 20  then qctx = 0
    //  elif qindex <= 60  then qctx = 1
    //  elif qindex <= 120 then qctx = 2
    //  else                    qctx = 3 (this is the selected qindex for now)
    //
    // This context is selected at each past-independent frame, and then held
    // across any dependent frames. In our case, where every frame is a key frame,
    // this means that it depends on the frame-level base_qindex.
    //
    // Then three other, more dynamic, values are factored into the context:
    // * Transform size, which in this case is 1 (8x8) for luma and 0 (4x4) for chroma
    // * Whether the current plane is luma or chroma (the "plane type")
    // * Surrounding coefficient values
    // The last two are bundled together in a complex way into a value we'll call
    // the "main context"
    assert!(self.encoder.qindex > 120);

    // For luma, for the all_zero symbol, the main context in theory has a complex dependency
    // on the nearby transform coefficients, but it's short-circuited to always be 0 for
    // max-size transforms (ie, transform size == block size), which saves us a lot of work!
    // all_zero(y, context=3,1,0) = 0
    self.bitstream.write_bool(all_zero, 31903);
    if all_zero {
      return;
    }

    // Transform type
    // As we selected the reduced transform set in the frame header,
    // we end up looking at the TX_SET_INTRA_2 set, which consists of
    // { IDTX, DCT_DCT, ADST_ADST, ADST_DCT, DCT_ADST }, in that order.
    // We want DCT_DCT, so we want to encode index 1.
    // The context here consists of the TX size (rounded down to a square size,
    // but 8x8 is already square) and the intra mode, which here is always DC_PRED
    self.bitstream.write_symbol(1, &[6554, 13107, 19661, 26214]);

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
    self.bitstream.write_symbol(eob_class, &[6307, 7541, 12060, 16358, 22553, 27865]);
    if eob_class > 1 {
      // EOB classes 2+ require extra bits
      // The first extra bit is coded with a special CDF, the rest are literal bits
      // Context = (qctx, tx size, ptype, eob_class - 2)
      // For 8x8 and luma, this gives:
      let first_extra_bit_cdf = [
        [20238],
        [21057],
        [19159],
        [22337],
        [20159]
      ];
      let eob_class_low = (1 << (eob_class - 1)) + 1;
      let eob_class_hi = 1 << eob_class;
      assert!(eob_class_low <= eob && eob <= eob_class_hi);

      let eob_shift = eob_class - 2;
      let extra_bit = ((eob - eob_class_low) >> eob_shift) & 1;
      self.bitstream.write_symbol(extra_bit, &first_extra_bit_cdf[eob_class - 2]);

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
        let coeff_base_eob_cdf = [
          [21457, 31043],
          [31951, 32483],
          [32153, 32562],
          [31473, 32215]
        ];
        assert!(abs_value >= 1);
        self.bitstream.write_symbol(abs_value - 1, &coeff_base_eob_cdf[base_eob_ctx]);
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

        let coeff_base_cdf = [
          [7754, 16948, 22142],
          [25670, 32330, 32691],
          [15663, 29225, 31994],
          [9878, 23288, 29158],
          [6419, 17088, 24336],
          [3859, 11003, 17039],
          [27562, 32595, 32725],
          [17575, 30588, 32399],
          [10819, 24838, 30309],
          [7124, 18686, 25916],
          [4479, 12688, 19340],
          [28385, 32476, 32673],
          [15306, 29005, 31938],
          [8937, 21615, 28322],
          [5982, 15603, 22786],
          [3620, 10267, 16136],
          [27280, 32464, 32667],
          [15607, 29160, 32004],
          [9091, 22135, 28740],
          [6232, 16632, 24020],
          [4047, 11377, 17672],
          [29220, 32630, 32718],
          [19650, 31220, 32462],
          [13050, 26312, 30827],
          [9228, 20870, 27468],
          [6146, 15149, 21971],
          [30169, 32481, 32623],
          [17212, 29311, 31554],
          [9911, 21311, 26882],
          [4487, 13314, 20372],
          [2570, 7772, 12889],
          [30924, 32613, 32708],
          [19490, 30206, 32107],
          [11232, 23998, 29276],
          [6769, 17955, 25035],
          [4398, 12623, 19214],
          [30609, 32627, 32722],
          [19370, 30582, 32287],
          [10457, 23619, 29409],
          [6443, 17637, 24834],
          [4645, 13236, 20106],
          [8192, 16384, 24576]
        ];
        self.bitstream.write_symbol(abs_value, &coeff_base_cdf[base_ctx]);
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

        let coeff_br_cdf = [
          [18274, 24813, 27890],
          [15537, 23149, 27003],
          [9449, 16740, 21827],
          [6700, 12498, 17261],
          [4988, 9866, 14198],
          [4236, 8147, 11902],
          [2867, 5860, 8654],
          [17124, 23171, 26101],
          [20396, 27477, 30148],
          [16573, 24629, 28492],
          [12749, 20846, 25674],
          [10233, 17878, 22818],
          [8525, 15332, 20363],
          [6283, 11632, 16255],
          [20466, 26511, 29286],
          [23059, 29174, 31191],
          [19481, 27263, 30241],
          [15458, 23631, 28137],
          [12416, 20608, 25693],
          [10261, 18011, 23261],
          [8016, 14655, 19666]
        ];

        // Now encode the coeff_br symbols
        let mut level = 3;
        for _ in 0..4 {
          let coeff_br = min(abs_value - level, 3);
          self.bitstream.write_symbol(coeff_br as usize, &coeff_br_cdf[br_ctx]);
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
      // dc_sign(context=3,0,neighbour signs) = 0
      // TODO: Update to include chroma
      let sign = if dc_coeff < 0 { 1 } else { 0 };
      let dc_sign_cdf = [
        [128 * 125],
        [128 * 102],
        [128 * 147]
      ];
      let dc_sign_ctx = self.dc_sign_ctx(plane, mi_row, mi_col, bsize);
      self.bitstream.write_symbol(sign, &dc_sign_cdf[dc_sign_ctx]);
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

    // Allocate a ModeInfo struct to hold information about the current block
    let mut this_mi = ModeInfo::zeroed();

    // For skip, the context depends on the above and left skip flags,
    // defaulting to false if those aren't present
    // As we always set skip = false, this context is always 0
    // skip = false
    self.bitstream.write_symbol(0, &[31671]);
  
    // For intra_frame_y_mode, the context depends on the above and left Y modes,
    // defaulting to DC_PRED if those aren't present
    // As we always choose DC_PRED, this context is always 0
    // intra_frame_y_mode(context=0,0) = DC_PRED
    self.bitstream.write_symbol(0, &[15588, 17027, 19338, 20218, 20682, 21110, 21825, 23244, 24189, 28165, 29093, 30466]);

    // For uv_mode, the context is simply y_mode combined with whether CFL is allowed
    // Here the y mode is always DC_PRED and CFL is always allowed for 8x8 blocks,
    // so we always end up with the same context
    // uv_mode(context=0, CFL allowed) = DC_PRED
    self.bitstream.write_symbol(0, &[10407, 11208, 12900, 13181, 13823, 14175, 14899, 15656, 15986, 20086, 20995, 22455, 24212]);

    // Encode residual per plane
    // TODO: Calculate residual and transform
    let mut y_coeffs = Array2D::zeroed(8, 8);
    if (mi_row + mi_col) % 8 < 4 {
      y_coeffs[1][0] = 1;
    } else {
      y_coeffs[0][1] = -1;
    }
    self.encode_coeffs(0, mi_row, mi_col, bsize, &mut this_mi, &y_coeffs);

    let uv_coeffs = Array2D::zeroed(4, 4);
    self.encode_coeffs(1, mi_row, mi_col, bsize, &mut this_mi, &uv_coeffs);
    self.encode_coeffs(2, mi_row, mi_col, bsize, &mut this_mi, &uv_coeffs);

    // Save mode info
    self.mode_info.fill_region(mi_row, mi_col, bsize/4, bsize/4, &this_mi);
  }
}
