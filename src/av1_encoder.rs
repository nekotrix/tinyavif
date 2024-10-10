use crate::bitcode::BitWriter;
use crate::entropycode::EntropyWriter;

// Top-level encoder state
pub struct AV1Encoder {
  width: usize,
  height: usize,
  qindex: u8
}

// Mutable state used while encoding a single tile
pub struct TileEncoder<'a> {
  encoder: &'a AV1Encoder,
  bitstream: EntropyWriter
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
    let mut tile = TileEncoder {
      encoder: &self,
      bitstream: EntropyWriter::new()
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
    // decode_partition at size 64x64

    // For partition, the context depends on the above and left partition sizes and
    // how they compare to the current partition size, defaulting to the max size
    // if those aren't present.
    // As we always use 64x64 partitions, this context is always 0
    // partition = PARTITION_NONE
    self.bitstream.write_symbol(0, &[20137, 21547, 23078, 29566, 29837, 30261, 30524, 30892, 31724]);

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
    // Here the y mode is always DC_PRED and CFL is never valid for 64x64 blocks
    // uv_mode = DC_PRED
    self.bitstream.write_symbol(0, &[22631, 24152, 25378, 25661, 25986, 26520, 27055, 27923, 28244, 30059, 30941, 31961]);
  
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
    // * Transform size, which in this case is 4 (64x64) for luma and 3 (32x32) for chroma
    // * Whether the current plane is luma or chroma (the "plane type")
    // * Surrounding coefficient values
    // The last two are bundled together in a complex way into a value we'll call
    // the "main context"
    assert!(self.encoder.qindex > 120);

    // For luma, for the all_zero symbol, the main context in theory has a complex dependency
    // on the nearby transform coefficients, but it's short-circuited to always be 0 for
    // max-size transforms (ie, transform size == block size), which saves us a lot of work!
    // all_zero(y, context=3,4,0) = 0
    self.bitstream.write_symbol(0, &[31539]); 

    // We don't need to signal a transform type, as the only valid type for 64x64 is DCT_DCT

    // Number of coefficients, encoded as a logarithmic class + value within that class
    // Here, the main context consists of the plane type, and *for 16x16 and smaller*
    // on whether this is a 1D or a 2D transform. For 64x64, it's always 2D.
    // eob_pt_1024(context=3,0) = 0, meaning 1 transform coefficient is present
    self.bitstream.write_symbol(0, &[6698, 8334, 11961, 15762, 20186, 23862, 27434, 29326, 31082, 32050]);

    // Base range of the single coefficient
    // There is a designated context for "the sole coefficient in a 1-coeff block",
    // so this ends up always being the main context
    // coeff_base_eob(context=3,4,0,0) = 1, meaning |quantized coefficient| = 2
    self.bitstream.write_symbol(1, &[12358, 24977]);

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
    let y_dc_sign_ctx = if sb_row == 0 && sb_col == 0 { 0 } else { 2 };
    self.bitstream.write_symbol(0, &y_dc_sign_cdf[y_dc_sign_ctx]);
  
    // For chroma, we don't signal any coefficients for now
    // This ends up meaning that the main context is always 7
    // all_zero(u, context=3,3,7) = 1
    self.bitstream.write_symbol(1, &[4656]);
    // all_zero(v, context=3,3,7) = 1
    self.bitstream.write_symbol(1, &[4656]);
  }
}
