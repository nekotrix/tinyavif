use crate::isobmff::ISOBMFFWriter;
use crate::util::write_leb128;

pub fn pack_obus(sequence_header: &[u8], frame_header: &[u8], tile_data: &[u8], include_temporal_delimiter: bool) -> Box<[u8]> {
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

pub fn pack_avif(av1_data: &[u8], crop_width: usize, crop_height: usize,
                 color_primaries: u16,
                 transfer_function: u16,
                 matrix_coefficients: u16) -> Box<[u8]> {
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
        ispe.write_u32(crop_width as u32);
        ispe.write_u32(crop_height as u32);
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
        colr.write_u16(color_primaries);
        colr.write_u16(transfer_function);
        colr.write_u16(matrix_coefficients);
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
