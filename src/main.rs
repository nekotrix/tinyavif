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

mod array2d;
mod av1_encoder;
mod bitcode;
mod cdf;
mod consts;
mod entropycode;
mod enums;
mod frame;
mod hls;
mod isobmff;
mod recon;
mod txfm;
mod util;
mod y4m;

use std::io::prelude::*;
use std::fs::File;
use std::path::PathBuf;
use std::process::exit;

use crate::av1_encoder::AV1Encoder;
use crate::hls::*;
use crate::y4m::Y4MReader;

use clap::Parser;

#[derive(Parser)]
#[command(override_usage = "tinyavif <INPUT> [-o <OUTPUT>] [--qindex <QINDEX>]")]
struct CommandlineArgs {
  /// Input file, must end in .y4m
  input: PathBuf,
  /// Output file, must end in .obu or .avif [default: <input>.avif]
  #[arg(short, long)]
  output: Option<PathBuf>,
  /// Quantizer to use. Valid range is 1-255, inclusive
  #[arg(short, long, default_value_t = 35)]
  qindex: u8,
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

  let input_path = args.input;

  match input_path.extension() {
    None => {
      println!("Error: Input file must end in .y4m");
      exit(2);
    },
    Some(ext_osstr) => {
      let ext = ext_osstr.to_str().unwrap();
      if ext != "y4m" {
        println!("Error: Input file must end in .y4m");
        exit(2);
      }
    }
  }

  let output_path = args.output.unwrap_or_else(|| {
    input_path.with_extension("avif")
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

  let base_qindex = args.qindex;

  let mut y4m = Y4MReader::new(File::open(input_path).unwrap()).unwrap();
  let source = y4m.read_frame().unwrap();

  // Check that the image will fit in one tile
  let crop_width = source.y().crop_width();
  let crop_height = source.y().crop_height();
  let padded_width = source.y().width();
  let padded_height = source.y().height();

  if padded_width > 4096 || padded_width * padded_height > 4096 * 2304 {
    println!("Error: image size {}x{} (padded to {}x{}) is too large to fit in a single tile",
             crop_width, crop_height, padded_width, padded_height);
    exit(2);
  }

  // Generate AV1 data
  let encoder = AV1Encoder::new(crop_width, crop_height);
  let sequence_header = encoder.generate_sequence_header();
  let frame_header = encoder.generate_frame_header(base_qindex, false);
  let tile_data = encoder.encode_image(&source, base_qindex);

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
