use std::cell::Cell;
use std::env;
use std::fs::{create_dir_all, read, File};
use std::hint::black_box;
use std::io::{BufRead, Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::OnceLock;
use std::time::Instant;

use zune_benches::sample_path;
use zune_jpeg::zune_core::bytestream::ZCursor;
use zune_jpeg::zune_core::colorspace::ColorSpace;
use zune_jpeg::zune_core::options::DecoderOptions;
use zune_jpeg::{DecodeProfile, JpegDecoder, RawImcuRowStatus, ScanlineReadStatus, ScanlineStatus};

const FIXTURES: [(&str, &str); 8] = [
    ("baseline-444", "speed_bench.jpg"),
    ("baseline-422", "speed_bench_horizontal_subsampling.jpg"),
    ("baseline-440", "speed_bench_vertical_subsampling.jpg"),
    ("baseline-420", "speed_bench_hv_subsampling.jpg"),
    ("progressive-444", "speed_bench_prog.jpg"),
    ("progressive-422", "speed_bench_prog_h_sampling.jpg"),
    ("progressive-440", "speed_bench_prog_v_sampling.jpg"),
    ("progressive-420", "speed_bench_prog_hv_sampling.jpg")
];

const MODES: [&str; 3] = ["packed-bgra", "scanlines-bgra", "raw"];

struct GrowableCursor<'a> {
    data:     &'a [u8],
    position: usize,
    limit:    Rc<Cell<usize>>
}

impl<'a> GrowableCursor<'a> {
    fn new(data: &'a [u8], limit: Rc<Cell<usize>>) -> Self {
        Self {
            data,
            position: 0,
            limit
        }
    }

    fn visible(&self) -> usize {
        self.limit.get().min(self.data.len())
    }
}

impl Read for GrowableCursor<'_> {
    fn read(&mut self, output: &mut [u8]) -> std::io::Result<usize> {
        let visible = self.visible();
        if self.position >= visible {
            return Ok(0);
        }
        let count = output.len().min(visible - self.position);
        output[..count].copy_from_slice(&self.data[self.position..self.position + count]);
        self.position += count;
        Ok(count)
    }
}

impl BufRead for GrowableCursor<'_> {
    fn fill_buf(&mut self) -> std::io::Result<&[u8]> {
        Ok(&self.data[self.position.min(self.visible())..self.visible()])
    }

    fn consume(&mut self, amount: usize) {
        self.position += amount;
    }
}

impl Seek for GrowableCursor<'_> {
    fn seek(&mut self, position: SeekFrom) -> std::io::Result<u64> {
        let next = match position {
            SeekFrom::Start(offset) => offset as i64,
            SeekFrom::Current(offset) => self.position as i64 + offset,
            SeekFrom::End(offset) => self.visible() as i64 + offset
        };
        if next < 0 {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "seek before start"
            ));
        }
        self.position = next as usize;
        Ok(self.position as u64)
    }
}

fn options(simd: bool) -> DecoderOptions {
    static OUTPUT_COLOR: OnceLock<ColorSpace> = OnceLock::new();
    let output_color =
        *OUTPUT_COLOR.get_or_init(|| match env::var("ZUNE_PROFILE_COLOR").as_deref() {
            Ok("rgb") => ColorSpace::RGB,
            Ok("rgba") => ColorSpace::RGBA,
            Ok("bgr") => ColorSpace::BGR,
            Ok("luma") => ColorSpace::Luma,
            Ok("bgra") | Err(_) => ColorSpace::BGRA,
            Ok(other) => panic!("unsupported ZUNE_PROFILE_COLOR {other}")
        });
    DecoderOptions::default()
        .jpeg_set_out_colorspace(output_color)
        .set_use_unsafe(simd)
}

fn packed(data: &[u8], simd: bool, profile_enabled: bool) -> (u64, DecodeProfile) {
    let mut decoder = JpegDecoder::new_with_options(ZCursor::new(data), options(simd));
    decoder.set_profile_enabled(profile_enabled);
    let output = decoder.decode().unwrap();
    black_box(&output);
    (0, decoder.profile())
}

