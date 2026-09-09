#!/usr/bin/env python3
from pathlib import Path

from PIL import Image

SOURCE = Path("test-images/png/benchmarks/speed_bench.png")
DESTINATION = Path("/tmp/zune-profile-baseline-gray-7680x4320.jpg")

image = Image.open(SOURCE).convert("L")
image.save(
    DESTINATION,
    format="JPEG",
    quality=90,
    optimize=False,
    progressive=False,
    subsampling=0,
)
