#!/usr/bin/env python3
import csv
import hashlib
import json
import math
import statistics
import sys
from collections import defaultdict
from pathlib import Path

SCRIPT_ROOT = Path(__file__).resolve().parent
ROOT = Path(sys.argv[1]).resolve() if len(sys.argv) > 1 else SCRIPT_ROOT / "jpeg-phase-profile-2026-09-09"
DATASETS = [
    "main",
    "main-disabled",
    "color-rgb",
    "color-bgr",
    "color-rgba",
    "grayscale-packed-bgra",
    "grayscale-scanlines-bgra",
    "grayscale-raw",
    "restart-progressive-packed-bgra",
    "restart-progressive-scanlines-bgra",
    "restart-progressive-raw",
    "restart-baseline-packed-bgra",
    "restart-baseline-scanlines-bgra",
    "restart-baseline-raw",
    "noninterleaved-packed-bgra",
    "noninterleaved-scanlines-bgra",
    "noninterleaved-raw",
]
COUNT_FIELDS = [
    "width",
    "height",
    "components",
    "mcu_rows",
    "mcus",
    "idct_1x1_calls",
    "idct_4x4_calls",
    "idct_8x8_calls",
    "entropy_samples",
    "sampled_nonzero_coefficients",
    "dequant_samples",
    "upsample_none_calls",
    "upsample_horizontal_calls",
    "upsample_vertical_calls",
    "upsample_hv_calls",
    "color_convert_calls",
    "checkpoint_creates",
    "checkpoint_restores",
    "restart_markers",
    "cancellation_polls",
    "raw_coeff_bytes",
    "raw_output_bytes",
    "upsample_scratch_bytes",
    "upsample_destination_bytes",
    "color_input_bytes",
    "packed_output_bytes",
    "progressive_init_bytes",
    "progressive_copy_bytes",
    "capacity_growths",
    "idct_pointer_calls",
    "upsample_pointer_calls",
    "color_pointer_calls",
    "pull_calls",
    "scan_count",
]


def number(row, key):
    value = row.get(key, "0")
    return float(value) if value else 0.0


def median(values):
    return statistics.median(values) if values else 0.0


def mean(values):
    return statistics.fmean(values) if values else 0.0


def stdev(values):
    return statistics.stdev(values) if len(values) > 1 else 0.0


def rsd(values):
    average = mean(values)
    return stdev(values) / average * 100.0 if average else 0.0


def geometric_mean(values):
    positive = [value for value in values if value > 0]
    return math.exp(mean([math.log(value) for value in positive])) if positive else 0.0


def pearson(xs, ys):
    if len(xs) < 2 or len(set(xs)) < 2 or len(set(ys)) < 2:
        return None
    x_mean = mean(xs)
    y_mean = mean(ys)
    numerator = sum((x - x_mean) * (y - y_mean) for x, y in zip(xs, ys))
    denominator = math.sqrt(
        sum((x - x_mean) ** 2 for x in xs) * sum((y - y_mean) ** 2 for y in ys)
    )
    return numerator / denominator if denominator else None


def sample_key(dataset, row):
    return (dataset, row["sample_id"])


def group_key(dataset, row):
    return (dataset, row["workload"], row["fixture"], row["mode"], row["variant"])


def tick_ms(row, ticks):
    total_ticks = number(row, "profile_total_ticks")
    total_nanos = number(row, "profile_total_nanos")
    return ticks * total_nanos / total_ticks / 1_000_000.0 if total_ticks else 0.0


def sampled_estimate(row, prefix, calls):
    samples = number(row, f"{prefix}_samples")
    ticks = number(row, f"{prefix}_ticks")
    return ticks / samples * calls if samples else 0.0


def normalized_parts(exact, weights):
    total = sum(max(value, 0.0) for value in weights.values())
    if exact <= 0 or total <= 0:
        return {name: 0.0 for name in weights}, exact, False
    if total <= exact:
        return {name: max(value, 0.0) for name, value in weights.items()}, exact - total, False
    scale = exact / total
    return {name: max(value, 0.0) * scale for name, value in weights.items()}, 0.0, True