fn scanlines(data: &[u8], simd: bool, profile_enabled: bool) -> (u64, DecodeProfile) {
    let mut decoder = JpegDecoder::new_with_options(ZCursor::new(data), options(simd));
    decoder.set_profile_enabled(profile_enabled);
    {
        let mut session = decoder.scanline_output();
        assert_eq!(session.start().unwrap(), ScanlineStatus::Ready);
        let row_bytes = session.output_row_bytes().unwrap();
        let height = session.output_height().unwrap();
        let mut output = vec![0_u8; row_bytes * 64];
        while session.output_scanline() < height {
            match session.read_scanlines(&mut output, row_bytes).unwrap() {
                ScanlineReadStatus::RowsProcessed { rows } => {
                    black_box(&output[..rows * row_bytes]);
                }
                ScanlineReadStatus::Complete => break,
                ScanlineReadStatus::NeedMoreInput => panic!("complete input suspended"),
                _ => unreachable!()
            }
        }
        assert_eq!(session.finish().unwrap(), ScanlineStatus::Complete);
    }
    (0, decoder.profile())
}

fn raw(data: &[u8], simd: bool, profile_enabled: bool) -> (u64, DecodeProfile) {
    let mut decoder = JpegDecoder::new_with_options(ZCursor::new(data), options(simd));
    decoder.set_profile_enabled(profile_enabled);
    decoder.decode_headers().unwrap();
    {
        let mut session = decoder.raw_output();
        let layout = session.layout().unwrap();
        let components = session.num_components().unwrap();
        let strides: Vec<usize> = layout[..components]
            .iter()
            .map(|plane| plane.width)
            .collect();
        let mut storage: Vec<Vec<u8>> = layout[..components]
            .iter()
            .map(|plane| vec![0; plane.width * plane.vertical_sampling_factor * 8])
            .collect();
        loop {
            let mut planes: Vec<&mut [u8]> = storage.iter_mut().map(Vec::as_mut_slice).collect();
            match session.decode_next_imcu_row(&mut planes, &strides).unwrap() {
                RawImcuRowStatus::RowReady { rows_written } => {
                    for index in 0..components {
                        black_box(&storage[index][..rows_written[index] * strides[index]]);
                    }
                }
                RawImcuRowStatus::Complete => break,
                RawImcuRowStatus::NeedMoreInput => panic!("complete input suspended"),
                _ => unreachable!()
            }
        }
    }
    (0, decoder.profile())
}

fn incremental(data: &[u8], simd: bool, profile_enabled: bool) -> (u64, DecodeProfile) {
    let partial = data.len() * 80 / 100;
    let limit = Rc::new(Cell::new(partial));
    let cursor = GrowableCursor::new(data, Rc::clone(&limit));
    let mut decoder = JpegDecoder::new_with_options(cursor, options(simd));
    decoder.set_profile_enabled(profile_enabled);
    decoder.set_incremental_mode(true);
    decoder.decode_headers().unwrap();
    let mut output = vec![0; decoder.output_buffer_size().unwrap()];
    let first = decoder.decode_into(&mut output).unwrap_err();
    assert!(first.is_recoverable_eof());
    limit.set(data.len());
    decoder.decode_into(&mut output).unwrap();
    black_box(&output);
    (0, decoder.profile())
}

fn decode(mode: &str, data: &[u8], simd: bool, profile_enabled: bool) -> (u64, DecodeProfile) {
    match mode {
        "packed-bgra" => packed(data, simd, profile_enabled),
        "scanlines-bgra" => scanlines(data, simd, profile_enabled),
        "raw" => raw(data, simd, profile_enabled),
        "incremental-bgra" => incremental(data, simd, profile_enabled),
        _ => unreachable!()
    }
}

