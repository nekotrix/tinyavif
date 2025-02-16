// Copyright (c) 2024-2025, The tinyavif contributors. All rights reserved
//
// This source code is subject to the terms of the BSD 2 Clause License and
// the Alliance for Open Media Patent License 1.0. If the BSD 2 Clause License
// was not distributed with this source code in the LICENSE file, you can
// obtain it at www.aomedia.org/license/software. If the Alliance for Open
// Media Patent License 1.0 was not distributed with this source code in the
// PATENTS file, you can obtain it at www.aomedia.org/license/patent.

#![allow(dead_code)]
#![allow(unused_variables)]
#![allow(unused_imports)]
#![allow(unreachable_code)]

// Disable name styling checks, so that we can name things in line with the AV1 spec
#![allow(non_camel_case_types)]
#![allow(non_snake_case)]
#![allow(non_upper_case_globals)]

mod av1_encoder;
mod bitcode;
mod cdf;
mod entropycode;
mod hls;
mod isobmff;
mod util;

use std::io::prelude::*;
use std::fs::File;
use std::path::PathBuf;
use std::process::exit;

use crate::av1_encoder::AV1Encoder;
use crate::hls::*;

use clap::Parser;

#[derive(Parser)]
#[command(override_usage = "tinyavif [-o <OUTPUT>]")]
struct CommandlineArgs {
  /// Output file, must end in .obu or .avif [default: <input>.avif]
  #[arg(short, long)]
  output: Option<PathBuf>,
  /// Color primaries
  #[arg(long, default_value_t = 2)]
  color_primaries: u16,
  /// Transfer function
  #[arg(long, default_value_t = 2)]
  transfer_function: u16,
  /// Matrix coefficients
  #[arg(long, default_value_t = 2)]
  matrix_coefficients: u16,
}

fn main() {
  let args = CommandlineArgs::parse();

  let output_path = args.output.unwrap_or_else(|| {
    "out.avif".into()
  });

  let output_ext = match output_path.extension() {
    None => {
      println!("Error: Output file must end in .obu or .avif");
      exit(2);
    },
    Some(ext_osstr) => {
      let ext = ext_osstr.to_str().unwrap();
      if ext != "obu" && ext != "avif" {
        println!("Error: Output file must end in .obu or .avif");
        exit(2);
      }
      ext
    }
  };

  // Use fixed output size and qindex. Not configurable in this version because
  // the focus is on the AVIF file structure, not on encoding a specific image.
  let base_qindex = 255;

  let crop_width = 256;
  let crop_height = 256;

  // Generate AV1 data
  let encoder = AV1Encoder::new(crop_width, crop_height);
  let sequence_header = encoder.generate_sequence_header();
  let frame_header = encoder.generate_frame_header(base_qindex, false);
  let tile_data = encoder.encode_image(crop_width, crop_height);

  // Pack into higher-level structure and write out
  let av1_data = pack_obus(&sequence_header, &frame_header, &tile_data, true);

  match output_ext {
    "obu" => {
      // Write OBU data directly, with no further wrapping
      let mut obu_file = File::create(output_path).unwrap();
      obu_file.write_all(&av1_data).unwrap();
    },
    "avif" => {
      // Wrap OBU data in an AVIF container
      let avif_data = pack_avif(&av1_data, crop_width, crop_height,
                                args.color_primaries,
                                args.transfer_function,
                                args.matrix_coefficients);
      let mut avif_file = File::create(output_path).unwrap();
      avif_file.write_all(&avif_data).unwrap();
    },
    _ => { unreachable!() }
  }
}
