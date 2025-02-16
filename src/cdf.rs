// Copyright (c) 2024-2025, The tinyavif contributors. All rights reserved
//
// This source code is subject to the terms of the BSD 2 Clause License and
// the Alliance for Open Media Patent License 1.0. If the BSD 2 Clause License
// was not distributed with this source code in the LICENSE file, you can
// obtain it at www.aomedia.org/license/software. If the Alliance for Open
// Media Patent License 1.0 was not distributed with this source code in the
// PATENTS file, you can obtain it at www.aomedia.org/license/patent.

// All of the CDFs used in the encoder currently

// Partitions
// Options are NONE, HORT, VERT, SPLIT, HORZ_A, HORZ_B, VERT_A, VERT_B, HORZ_4, VERT_4
// In this version of the encoder, we only ever use context 0 for 64x64 partitions
pub const partition_64x64_cdf: [u16; 9] = [
  20137, 21547, 23078, 29566, 29837, 30261, 30524, 30892, 31724
];

// Block mode syntax
// This encoder arranges things so that these only ever use one context each,
// so just store the single relevant CDF
pub const skip_cdf: [[u16; 1]; 3] = [[31671], [16515], [4576]];
pub const y_mode_cdf: [u16; 12] = [15588, 17027, 19338, 20218, 20682, 21110, 21825, 23244, 24189, 28165, 29093, 30466];
pub const uv_mode_cdf: [u16; 12] = [22631, 24152, 25378, 25661, 25986, 26520, 27055, 27923, 28244, 30059, 30941, 31961];
