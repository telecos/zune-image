# zune-jpeg Phase Profile at Z5

## Executive conclusion

This investigation profiles `zune-jpeg` at exact revision
`aede1e43cdd650fc12540d6334197977e2401109` without implementing a production
optimization.

The baseline result is mixed rather than a single-function bottleneck. On the
large 4:2:0 packed-BGRA workload, the active phase profile attributes about
28.2% to Huffman entropy, 21.7% to sparse IDCT, 24.2% to packed conversion,
7.6% to upsampling, 4.8% to dequantization, and 8.7% to coefficient clearing,
looping, and dispatch. Raw output removes about 30% of end-to-end time. This
supports an independently reviewable merged 4:2:0/4:2:2 upsampling and packed
conversion PR before a generic entropy rewrite.

Progressive is qualitatively different. Entropy scans consume about 63-65% of
packed output time. In 4:2:0, the two Y AC-refinement scans take approximately
33.8 ms and 48.6 ms, about 36% of total decode by themselves. Final dequant,
IDCT, upsampling, and conversion are material but shared reconstruction is not
the dominant progressive deficit. Progressive AC refinement is the recommended
first optimization PR.

The prior hypothesis that baseline is dominated only by SIMD quality is partly
falsified: runtime `set_use_unsafe(false)` changes little on this host because
sparse-IDCT classes dominate and the benchmark's SIMD/scalar distinction does
not isolate every kernel. The hypothesis that progressive AC refinement is
material is confirmed. The hypothesis that progressive output modes are cheap
wrappers is falsified: resumable scanline/raw operation copies 663-929 MB of
coefficient data per large decode.

Z5 should remain separate from Z4. Z5 changes baseline pull bookkeeping, has no
API change, and is independently reviewable from Z4's BGR/BGRA SIMD kernels.

## Provenance

- Source branch: `investigation/jpeg-phase-profile`
- Profiled production revision (branch base):
  `aede1e43cdd650fc12540d6334197977e2401109`
- Parent Z4: `3885e73828f095a68ae98c99b3b771ea64abbcfb`
- Profiler binary SHA256:
  `51545689d4dff6a310cc120ce3e29d877380fe62f2528e4a98c380b896fcc966`
- The profiling branch adds only feature-gated instrumentation, benchmark
  source, raw results, and this report. The worktree is clean at handoff. No
  production optimization was added.
- Host: AMD EPYC 7763 under Microsoft full virtualization, Linux
  `6.17.0-1022-azure`
- Allowed CPUs: 0-7; runs pinned internally to CPU 0. The requested CPU 10 was
  unavailable.
- Rust: 1.89.0, LLVM 20.1.7
- Bench profile: opt-level 3, 16 codegen units, no LTO, debug info enabled,
  debug assertions disabled, generic x86-64 target with runtime AVX2 dispatch.
- `perf_event_paranoid=4`; perf events were unavailable. Valgrind, Callgrind,
  and Samply were unavailable.
- The two supplied Chromium JSON paths no longer existed. Their reported
  aggregate values are retained as external controls but were not reparsed.

Complete environment and build settings are in `build-environment.txt`.
Fixture hashes and properties are in `fixture-manifest.csv`.

## Method

End-to-end time uses `Instant`. Phases use serialized x86 TSC ticks. Exact outer
timers cover baseline MCU production, progressive scans, progressive final
transform, post-processing, raw copying, and checkpoint creation. Inner
entropy/dequant/IDCT proportions use one deterministic multiplicative sample
classification per block, with disjoint entropy+IDCT and dequant buckets at
approximately 1/256 each. No clock is called per pixel or coefficient.

Phase estimates are bounded by exact outer totals. If sampled inner estimates
exceed their exact transform envelope, they are normalized proportionally;
this is recorded per sample in `analysis.json`. Unassigned exact time remains
visible as traversal/dispatch or orchestration instead of being hidden.

The active profiler overhead target was not met. Same-binary enabled versus
disabled overhead was 7.3% for SIMD and 7.0% for forced scalar in the final
summary, with host-load instability in that run. Exact outer phase percentages
remain useful; counters-disabled data supplies end-to-end medians. Results
should be confirmed with hardware sampling on the Chromium CPU-10 host before
an optimization is landed.

## End-to-end results

Counters-disabled medians are shown in milliseconds. RSD is in parentheses.
SIMD and forced-scalar output were measured in alternating order. Some baseline
groups remained bimodal under virtualization despite isolated reruns; every raw
sample is retained in `end-to-end-summary.csv`.

