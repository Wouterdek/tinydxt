# tinydxt

Small rust app to (de-)compress grayscale pngs using DXT1 texture compression.
Blocks of 4x4 1-byte pixels are converted into 6 bytes (2 bytes of values + 4 bytes of codes), so a fixed 37.5% compression rate applies.
