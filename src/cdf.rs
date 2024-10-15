// All of the CDFs used in the encoder currently

// Partitions
// For 8x8, the options are NONE, HORZ, VERT, SPLIT only;
// for larger sizes, T-shaped and 4-way partitions are also available
// (HORZ_A, HORZ_B, VERT_A, VERT_B, HORZ_4, VERT_4)

// We only ever use one context for 8x8 partitions, so don't
// bother including the other three
pub const partition_8x8_cdf: [u16; 3] = [19132, 25510, 30392];

pub const partition_16x16_cdf: [[u16; 9]; 4] = [
  [15597, 20929, 24571, 26706, 27664, 28821, 29601, 30571, 31902],
  [7925, 11043, 16785, 22470, 23971, 25043, 26651, 28701, 29834],
  [5414, 13269, 15111, 20488, 22360, 24500, 25537, 26336, 32117],
  [2662, 6362, 8614, 20860, 23053, 24778, 26436, 27829, 31171]
];

pub const partition_32x32_cdf: [[u16; 9]; 4] = [
  [18462, 20920, 23124, 27647, 28227, 29049, 29519, 30178, 31544],
  [7689, 9060, 12056, 24992, 25660, 26182, 26951, 28041, 29052],
  [6015, 9009, 10062, 24544, 25409, 26545, 27071, 27526, 32047],
  [1394, 2208, 2796, 28614, 29061, 29466, 29840, 30185, 31899]
];

pub const partition_64x64_cdf: [[u16; 9]; 4] = [
  [20137, 21547, 23078, 29566, 29837, 30261, 30524, 30892, 31724],
  [6732, 7490, 9497, 27944, 28250, 28515, 28969, 29630, 30104],
  [5945, 7663, 8348, 28683, 29117, 29749, 30064, 30298, 32238],
  [870, 1212, 1487, 31198, 31394, 31574, 31743, 31881, 32332]
];

// Block mode syntax
// This encoder arranges things so that these only ever use one context each,
// so just store the single relevant CDF
pub const skip_cdf: [u16; 1] = [31671];
pub const y_mode_cdf: [u16; 12] = [15588, 17027, 19338, 20218, 20682, 21110, 21825, 23244, 24189, 28165, 29093, 30466];
pub const uv_mode_cdf: [u16; 13] = [10407, 11208, 12900, 13181, 13823, 14175, 14899, 15656, 15986, 20086, 20995, 22455, 24212];

// Residual syntax
// These CDFs all have complex contexts, some of which are fixed in our case
// and some of which are not. They also all depend on the qindex via the qctx value.
