# Important

This is not the main branch of tinyavif. This is a side branch
intended to accompany part 1 of the blog post series.
Everything which is not part of that post has been stripped out,
so that it is easier to understand how the code relates to what
is discussed in that post.

In particular, this version of tinyavif **cannot take an input image**.
All it can do is generate a fixed output file, which contains
a 256x256 pixel grey square. This is because the focus of the post
is to get the AVIF structure correct, not to act as a general encoder.

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

Either way, the basic usage is **stripped down to**:

    tinyavif [-o <OUTPUT>]

The output file can be either a raw AV1 stream (filename ending in `.obu`) or
an AVIF file (filename ending in `.avif`). If no filename is provided, it
defaults to `out.avif`.

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