fn write_profile(
    file: &mut File, sample_id: usize, workload: &str, fixture: &str, mode: &str, variant: &str,
    repetition: usize, elapsed_nanos: u128, profile: &DecodeProfile
) {
    write!(
        file,
        "{sample_id},{workload},{fixture},{mode},{variant},{repetition},{elapsed_nanos}"
    )
    .unwrap();
    macro_rules! field {
        ($value:expr) => {
            write!(file, ",{}", $value).unwrap()
        };
    }
    field!(profile.total_nanos);
    field!(profile.total_ticks);
    field!(profile.timer_overhead_ticks);
    field!(profile.width);
    field!(profile.height);
    field!(profile.components);
    field!(profile.mcu_rows);
    field!(profile.mcus);
    field!(profile.idct_1x1_calls);
    field!(profile.idct_4x4_calls);
    field!(profile.idct_8x8_calls);
    field!(profile.idct_1x1_samples);
    field!(profile.idct_4x4_samples);
    field!(profile.idct_8x8_samples);
    field!(profile.idct_1x1_ticks);
    field!(profile.idct_4x4_ticks);
    field!(profile.idct_8x8_ticks);
    field!(profile.entropy_samples);
    field!(profile.entropy_ticks);
    field!(profile.baseline_mcu_ticks);
    field!(profile.sampled_nonzero_coefficients);
    field!(profile.dequant_samples);
    field!(profile.dequant_ticks);
    field!(profile.upsample_none_calls);
    field!(profile.upsample_horizontal_calls);
    field!(profile.upsample_vertical_calls);
    field!(profile.upsample_hv_calls);
    field!(profile.upsample_ticks);
    field!(profile.color_convert_calls);
    field!(profile.color_convert_ticks);
    field!(profile.post_process_ticks);
    field!(profile.progressive_reconstruct_ticks);
    field!(profile.progressive_transform_ticks);
    field!(profile.checkpoint_creates);
    field!(profile.checkpoint_restores);
    field!(profile.checkpoint_ticks);
    field!(profile.restart_markers);
    field!(profile.cancellation_polls);
    field!(profile.raw_coeff_bytes);
    field!(profile.raw_output_bytes);
    field!(profile.raw_copy_ticks);
    field!(profile.upsample_scratch_bytes);
    field!(profile.upsample_destination_bytes);
    field!(profile.color_input_bytes);
    field!(profile.packed_output_bytes);
    field!(profile.progressive_init_bytes);
    field!(profile.progressive_copy_bytes);
    field!(profile.capacity_growths);
    field!(profile.idct_pointer_calls);
    field!(profile.upsample_pointer_calls);
    field!(profile.color_pointer_calls);
    field!(profile.pull_calls);
    field!(profile.scan_count);
    writeln!(file).unwrap();
}

fn write_scans(file: &mut File, sample_id: usize, profile: &DecodeProfile) {
    for (index, scan) in profile.scans[..profile.scan_count].iter().enumerate() {
        writeln!(
            file,
            "{sample_id},{index},{:?},{},{},{}",
            scan.kind, scan.component, scan.ticks, scan.mcus
        )
        .unwrap();
    }
}