| Workload | Packed BGRA | Converted scanlines | Raw components |
|---|---:|---:|---:|
| Baseline 4:4:4 | 92.92 (4.23%) | 90.73 (6.76%) | 82.20 (0.43%) |
| Baseline 4:2:2 | 79.86 (3.05%) | 77.58 (6.42%) | 64.77 (0.66%) |
| Baseline 4:4:0 | 82.69 (2.08%) | 78.54 (0.56%) | 65.65 (4.72%) |
| Baseline 4:2:0 | 72.95 (4.02%) | 68.56 (5.61%) | 52.00 (1.57%) |
| Progressive 4:4:4 | 279.94 (1.32%) | 331.06 (1.98%) | 324.28 (2.33%) |
| Progressive 4:2:2 | 233.09 (2.58%) | 309.84 (1.93%) | 296.04 (2.62%) |
| Progressive 4:4:0 | 230.40 (1.18%) | 303.76 (3.40%) | 292.34 (2.35%) |
| Progressive 4:2:0 | 230.93 (4.25%) | 305.62 (2.48%) | 289.84 (1.91%) |

Forced-scalar medians are within roughly 1% of zune SIMD for these end-to-end
workloads. This does not mean every kernel is scalar-equivalent; it means the
current option and workload do not expose a large aggregate AVX2 advantage.

The supplied Chromium controls remain the relevant external comparison:

| Chromium large workload | Time |
|---|---:|
| libjpeg-turbo SIMD baseline packed | 15.61 ms |
| zune Z5 baseline packed | 25.15 ms |
| libjpeg-turbo forced-scalar baseline packed | 37.67 ms |
| libjpeg-turbo SIMD progressive packed | 46.11 ms |
| libjpeg-turbo forced-scalar progressive packed | 68.37 ms |
| zune Z5 progressive packed | 97.77 ms |
| libjpeg-turbo SIMD progressive raw | 46.36 ms |
| zune Z5 progressive raw | 104.41 ms |

The locally packaged `mozjpeg` benchmark did not respond materially to
`JSIMD_FORCENONE=1`, so its SIMD/scalar ratio is not used as evidence. The
Chromium control should be rerun where libjpeg-turbo SIMD dispatch can be
verified.

## Packed SIMD phase tables

Percent of active profiled decode time. Rows reconcile to approximately 100%;
small differences are median-of-phase aggregation and exact residuals.

| Workload | Entropy scans | Baseline Huffman | Dequant | IDCT | Traversal/dispatch | Upsample | Convert | Copy/other | Orchestration |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| Baseline 4:4:4 | 0.0 | 30.9 | 4.7 | 28.7 | 15.8 | 0.0 | 18.6 | 0.2 | 0.2 |
| Baseline 4:2:2 | 0.0 | 29.6 | 4.9 | 23.6 | 15.0 | 4.2 | 21.7 | 0.2 | 0.2 |
| Baseline 4:4:0 | 0.0 | 28.2 | 4.8 | 22.8 | 16.6 | 5.3 | 21.2 | 0.8 | 0.2 |
| Baseline 4:2:0 | 0.0 | 28.2 | 4.8 | 21.7 | 8.7 | 7.6 | 24.2 | 1.0 | 0.2 |
| Progressive 4:4:4 | 64.6 | 0.0 | 8.7 | 9.5 | 3.6 | 0.0 | 6.7 | 0.1 | 6.5 |
| Progressive 4:2:2 | 64.6 | 0.0 | 7.3 | 10.5 | 2.3 | 1.5 | 8.2 | 0.2 | 5.5 |
| Progressive 4:4:0 | 63.0 | 0.0 | 7.7 | 11.5 | 0.1 | 2.0 | 8.2 | 0.4 | 5.6 |
| Progressive 4:2:0 | 63.3 | 0.0 | 7.5 | 11.4 | 1.6 | 2.0 | 8.2 | 0.4 | 5.6 |

### Baseline 4:2:0 raw output

| Phase | Median share |
|---|---:|
| Huffman entropy | about 39% |
| Sparse IDCT total | about 29% |
| Coefficient clear/dispatch | about 17% |
| Dequantization | about 7% |
| Raw plane copy | about 4% |
| Pull/session orchestration | about 4% |

Raw output removes upsampling and packed conversion but does not eliminate the
entropy/IDCT/intermediate-plane work. This agrees with the supplied Chromium
observation that zune raw tracks scalar libjpeg-turbo rather than SIMD
libjpeg-turbo.

## Progressive 4:2:0 scan breakdown

