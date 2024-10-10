# Pathway toward an actual encoder

* Support modified partition syntax at right/bottom edges of frame
  * Remember that the image is always padded to a multiple of 8x8 pixels

* Maintain an encoder-side idea of what the reconstructed output should be,
  and dump that to a file for debugging

* Implement DC_PRED logic - this is enough for all predictions currently

* Implement 8x8 forward and inverse transforms - this is enough for luma

* Hook up transform pipeline
  load source => subtract pred => txfm => quantize => inverse txfm => add to pred => store to recon

* Hook up proper coefficient syntax for luma

* Then do the same for chroma
  * Requires implementing 4x4 forward and inverse transforms

* Select transform-related CDFs based on qindex
  * Move CDF definitions to a separate file

# Comparisons

Once the above is done:

* Grab a bunch of test images

* Encode JPEGs at various qualities

* Encode with this program at various qindex values

* Compute sensible metrics (!) and plot graphs

# Blog series

* Start a new repo and port things across in a pedagogically useful order

* Write blog posts to go along with this