fn main() {
    let output_dir = PathBuf::from(
        env::var_os("ZUNE_PROFILE_OUT").unwrap_or_else(|| "/tmp/zune-jpeg-profile".into())
    );
    let repetitions = env::var("ZUNE_PROFILE_REPETITIONS")
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(5);
    let iterations = env::var("ZUNE_PROFILE_ITERATIONS")
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(3);
    let profile_enabled = env::var_os("ZUNE_PROFILE_DISABLED").is_none();
    let fixture_filter = env::var("ZUNE_PROFILE_FIXTURE").ok();
    let mode_filter = env::var("ZUNE_PROFILE_MODE").ok();
    let custom_fixture = env::var_os("ZUNE_PROFILE_PATH").map(PathBuf::from);
    let custom_workload = env::var("ZUNE_PROFILE_WORKLOAD").ok();
    create_dir_all(&output_dir).unwrap();
    let mut samples = File::create(output_dir.join("samples.csv")).unwrap();
    let mut scans = File::create(output_dir.join("scans.csv")).unwrap();
    writeln!(samples, "sample_id,workload,fixture,mode,variant,repetition,elapsed_nanos,profile_total_nanos,profile_total_ticks,timer_overhead_ticks,width,height,components,mcu_rows,mcus,idct_1x1_calls,idct_4x4_calls,idct_8x8_calls,idct_1x1_samples,idct_4x4_samples,idct_8x8_samples,idct_1x1_ticks,idct_4x4_ticks,idct_8x8_ticks,entropy_samples,entropy_ticks,baseline_mcu_ticks,sampled_nonzero_coefficients,dequant_samples,dequant_ticks,upsample_none_calls,upsample_horizontal_calls,upsample_vertical_calls,upsample_hv_calls,upsample_ticks,color_convert_calls,color_convert_ticks,post_process_ticks,progressive_reconstruct_ticks,progressive_transform_ticks,checkpoint_creates,checkpoint_restores,checkpoint_ticks,restart_markers,cancellation_polls,raw_coeff_bytes,raw_output_bytes,raw_copy_ticks,upsample_scratch_bytes,upsample_destination_bytes,color_input_bytes,packed_output_bytes,progressive_init_bytes,progressive_copy_bytes,capacity_growths,idct_pointer_calls,upsample_pointer_calls,color_pointer_calls,pull_calls,scan_count").unwrap();
    writeln!(scans, "sample_id,scan_index,kind,component,ticks,mcus").unwrap();

    let core = core_affinity::get_core_ids()
        .and_then(|cores| cores.into_iter().next())
        .expect("no benchmark CPU available");
    assert!(core_affinity::set_for_current(core));

    let fixture_root = sample_path().join("test-images/jpeg/benchmarks");
    let mut sample_id = 0;
    let fixtures: Vec<(String, PathBuf)> = if let Some(path) = custom_fixture {
        vec![(
            custom_workload.unwrap_or_else(|| "custom".to_string()),
            path
        )]
    } else {
        FIXTURES
            .iter()
            .map(|(workload, fixture)| ((*workload).to_string(), fixture_root.join(fixture)))
            .collect()
    };
    for (workload, fixture_path) in fixtures {
        if fixture_filter
            .as_deref()
            .is_some_and(|filter| filter != workload.as_str())
        {
            continue;
        }
        let data = read(&fixture_path).unwrap();
        let fixture = fixture_path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("unknown");
        let modes: &[&str] = if workload == "baseline-420" {
            &["packed-bgra", "scanlines-bgra", "raw", "incremental-bgra"]
        } else {
            &MODES
        };
        for &mode in modes {
            if mode_filter.as_deref().is_some_and(|filter| filter != mode) {
                continue;
            }
            for simd in [true, false] {
                black_box(decode(mode, &data, simd, profile_enabled));
            }
            for repetition in 0..repetitions {
                let order = if repetition % 2 == 0 { [true, false] } else { [false, true] };
                for simd in order {
                    for _ in 0..iterations {
                        let started = Instant::now();
                        let (checksum, profile) = decode(mode, &data, simd, profile_enabled);
                        let elapsed = started.elapsed().as_nanos();
                        black_box(checksum);
                        let variant = if simd { "zune-simd" } else { "zune-scalar" };
                        write_profile(
                            &mut samples,
                            sample_id,
                            &workload,
                            fixture,
                            mode,
                            variant,
                            repetition,
                            elapsed,
                            &profile
                        );
                        write_scans(&mut scans, sample_id, &profile);
                        sample_id += 1;
                    }
                }
            }
        }
    }

    let mut metadata = File::create(output_dir.join("metadata.txt")).unwrap();
    writeln!(
        metadata,
        "revision=aede1e43cdd650fc12540d6334197977e2401109"
    )
    .unwrap();
    writeln!(metadata, "profile_enabled={profile_enabled}").unwrap();
    writeln!(metadata, "repetitions={repetitions}").unwrap();
    writeln!(metadata, "iterations={iterations}").unwrap();
    writeln!(
        metadata,
        "output_color={}",
        env::var("ZUNE_PROFILE_COLOR").unwrap_or_else(|_| "bgra".to_string())
    )
    .unwrap();
    writeln!(metadata, "fixture_root={}", fixture_root.display()).unwrap();
    writeln!(
        metadata,
        "cwd={}",
        Path::new(".").canonicalize().unwrap().display()
    )
    .unwrap();
}
