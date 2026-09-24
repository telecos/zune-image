# libjpeg-turbo parity fixtures

These 32x32 JPEGs and PPM references exercise scan organization, restart
markers, arithmetic coding, and the sampling factors accepted by zune-jpeg.

The source image is `test-images/jpeg/app14/baseline_rgb.jpg`, a CC0-1.0
fixture from `robert-ancell/jpegsuite` revision
`8382e7831896cd1adf1cc61a1c9e565c4030aa43`. The generated JPEGs and decoded
references retain that CC0-1.0 dedication.

References were generated with libjpeg-turbo 2.1.5. First extract the source:

```text
djpeg -rgb -outfile baseline_rgb.ppm ../app14/baseline_rgb.jpg
```

Generate a JPEG with the required options, then decode its reference:

```text
cjpeg -quality 90 -sample 2x2,1x1,1x1 \
  -outfile sampling_420.jpg baseline_rgb.ppm
djpeg -rgb -outfile sampling_420.ppm sampling_420.jpg
```

The JPEGs were generated from `baseline_rgb.ppm` with `cjpeg -quality 90`.
Sampling cases use:

```text
4:4:4  -sample 1x1,1x1,1x1
4:2:2  -sample 2x1,1x1,1x1
4:2:0  -sample 2x2,1x1,1x1
4:4:0  -sample 1x2,1x1,1x1
4:1:1  -sample 4x1,1x1,1x1
4:1:0  -sample 4x2,1x1,1x1
```

`noninterleaved_420.jpg` uses one sequential scan per component.
`progressive_420.jpg`, `restart_420.jpg`, `arithmetic_420.jpg`, and
`arithmetic_progressive_420.jpg` respectively add:

```text
-scans noninterleaved.scans
-progressive
-restart 1B
-arithmetic
-arithmetic -progressive
```
