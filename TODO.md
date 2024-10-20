# Pathway toward an actual encoder

* Figure out small (+/- 1 pixel value) discrepancies between encoder recon and avifdec output
  * Might be a colour space issue - need to carefully check pipeline to check this

* Figure out meaning of `hdlr` box, and stop pretending to be libavif

# Comparisons

Once the above is done:

* Grab a bunch of test images

* Encode JPEGs at various qualities

* Encode with this program at various qindex values

* Compute sensible metrics (!) and plot graphs

# Blog series

* Start a new repo and port things across in a pedagogically useful order

* Write blog posts to go along with this
