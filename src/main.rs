//

#![allow(dead_code)]
#![allow(unused_variables)]
#![allow(unused_imports)]

mod bitcode;
mod entropycode;
mod frame;
mod util;
mod y4m;

use std::io::prelude::*;
use std::fs::File;

use bitcode::BitWriter;
use entropycode::EntropyWriter;
use util::write_leb128;

fn generate_sequence_header(width: usize, height: usize) -> Box<[u8]> {
  assert!(0 < width && width <= 65536);
  assert!(0 < height && height <= 65536);

  let mut w = BitWriter::new();
  
  w.write_bits(0, 3); // "Main" profile: 8 or 10 bits, YUV 4:2:0 or monochrome
  w.write_bit(1); // Still picture
  w.write_bit(1); // with simplified headers

  w.write_bits(31, 5); // Level = 31, a special value meaning no level-based constraints apply

  // Width and height - we first code how many bits to use for each value (here just use 16,
  // for simplicity), then one less than the actual width and height
  w.write_bits(15, 4);
  w.write_bits(15, 4);
  w.write_bits((width-1) as u64, 16);
  w.write_bits((height-1) as u64, 16);

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

fn generate_frame_header(width: usize, height: usize, qindex: u8, add_trailing_one_bit: bool) -> Box<[u8]> {
  assert!(0 < width && width <= 65536);
  assert!(0 < height && height <= 65536);
  assert!(qindex != 0); // Allowed by AV1, but indicates lossless mode, which we don't support

  let mut w = BitWriter::new();
  
  w.write_bit(1); // Disable CDF updates
  w.write_bit(0); // Disable screen content tools
  w.write_bit(0); // Render size = frame size

  // Tile info
  // We need to code a tiling mode, then two zero bits to select 1x1 tiling.
  // However, if the width or height is less than one superblock (ie, 64 pixels), the
  // corresponding flag is implicitly set to 0 and doesn't need to be signalled.
  // So we need to add these conditionally
  w.write_bit(1); // Uniform tile mode - allows the cheapest signaling of 1x1 tile
  if width > 64 {
    w.write_bit(0); // 1 tile column
  }
  if height > 64 {
    w.write_bit(0); // 1 tile row
  }

  w.write_bits(qindex as u64, 8);

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

fn encode_image(width: usize, height: usize, qindex: u8) -> Box<[u8]> {
  // Temporary: Encode a fixed 64x64 image
  assert!(width == 64);
  assert!(height == 64);

  let mut e = EntropyWriter::new();

  // decode_partition at size 64x64
  // partition(width=64, context=0) = PARTITION_NONE
  e.write_symbol(0, &[20137, 21547, 23078, 29566, 29837, 30261, 30524, 30892, 31724]);
  // skip(context=0) = 0
  e.write_symbol(0, &[31671]);
  // intra_frame_y_mode(context=0,0) = DC_PRED
  e.write_symbol(0, &[15588, 17027, 19338, 20218, 20682, 21110, 21825, 23244, 24189, 28165, 29093, 30466]);
  // uv_mode(context=0, cfl disallowed) = DC_PRED
  e.write_symbol(0, &[22631, 24152, 25378, 25661, 25986, 26520, 27055, 27923, 28244, 30059, 30941, 31961]);

  // Residual coeffs per plane (iff skip == 0)
  // Note on contexts:
  // Coeff symbols have an implicit qindex-based context, which is:
  //  if   qindex <= 20  then qctx = 0
  //  elif qindex <= 60  then qctx = 1
  //  elif qindex <= 120 then qctx = 2
  //  else                    qctx = 3 (this is the selected qindex for now)
  //
  // This context is selected at each past-independent frame, and then held
  // across any dependent frames. In our case, where every frame is a key frame,
  // this means that the qindex used is the frame-level base_qindex.
  //
  // Then there is a tx-size based context, which in this case is 4 (64x64) for luma
  // and 3 (32x32) for chroma.
  // And finally a context which depends on what coefficients existed in
  // surrounding blocks (which for now are irrelevant) and the plane
  assert!(qindex > 120);
  // all_zero(y, context=3,4,0) = 0
  e.write_symbol(0, &[31539]); 
  // [tx type forced to be DCT_DCT as txfm is 64x64]
  // eob_pt_1024(context=3,0,0) = 0, meaning 1 transform coefficient is present
  e.write_symbol(0, &[6698, 8334, 11961, 15762, 20186, 23862, 27434, 29326, 31082, 32050]);
  // coeff_base_eob(context=3,4,0,0) = 0, meaning |quantized coefficient| = 1
  e.write_symbol(0, &[12358, 24977]);
  // dc_sign(context=3,0,0) = 0, meaning quantized coefficient = +1
  e.write_symbol(0, &[16000]);

  // all_zero(u, context=3,3,7) = 1
  e.write_symbol(1, &[4656]);
  // all_zero(v, context=3,3,7) = 1
  e.write_symbol(1, &[4656]);

  return e.finalize();
}

fn main() {
  let width = 64;
  let height = 64;
  let qindex = 200;

  // Generate AV1 data
  let sequence_header = generate_sequence_header(width, height);
  let frame_header = generate_frame_header(width, height, qindex, false);
  let tile_data = encode_image(width, height, qindex);

  // Pack into OBUs
  let mut av1_data = Vec::new();

  // Standalone AV1 streams *must* start with a temporal delimiter, even for
  // still images. This seems to not be necessary for AVIF files, though
  // libavif still includes one anyway
  av1_data.push(0b0001_0010); // Temporal delimiter OBU
  av1_data.push(0u8); // with a zero-byte payload

  av1_data.push(0b0000_1010); // Sequence header OBU
  write_leb128(&mut av1_data, sequence_header.len()); // Payload size
  av1_data.extend_from_slice(&sequence_header); // Payload

  av1_data.push(0b0011_0010); // Frame OBU: combined frame header + tile data
  write_leb128(&mut av1_data, frame_header.len() + tile_data.len());
  av1_data.extend_from_slice(&frame_header);
  av1_data.extend_from_slice(&tile_data);

  // Finally, dump to a file
  let mut f = File::create("test.obu").unwrap();
  f.write_all(&av1_data).unwrap();
}
