// Scan orders for 2D (ie. not H_* or V_*) transforms
// The input to this is an index in coefficient scan order,
// the output is an index (row * tx_width + col) into the quantized
// coefficient array

// 4x4
pub const default_scan_4x4: [(u8, u8); 16] = [
  (0, 0), (1, 0), (0, 1), (0, 2), (1, 1), (2, 0), (3, 0), (2, 1),
  (1, 2), (0, 3), (1, 3), (2, 2), (3, 1), (3, 2), (2, 3), (3, 3)
];

// 8x8
pub const default_scan_8x8: [(u8, u8); 64] = [
  (0, 0), (1, 0), (0, 1), (0, 2), (1, 1), (2, 0), (3, 0), (2, 1),
  (1, 2), (0, 3), (0, 4), (1, 3), (2, 2), (3, 1), (4, 0), (5, 0),
  (4, 1), (3, 2), (2, 3), (1, 4), (0, 5), (0, 6), (1, 5), (2, 4),
  (3, 3), (4, 2), (5, 1), (6, 0), (7, 0), (6, 1), (5, 2), (4, 3),
  (3, 4), (2, 5), (1, 6), (0, 7), (1, 7), (2, 6), (3, 5), (4, 4),
  (5, 3), (6, 2), (7, 1), (7, 2), (6, 3), (5, 4), (4, 5), (3, 6),
  (2, 7), (3, 7), (4, 6), (5, 5), (6, 4), (7, 3), (7, 4), (6, 5),
  (5, 6), (4, 7), (5, 7), (6, 6), (7, 5), (7, 6), (6, 7), (7, 7)
];

// Offsets of coefficients which are looked at to determine
// the context for coeff_base
// We only store the offsets for DCT_DCT for now
pub const Sig_Ref_Diff_Offset: [(u8, u8); 5] = [
  (0, 1), (1, 0), (1, 1), (0, 2), (2, 0)
];

pub const Mag_Ref_Offset: [(u8, u8); 3] = [
  (0, 1), (1, 0), (1, 1)
];

pub const Coeff_Base_Ctx_Offset_8x8: [[u8; 5]; 5] = [
  [0,  1,  6,  6,  21],
  [1,  6,  6,  21, 21],
  [6,  6,  21, 21, 21],
  [6,  21, 21, 21, 21],
  [21, 21, 21, 21, 21]
];