| Scan | Kind | Component | Median |
|---:|---|---:|---:|
| 0 | DC first | interleaved | 21.80 ms |
| 1 | AC first | Y | 2.05 ms |
| 2 | AC first | Cr | 1.03 ms |
| 3 | AC first | Cb | 1.03 ms |
| 4 | AC first | Y | 1.84 ms |
| 5 | AC refine | Y | 33.84 ms |
| 6 | DC refine | interleaved | 22.15 ms |
| 7 | AC refine | Cr | 6.93 ms |
| 8 | AC refine | Cb | 5.07 ms |
| 9 | AC refine | Y | 48.59 ms |

The two Y AC-refinement scans total about 82.4 ms and are the dominant
progressive owner. Across sampling modes, all progressive entropy scans account
for 63-65% of packed decode.

## Shared reconstruction costs

Baseline and progressive share full-IDCT dispatch, upsampling, color conversion,
and final output writes. They do not share the dominant entropy/coefficient
path: baseline performs Huffman decode, dequantization, sparse classification,
and IDCT per block; progressive stores coefficients over ten scans and later
runs full IDCT for every reconstructed block.

For large 4:2:0:

- Baseline: 777,600 IDCT calls; 48.68% 1x1, 45.94% 4x4, 5.38% full 8x8.
- Progressive: 1,036,800 full 8x8 IDCT calls.
- Baseline `raw_coeff` writes: 99.5 MB.
- Progressive final `raw_coeff` writes: 132.7 MB.
- Upsample destination writes: 165.9 MB.
- Packed conversion input reads: 199.1 MB.
- Packed BGRA output writes: 132.7 MB.

The sparse-IDCT distribution explains why simply optimizing full 8x8 baseline
IDCT cannot close the baseline gap. Progressive final reconstruction does make
full IDCT material, but it remains smaller than scan parsing/refinement.

## Output and preservation traffic

One-shot progressive packed decode uses no scan-preservation copies. Pull-based
progressive 4:2:0 scanline/raw output records approximately 663.6 MB of
coefficient-copy traffic, 265 MB of initialization traffic, and 43 checkpoint
creates. Progressive 4:4:4 pull output reaches roughly 929 MB of copy traffic.
This explains why progressive scanline/raw modes are 17-33% slower than packed
one-shot output in the local matrix.

Restart controls behave as expected:

- Baseline four-component raw pull: 76 restart markers, 154 checkpoint creates,
  76 restores.
- Progressive restart raw pull: 214 restart markers and 12 progressive
  preservation checkpoints.
- Non-interleaved baseline raw pull: 41 pull calls and 160 checkpoints.

No decoded output or error behavior changed in profile-enabled parity tests.

## Packed color conversion

Large baseline 4:2:0 conversion time is similar across requested packed orders:

| Output | SIMD total | SIMD conversion | Conversion share |
|---|---:|---:|---:|
| RGB | about 78 ms | about 18.7 ms | about 24% |
| BGR | about 76 ms | about 17.8 ms | about 23% |
| RGBA | about 77 ms | about 18.8 ms | about 25% |
| BGRA | about 77 ms | about 18.5 ms | about 24% |

Z4 therefore achieved channel-order parity, but conversion plus intermediate
traffic remains material. Conversion alone is not enough evidence for another
standalone packer PR; the stronger boundary is merged sampling/conversion.

## Scaling

All main fixtures contain the same 33.18 million pixels, so pixel-count
correlation is undefined within this matrix. Baseline time follows decoded
block count and sampling work. Progressive packed time follows scan count and
per-scan MCU/coefficient traversal much more strongly; the analysis records a
high scan-count correlation, while MCU/block count alone is weak across mixed
baseline/progressive workloads.

Expected scaling owners:

- Huffman and sparse IDCT: decoded block count and coefficient density.
- Baseline dequantization: nonzero coefficient count.
- Upsampling/conversion: output pixels and chroma sampling.
- Progressive scan parsing: scan count, component dimensions, EOB runs, and
  refinement nonzeros.
- Progressive preservation: touched coefficient-buffer bytes per retry/pull
  sequence.
- Checkpoints/cancellation: MCU rows, restart interval, and pull count.

## Memory

The full active matrix peaked at 395,740 KiB RSS; counters-disabled peaked at
395,696 KiB. This includes benchmark output buffers and process state, not only
decoder allocations.

Existing allocation benchmarks at Z5 report:

| Mode | Peak bytes | Allocations |
|---|---:|---:|
| Full output | 100,867,168 | 33 |
| One-row scanlines | 1,772,360 | 35 |
| 64-row scanlines | 3,223,880 | 35 |