def baseline_phases(row):
    total = number(row, "profile_total_ticks")
    blocks = number(row, "idct_pointer_calls")
    entropy = sampled_estimate(row, "entropy", blocks)
    entropy_samples = number(row, "entropy_samples")
    nonzero_per_block = (
        number(row, "sampled_nonzero_coefficients") / entropy_samples
        if entropy_samples
        else 0.0
    )
    estimated_nonzero = nonzero_per_block * blocks
    dequant_samples = number(row, "dequant_samples")
    dequant = (
        number(row, "dequant_ticks") / dequant_samples * estimated_nonzero
        if dequant_samples
        else 0.0
    )
    weights = {
        "huffman_entropy": entropy,
        "coefficient_dequant": dequant,
        "idct_1x1": sampled_estimate(row, "idct_1x1", number(row, "idct_1x1_calls")),
        "idct_4x4": sampled_estimate(row, "idct_4x4", number(row, "idct_4x4_calls")),
        "idct_8x8": sampled_estimate(row, "idct_8x8", number(row, "idct_8x8_calls")),
    }
    mcu = number(row, "baseline_mcu_ticks")
    parts, mcu_other, normalized = normalized_parts(mcu, weights)
    parts["coefficient_clear_dispatch"] = mcu_other

    post = number(row, "post_process_ticks")
    upsample = min(number(row, "upsample_ticks"), post)
    color = min(number(row, "color_convert_ticks"), max(post - upsample, 0.0))
    parts["upsampling"] = upsample
    parts["packed_conversion"] = color
    parts["post_output_copy"] = max(post - upsample - color, 0.0)

    raw_copy = number(row, "raw_copy_ticks")
    outside = max(total - mcu - post, 0.0)
    parts["raw_plane_copy"] = min(raw_copy, outside)
    outside -= parts["raw_plane_copy"]
    checkpoint = min(number(row, "checkpoint_ticks"), outside)
    parts["checkpoint"] = checkpoint
    parts["session_orchestration"] = max(outside - checkpoint, 0.0)
    residual = total - sum(parts.values())
    return parts, residual, normalized


def progressive_phases(row, scans):
    total = number(row, "profile_total_ticks")
    scan_groups = defaultdict(float)
    for scan in scans:
        scan_groups[f"scan_{scan['kind'].lower()}_c{scan['component']}"] += float(scan["ticks"])
    parts = dict(scan_groups)
    scan_sum = sum(scan_groups.values())

    transform = number(row, "progressive_transform_ticks")
    weights = {
        "final_dequant": sampled_estimate(row, "dequant", number(row, "idct_pointer_calls")),
        "final_idct_8x8": sampled_estimate(
            row, "idct_8x8", number(row, "idct_8x8_calls")
        ),
    }
    transform_parts, transform_other, normalized = normalized_parts(transform, weights)
    parts.update(transform_parts)
    parts["final_transform_traversal"] = transform_other

    reconstruct = number(row, "progressive_reconstruct_ticks")
    post = number(row, "post_process_ticks")
    upsample = min(number(row, "upsample_ticks"), post)
    color = min(number(row, "color_convert_ticks"), max(post - upsample, 0.0))
    parts["upsampling"] = upsample
    parts["packed_conversion"] = color
    parts["post_output_copy"] = max(post - upsample - color, 0.0)
    raw_copy = number(row, "raw_copy_ticks")
    parts["raw_plane_copy"] = raw_copy
    accounted_reconstruct = transform + post + raw_copy
    parts["reconstruction_other"] = max(reconstruct - accounted_reconstruct, 0.0)
    parts["session_orchestration"] = max(total - scan_sum - reconstruct, 0.0)
    residual = total - sum(parts.values())
    return parts, residual, normalized


def load_data():
    samples = []
    scans_by_sample = defaultdict(list)
    datasets = DATASETS + sorted(path.name for path in ROOT.glob("rerun-*") if path.is_dir())
    for dataset in datasets:
        directory = ROOT / dataset
        sample_path = directory / "samples.csv"
        scan_path = directory / "scans.csv"
        if not sample_path.exists():
            continue
        with sample_path.open(newline="") as handle:
            for row in csv.DictReader(handle):
                row["dataset"] = dataset
                samples.append(row)
        if scan_path.exists():
            with scan_path.open(newline="") as handle:
                for row in csv.DictReader(handle):
                    scans_by_sample[(dataset, row["sample_id"])].append(row)
    return samples, scans_by_sample


def write_csv(path, fieldnames, rows):
    with path.open("w", newline="") as handle:
        writer = csv.DictWriter(handle, fieldnames=fieldnames)
        writer.writeheader()
        writer.writerows(rows)


