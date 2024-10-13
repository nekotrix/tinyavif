use bytemuck::Zeroable;

use crate::bitcode::BitWriter;
use crate::entropycode::EntropyWriter;
use crate::array_2d::Array2D;

// Top-level encoder state
pub struct AV1Encoder {
  width: usize,
  height: usize,
  qindex: u8
}

// "Mode info" unit - a struct representing the state of a single 4x4 luma pixel unit.
// The values in here can be used as contexts when encoding later blocks
#[derive(Zeroable)]
pub struct ModeInfo {
  // Sign of the DC coefficient in this block
  // This is stored differently to what the spec says: we store
  // -1 if the DC coefficient is negative, 0 if zero, 1 if positive.
  // This way, we can compare the number of nearby +ve and -ve DC coefficients by
  // simply summing this value over nearby blocks.
  dc_sign: i8,
}

// Mutable state used while encoding a single tile
pub struct TileEncoder<'a> {
  encoder: &'a AV1Encoder,
  bitstream: EntropyWriter,
  mode_info: Array2D<ModeInfo>
}

impl AV1Encoder {
  pub fn new(width: usize, height: usize, qindex: u8) -> Self {
    // Check limits imposed by AV1
    assert!(0 < width && width <= 65536);
    assert!(0 < height && height <= 65536);

    // We don't currently support lossless mode
    assert!(qindex != 0);

    Self {
      width: width,
      height: height,
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
    w.write_bits((self.width-1) as u64, 16);
    w.write_bits((self.height-1) as u64, 16);
  
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
    if self.width > 64 {
      w.write_bit(0); // 1 tile column
    }
    if self.height > 64 {
      w.write_bit(0); // 1 tile row
    }
  
    w.write_bits(self.qindex as u64, 8);
  
    w.write_bits(0, 3); // No frame-level delta-qs (one bit per channel)
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
    let mi_rows = self.width.div_ceil(4);
    let mi_cols = self.height.div_ceil(4);

    let mut tile = TileEncoder {
      encoder: &self,
      bitstream: EntropyWriter::new(),
      mode_info: Array2D::zeroed(mi_rows, mi_cols)
    };

    tile.encode();
    return tile.bitstream.finalize();
  }
}

impl<'a> TileEncoder<'a> {
  pub fn encode(&mut self) {
    let sb_cols = self.encoder.width.div_ceil(64);
    let sb_rows = self.encoder.height.div_ceil(64);

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
      self.bitstream.write_symbol(3, cdf); // PARTITION_SPLIT

      let offset = bsize / 8;
      for i in 0..2 {
        for j in 0..2 {
          self.encode_partition(mi_row + i*offset, mi_col + j*offset, bsize/2);
        }
      }
    }
  }

  fn encode_block(&mut self, mi_row: usize, mi_col: usize, bsize: usize) {
    assert!(bsize == 8);

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

    // Transform type and coefficients per plane
    //
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
    self.bitstream.write_symbol(0, &[31903]);

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
    // eob_pt_64(context=3,0,0) = 0, meaning 1 transform coefficient is present
    self.bitstream.write_symbol(0, &[6307, 7541, 12060, 16358, 22553, 27865]);

    // Base range of the single coefficient
    // There is a designated context for "the sole coefficient in a 1-coeff block",
    // so this ends up always being the main context
    // coeff_base_eob(context=3,1,0,0) = 1, meaning |quantized coefficient| = 1
    self.bitstream.write_symbol(0, &[21457, 31043]);

    // Sign of the single coefficient
    // This is the one case where we need to (pretend to) track state even for this simple case
    // The DC sign context depends on whether there are more +ve signs, more -ve signs,
    // or an equal number, among all above and left 4x4 units.
    // In our case, the first transform block has 0 +ve and 0 -ve signs nearby,
    // so uses context 0, while all others have more +ves than -ves and so use context 2.
    // dc_sign(context=3,0,N) = 0, meaning quantized coefficient is +ve
    let y_dc_sign_cdf = [
      [128 * 125],
      [128 * 102],
      [128 * 147]
    ];
    let y_dc_sign_ctx = if mi_row == 0 && mi_col == 0 { 0 } else { 2 };
    self.bitstream.write_symbol(0, &y_dc_sign_cdf[y_dc_sign_ctx]);
  
    // For chroma, we don't signal any coefficients for now
    // This ends up meaning that the main context is always 7
    // all_zero(u, context=3,0,7) = 1
    self.bitstream.write_symbol(1, &[2713]);
    // all_zero(v, context=3,0,7) = 1
    self.bitstream.write_symbol(1, &[2713]);
  }
}