Profile counters additionally record capacity growth, initialization bytes,
progressive copy bytes, raw copy bytes, and intermediate traffic per sample in
`counter-summary.csv`.

## Correctness and tooling

Passed during instrumentation development:

- Default, profile-active, arithmetic, no-default/no-std, and AArch64 NEON
  compile checks.
- 118 targeted incremental, malformed/truncated, raw, exact-length, scanline,
  cancellation, progressive preview/checkpoint, and restart tests.
- Scalar/AVX2 output parity through existing scanline and raw suites.
- AArch64 profile-active NEON compile check; Arm64 runtime was unavailable.
- `git diff --check`.

The final validation section should be read with two repository caveats:
workspace formatting and strict all-feature clippy have unrelated pre-existing
failures on the source revision. The profiling files were formatted with the
installed nightly rustfmt. No production API is enabled by default; profiling
requires the non-default `profile-active` feature.

## Ranked bottlenecks and falsifiable hypotheses

1. **Progressive Y AC refinement**
  - Evidence: two Y scans total about 82.4 ms; all scans are 63-65% of packed
     progressive decode.
   - Hypothesis: reducing repeated coefficient-buffer traversal and branch work
     in `decode_mcu_ac_refine` by 25% reduces large progressive packed time by
     at least 10%, with checkpoint previews byte-identical.

2. **Baseline sampled output pipeline**
   - Evidence: 4:2:0 upsampling plus conversion is about 31.5%; 165.9 MB of
     intermediate writes are reread as 199.1 MB of conversion input.
   - Hypothesis: a fused 4:2:0/4:2:2 upsample-and-pack path removing the full
     i16 upsample destination reduces large baseline packed time by at least
     15%, while raw output and progressive entropy remain unchanged.

3. **Baseline Huffman plus sparse IDCT**
   - Evidence: about 27% entropy, 21% IDCT, 5% dequant, with 94.6% of blocks
     selecting 1x1 or 4x4 IDCT.
   - Hypothesis: batching sparse IDCT dispatch across an MCU row or specializing
     common 4:2:0 component order reduces baseline raw time by at least 8%.
     A generic full-IDCT-only optimization should fail this gate.

4. **Progressive coefficient preservation for pull APIs**
   - Evidence: 663-929 MB copied and 43-77 checkpoints for large pull output;
     pull modes are 17-33% slower than packed one-shot.
   - Hypothesis: persistent touched-component scratch with commit metadata can
     reduce copy traffic by at least 50% and pull time by at least 15% without
     exposing partial scans.

## Recommended review order

1. **Progressive AC-refinement hot loop**
   - First because it owns the largest measured phase and is isolated from
     final reconstruction and public APIs.

2. **Fused 4:2:0/4:2:2 upsample plus packed conversion**
   - Independently reviewable as an output-stage specialization. Keep scalar,
     AVX2, and NEON implementations in separate commits or PRs if review size
     warrants it; do not mix progressive entropy changes.

3. **Progressive preservation-copy reduction**
   - Separate because it changes retry/pull state ownership and requires the
     checkpoint/preview test matrix, while one-shot progressive behavior should
     remain unchanged.

4. **Baseline sparse-IDCT/MCU batching**
   - Separate low-level transform/dispatch work. Gate it on baseline raw and
     packed improvements; full 8x8 IDCT alone is not justified by this profile.

No API change is indicated by the measurements. Skia CL 1352756 should continue
using the current raw and scanline contracts.

## Raw outputs

- `main/samples.csv`: active phase samples.
- `main/scans.csv`: per-progressive-scan samples.
- `main-disabled/samples.csv`: counters-disabled end-to-end samples.
- `end-to-end-summary.csv`: medians, all samples, and RSD.
- `phase-summary.csv`: median phase time and percentage.
- `sample-phases.csv`: every per-sample reconciled phase.
- `scan-summary.csv`: scan index/type/component summaries.
- `counter-summary.csv`: invocation, traffic, checkpoint, and capacity counters.
- `overhead-summary.csv`: active versus disabled instrumentation overhead.
- `analysis.json`: assumptions, normalization use, unstable groups, correlations,
  and largest AC-refinement scans.
- `libjpeg-control/`: local mozjpeg normal/forced-none logs; retained but not
  used as SIMD evidence.
- `fixture-manifest.csv`: exact fixture hashes.
- `build-environment.txt`: hardware, toolchain, and build settings.
- `generate_grayscale_fixture.py`: deterministic generated-fixture recipe.