def main():
    samples, scans_by_sample = load_data()
    groups = defaultdict(list)
    for row in samples:
        groups[group_key(row["dataset"], row)].append(row)

    end_rows = []
    count_rows = []
    phase_values = defaultdict(lambda: defaultdict(list))
    normalized_groups = defaultdict(int)
    residual_values = defaultdict(list)
    sample_phase_rows = []

    for key, rows in sorted(groups.items()):
        dataset, workload, fixture, mode, variant = key
        elapsed = [number(row, "elapsed_nanos") / 1_000_000.0 for row in rows]
        end_rows.append(
            {
                "dataset": dataset,
                "workload": workload,
                "fixture": fixture,
                "mode": mode,
                "variant": variant,
                "samples": len(rows),
                "median_ms": f"{median(elapsed):.6f}",
                "mean_ms": f"{mean(elapsed):.6f}",
                "stdev_ms": f"{stdev(elapsed):.6f}",
                "rsd_pct": f"{rsd(elapsed):.4f}",
                "all_samples_ms": ";".join(f"{value:.6f}" for value in elapsed),
            }
        )
        count_row = {
            "dataset": dataset,
            "workload": workload,
            "fixture": fixture,
            "mode": mode,
            "variant": variant,
        }
        for field in COUNT_FIELDS:
            count_row[field] = f"{median([number(row, field) for row in rows]):.3f}"
        count_rows.append(count_row)

        for row in rows:
            if number(row, "baseline_mcu_ticks") > 0:
                phases, residual, normalized = baseline_phases(row)
            elif number(row, "scan_count") > 0:
                phases, residual, normalized = progressive_phases(
                    row, scans_by_sample[sample_key(dataset, row)]
                )
            else:
                continue
            total_ticks = number(row, "profile_total_ticks")
            if normalized:
                normalized_groups[key] += 1
            residual_values[key].append(tick_ms(row, residual))
            for phase, ticks in phases.items():
                milliseconds = tick_ms(row, ticks)
                percentage = ticks / total_ticks * 100.0 if total_ticks else 0.0
                phase_values[key][phase].append((milliseconds, percentage))
                sample_phase_rows.append(
                    {
                        "dataset": dataset,
                        "sample_id": row["sample_id"],
                        "workload": workload,
                        "fixture": fixture,
                        "mode": mode,
                        "variant": variant,
                        "phase": phase,
                        "milliseconds": f"{milliseconds:.9f}",
                        "percent": f"{percentage:.6f}",
                    }
                )

    phase_rows = []
    for key, phases in sorted(phase_values.items()):
        dataset, workload, fixture, mode, variant = key
        for phase, values in sorted(phases.items()):
            milliseconds = [value[0] for value in values]
            percentages = [value[1] for value in values]
            phase_rows.append(
                {
                    "dataset": dataset,
                    "workload": workload,
                    "fixture": fixture,
                    "mode": mode,
                    "variant": variant,
                    "phase": phase,
                    "median_ms": f"{median(milliseconds):.6f}",
                    "median_pct": f"{median(percentages):.4f}",
                    "rsd_pct": f"{rsd(milliseconds):.4f}",
                }
            )

    scan_groups = defaultdict(list)
    sample_lookup = {sample_key(row["dataset"], row): row for row in samples}
    for key, scans in scans_by_sample.items():
        sample = sample_lookup[key]
        for scan in scans:
            scan_key = (
                sample["dataset"],
                sample["workload"],
                sample["fixture"],
                sample["mode"],
                sample["variant"],
                scan["scan_index"],
                scan["kind"],
                scan["component"],
            )
            scan_groups[scan_key].append(tick_ms(sample, float(scan["ticks"])))
    scan_rows = []
    for key, values in sorted(scan_groups.items()):
        dataset, workload, fixture, mode, variant, index, kind, component = key
        scan_rows.append(
            {
                "dataset": dataset,
                "workload": workload,
                "fixture": fixture,
                "mode": mode,
                "variant": variant,
                "scan_index": index,
                "kind": kind,
                "component": component,
                "median_ms": f"{median(values):.6f}",
                "rsd_pct": f"{rsd(values):.4f}",
            }
        )

    overhead_rows = []
    overhead_samples = {}
    for state in ("enabled", "disabled"):
        path = ROOT / f"overhead-{state}" / "samples.csv"
        with path.open(newline="") as handle:
            rows = list(csv.DictReader(handle))
        for variant in ("zune-simd", "zune-scalar"):
            values = [
                number(row, "elapsed_nanos") / 1_000_000.0
                for row in rows
                if row["variant"] == variant
            ]
            overhead_samples[(state, variant)] = values
    for variant in ("zune-simd", "zune-scalar"):
        enabled = overhead_samples[("enabled", variant)]
        disabled = overhead_samples[("disabled", variant)]
        ratio = geometric_mean(enabled) / geometric_mean(disabled)
        overhead_rows.append(
            {
                "variant": variant,
                "enabled_median_ms": f"{median(enabled):.6f}",
                "enabled_rsd_pct": f"{rsd(enabled):.4f}",
                "disabled_median_ms": f"{median(disabled):.6f}",
                "disabled_rsd_pct": f"{rsd(disabled):.4f}",
                "geomean_overhead_pct": f"{(ratio - 1.0) * 100.0:.4f}",
            }
        )

    main_packed_simd = [
        row
        for row in end_rows
        if row["dataset"] == "main"
        and row["mode"] == "packed-bgra"
        and row["variant"] == "zune-simd"
    ]
    scale_rows = []
    for summary in main_packed_simd:
        matching = next(
            count
            for count in count_rows
            if all(count[key] == summary[key] for key in ("dataset", "workload", "fixture", "mode", "variant"))
        )
        scale_rows.append(
            {
                "time": float(summary["median_ms"]),
                "pixels": float(matching["width"]) * float(matching["height"]),
                "mcus": float(matching["mcus"]),
                "blocks": float(matching["idct_pointer_calls"]),
                "scan_count": float(matching["scan_count"]),
            }
        )
    correlations = {
        name: pearson([row[name] for row in scale_rows], [row["time"] for row in scale_rows])
        for name in ("pixels", "mcus", "blocks", "scan_count")
    }

    unstable = [
        row for row in end_rows if float(row["rsd_pct"]) > 3.0
    ]
    largest_scans = sorted(
        [row for row in scan_rows if row["dataset"] == "main" and row["kind"] == "AcRefine"],
        key=lambda row: float(row["median_ms"]),
        reverse=True,
    )[:20]

    write_csv(
        ROOT / "end-to-end-summary.csv",
        list(end_rows[0]),
        end_rows,
    )
    write_csv(ROOT / "phase-summary.csv", list(phase_rows[0]), phase_rows)
    write_csv(ROOT / "sample-phases.csv", list(sample_phase_rows[0]), sample_phase_rows)
    write_csv(ROOT / "scan-summary.csv", list(scan_rows[0]), scan_rows)
    write_csv(ROOT / "counter-summary.csv", list(count_rows[0]), count_rows)
    write_csv(ROOT / "overhead-summary.csv", list(overhead_rows[0]), overhead_rows)

    analysis = {
        "revision": "aede1e43cdd650fc12540d6334197977e2401109",
        "method": {
            "phase_clock": "x86 TSC with lfence; wall time from Instant",
            "block_sampling": "one deterministic multiplicative classification per block; entropy+IDCT and dequant use disjoint buckets, each approximately 1/256",
            "baseline_reconciliation": "sampled entropy/dequant/IDCT estimates retained when below exact baseline MCU ticks; normalized only when estimates exceed exact total",
            "progressive_reconciliation": "sampled dequant/IDCT proportions normalized to exact progressive transform ticks",
            "overhead_limitation": "same-binary active versus disabled overhead remained above target: see overhead-summary.csv; exact outer phase totals bound attribution",
        },
        "sample_groups": len(end_rows),
        "unstable_groups_over_3pct_rsd": unstable,
        "normalization_sample_counts": {
            "|".join(key): count for key, count in normalized_groups.items() if count
        },
        "median_residual_ms": {
            "|".join(key): median(values) for key, values in residual_values.items()
        },
        "scaling_correlations_main_packed_simd": correlations,
        "largest_main_ac_refinement_scans": largest_scans,
        "active_instrumentation_overhead": overhead_rows,
    }
    (ROOT / "analysis.json").write_text(json.dumps(analysis, indent=2) + "\n")
    print(json.dumps({
        "groups": len(end_rows),
        "phase_rows": len(phase_rows),
        "scan_rows": len(scan_rows),
        "unstable_groups": len(unstable),
        "overhead": overhead_rows,
        "correlations": correlations,
        "largest_ac_refine": largest_scans[:6],
    }, indent=2))


if __name__ == "__main__":
    main()
