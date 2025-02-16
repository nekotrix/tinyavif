// Copyright (c) 2024-2025, The tinyavif contributors. All rights reserved
//
// This source code is subject to the terms of the BSD 2 Clause License and
// the Alliance for Open Media Patent License 1.0. If the BSD 2 Clause License
// was not distributed with this source code in the LICENSE file, you can
// obtain it at www.aomedia.org/license/software. If the Alliance for Open
// Media Patent License 1.0 was not distributed with this source code in the
// PATENTS file, you can obtain it at www.aomedia.org/license/patent.

pub enum Partition {
  NONE = 0,
  HORZ = 1,
  VERT = 2,
  SPLIT = 3,
  HORZ_A = 4,
  HORZ_B = 5,
  VERT_A = 6,
  VERT_B = 7,
  HORZ_4 = 8,
  VERT_4 = 9
}
