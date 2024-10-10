//

#![allow(dead_code)]
#![allow(unused_variables)]
#![allow(unused_imports)]

mod bitcode;
mod entropycode;
mod frame;
mod isobmff;
mod util;
mod y4m;

use std::io::prelude::*;
use std::fs::File;

use bitcode::BitWriter;
use entropycode::EntropyWriter;
use isobmff::ISOBMFFWriter;
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

fn pack_obus(sequence_header: &[u8], frame_header: &[u8], tile_data: &[u8], include_temporal_delimiter: bool) -> Box<[u8]> {
  let mut av1_data = Vec::new();

  // Optionally include temporal delimiter
  // Reasoning:
  //
  // 1) The AV1 spec says that AV1 streams must begin with a temporal delimiter,
  //    even for still images. Accordingly, libaom will refuse to decode any .obu file
  //    which doesn't start with one, whether invoked via aomdec or ffmpeg.
  //
  // 2) The AVIF spec explicitly states that this is *not* required in still-image AVIF files,
  //    and libavif does not require one
  //
  // 3) However, when generating AVIF files, libavif *does* include a temporal delimiter,
  //    while ffmpeg doesn't
  //
  // The upshot is that this is mandatory for .obu files, and optional for .avif files
  if include_temporal_delimiter {
    av1_data.push(0b0001_0010); // Temporal delimiter OBU
    av1_data.push(0u8); // with a zero-byte payload
  }

  av1_data.push(0b0000_1010); // Sequence header OBU
  write_leb128(&mut av1_data, sequence_header.len()); // Payload size
  av1_data.extend_from_slice(&sequence_header); // Payload

  av1_data.push(0b0011_0010); // Frame OBU: combined frame header + tile data
  write_leb128(&mut av1_data, frame_header.len() + tile_data.len());
  av1_data.extend_from_slice(&frame_header);
  av1_data.extend_from_slice(&tile_data);

  return av1_data.into_boxed_slice();
}

