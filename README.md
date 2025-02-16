# Tinyavif - the world's most minimal AV1 encoder

Tinyavif is a proof-of-concept project to test how simple an AV1 encoder can be
while still generating spec-compliant, AVIF-format output.

Disclaimer: This is not intended as a serious production encoder, and should
not be relied upon as such. However, if you wish to use this project as a
starting point to further explore AV1 encoding, please do! We'd love to hear
about it!

For more details, please see
[the accompanying blog post series](https://www.rachelplusplus.me.uk/blog/2025/01/lets-build-an-avif-encoder-part-0/).

## Usage

As a proof of concept, tinyavif does not have any formal packaging. Therefore,
to build, download the repository and run `cargo build --release`. This
generates a statically-linked executable `target/release/tinyavif`.

Then to run, either copy this executable somewhere and run `tinyavif ARGS...`,
or combine the two steps as `cargo run release -- ARGS...` (the `--` is
required).

Either way, the basic usage is:

    tinyavif <INPUT> [-o <OUTPUT>] [--qindex <QINDEX>]

The input file must be in the Y4M format, and must use 8 bits per pixel with
4:2:0 downsampling (`yuv420p` format if using `ffmpeg` for conversion).

The output file can be either a raw AV1 stream (filename ending in `.obu`) or
an AVIF file (filename ending in `.avif`).

`qindex` acts as the quality setting, and ranges from 1 (near-lossless) to 255
(extremely low quality). The default is 35, which should be a decent starting
point for high-quality encodes.

If coming from other AV1 encoders which expect a `qp` value, start from
`qindex = 4 * qp` and adjust from there.

## Colour spaces

Tinyavif does not read colour space information from its input yet. By default
it sets all output colour space parameters to "unspecified"; other tools will
then make a best guess at the appropriate parameters.

If you want to set specific parameters, these can be set using the
`--color-primaries`, `--transfer-function`, and `--matrix-coefficients`
arguments. Each argument takes a numerical index; see the excellent
[Codec Wiki](https://wiki.x266.mov/docs/colorimetry/primaries) pages on
colorimetry for what these correspond to.

# License

The source code for tinyavif is distributed under the BSD 2-clause license.
In addition, tinyavif is subject to the AOMedia Patent License 1.0.
