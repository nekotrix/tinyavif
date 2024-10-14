# Pathway toward an actual encoder

* Figure out small (+/- 1 pixel value) discrepancies between encoder recon and avifdec output

* Hook up chroma transforms + coefficient encoding
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