fn pack_avif(av1_data: &[u8], width: usize, height: usize) -> Box<[u8]> {
  let mut avif = ISOBMFFWriter::new();

  let content_pos_marker;
  let content_size = av1_data.len();

  // "File type" box
  let mut ftyp = avif.open_box(b"ftyp");
  ftyp.write_bytes(b"avif"); // Main file type
  ftyp.write_u32(0);         // AVIF version
  ftyp.write_bytes(b"avifmif1miafMA1B"); // "compatible brands"
  drop(ftyp);

  // Metadata box - contains the rest of the file header
  let mut meta = avif.open_box_with_version(b"meta", 0, 0);
  {
    // "Handler" box - TODO figure out what this does
    // TODO: Is this box needed? Can we put different values in here?
    let mut hdlr = meta.open_box_with_version(b"hdlr", 0, 0);
    hdlr.write_u32(0);
    hdlr.write_bytes(b"pict");
    hdlr.write_u32(0);
    hdlr.write_u32(0);
    hdlr.write_u32(0);
    hdlr.write_bytes(b"libavif\0"); // Pretend to be libavif for now
    drop(hdlr);

    // "Primary item" box
    let mut pitm = meta.open_box_with_version(b"pitm", 0, 0);
    pitm.write_u16(1); // Primary item is item number 1
    drop(pitm);

    // "Item location" box
    let mut iloc = meta.open_box_with_version(b"iloc", 0, 0);
    iloc.write_u8(0x44); // 4 bytes each for offset and length
    iloc.write_u8(0);    // No base offset; 4 reserved bits
    iloc.write_u16(1);   // One item

    iloc.write_u16(1); // Item ID 1:
    iloc.write_u16(0); // "Data reference index" = 0
    iloc.write_u16(1); // One extent
    // Allocate space for the content position, but we'll need to come back and fill it in later
    content_pos_marker = iloc.mark_u32();
    iloc.write_u32(content_size as u32); // Content length
    drop(iloc);

    // "Item info" box
    let mut iinf = meta.open_box_with_version(b"iinf", 0, 0);
    iinf.write_u16(1); // One item
    // "infe" box per item
    {
      let mut infe = iinf.open_box_with_version(b"infe", 2, 0);
      infe.write_u16(1);            // Item index 1
      infe.write_u16(0);            // "Protection" = 0
      infe.write_bytes(b"av01");    // This stream is AV1 :)
      infe.write_bytes(b"Color\0"); // and it's the main colour data, not, say, alpha data
      drop(infe);
    }
    drop(iinf);

    // "Image properties" box
    let mut iprp = meta.open_box(b"iprp");
    {
      // "Image property container" box
      let mut ipco = iprp.open_box(b"ipco");
      {
        // "Image spatial extent" box
        let mut ispe = ipco.open_box_with_version(b"ispe", 0, 0);
        ispe.write_u32(width as u32);
        ispe.write_u32(height as u32);
        drop(ispe);

        // "Pixel information" box
        let mut pixi = ipco.open_box_with_version(b"pixi", 0, 0);
        pixi.write_u8(3); // 3 channels...
        pixi.write_u8(8);
        pixi.write_u8(8);
        pixi.write_u8(8); // ...each of which is 8 bits per pixel
        drop(pixi);

        // AV1-specific info box
        #[allow(non_snake_case)]
        let mut av1C = ipco.open_box(b"av1C");
        av1C.write_u8(0x81);       // Custom version field: 1 bit marker that must be 1 + 7-bit version = 1
        av1C.write_u8(0x1F);       // Profile 0, level 31 (== unconstrained)
        av1C.write_u8(0b00001110); // Main tier, 8bpp, not monochrome, 4:2:0 subsampling, chroma located in top-left corner
        av1C.write_u8(0x10);       // No presentation delay info
        drop(av1C);

        // Colour info box
        // TODO: Decide what the colour settings should be
        let mut colr = ipco.open_box(b"colr");
        colr.write_bytes(b"nclx"); // Required subtype
        colr.write_u16(1); // Colour primaries (1 = BT.709)
        colr.write_u16(1); // Transfer function (1 = BT.709)
        colr.write_u16(1); // Matrix coefficients (1 = BT.709)
        colr.write_u8(0);  // TV colour range (change to 0x80 for full-range)
        drop(colr);
      }
      drop(ipco);

      // "Image property mapping association" box (TODO check name)
      let mut ipma = iprp.open_box_with_version(b"ipma", 0, 0);
      ipma.write_u32(1); // One item

      ipma.write_u16(1); // Item ID 1:
      ipma.write_u8(4); // Four associations
      // Associations - 1 byte each
      // Each has a 1-bit flag (0x80 bit) indicating whether the association is mandatory,
      // and a 7-bit ID which presumably indexes into the 'ipco' table above
      ipma.write_u8(1);
      ipma.write_u8(2);
      ipma.write_u8(0x83);
      ipma.write_u8(4);
      drop(ipma);
    }
    drop(iprp);
  }
  drop(meta);

  // Finally, the 'mdat' box contains the image data itself
  let mut mdat = avif.open_box(b"mdat");
  let content_pos = mdat.get_file_pos() as u32;
  mdat.write_bytes(av1_data);
  drop(mdat);

  avif.write_u32_at_marker(content_pos_marker, content_pos);

  return avif.finalize();
}

fn main() {
  let width = 64;
  let height = 64;
  let qindex = 200;

  // Generate AV1 data
  let sequence_header = generate_sequence_header(width, height);
  let frame_header = generate_frame_header(width, height, qindex, false);
  let tile_data = encode_image(width, height, qindex);

  let av1_data = pack_obus(&sequence_header, &frame_header, &tile_data, true);
  let avif_data = pack_avif(&av1_data, width, height);

  // Dump raw OBU data to one file...
  let mut obu_file = File::create("test.obu").unwrap();
  obu_file.write_all(&av1_data).unwrap();

  // ...and AVIF data to another
  let mut avif_file = File::create("test.avif").unwrap();
  avif_file.write_all(&avif_data).unwrap();
}
