use std::fs::{read, read_dir};
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::path::PathBuf;

use zune_core::bytestream::ZCursor;
use zune_core::options::DecoderOptions;
use zune_jpeg::JpegDecoder;

use crate::sample_path;

const MAX_OUTPUT_LEN: usize = 64 * 1024 * 1024;

fn corpus_files() -> Vec<PathBuf> {
    let mut files: Vec<_> = read_dir(sample_path().join("fuzz-corpus/jpeg"))
        .expect("JPEG fuzz corpus is missing")
        .map(|entry| entry.expect("failed to read JPEG corpus entry").path())
        .filter(|path| path.is_file())
        .collect();
    files.sort();
    files
}

#[test]
fn test_jpeg_fuzz_corpus() {
    let files = corpus_files();
    assert!(!files.is_empty(), "JPEG fuzz corpus is empty");

    for path in files {
        let data = read(&path).unwrap_or_else(|error| {
            panic!(
                "failed to read JPEG corpus seed {}: {error}",
                path.display()
            )
        });
        for strict in [false, true] {
            let result = catch_unwind(AssertUnwindSafe(|| {
                let options = DecoderOptions::default()
                    .set_strict_mode(strict)
                    .set_max_width(4_096)
                    .set_max_height(4_096)
                    .jpeg_set_max_scans(256);
                let mut decoder = JpegDecoder::new_with_options(ZCursor::new(&data), options);
                if decoder.decode_headers().is_err() {
                    return;
                }
                let Some(size) = decoder.output_buffer_size() else {
                    return;
                };
                if size > MAX_OUTPUT_LEN {
                    return;
                }
                let mut output = vec![0; size];
                let _ = decoder.decode_into(&mut output);
            }));
            assert!(
                result.is_ok(),
                "JPEG corpus seed {} panicked in strict={strict}",
                path.display()
            );
        }
    }
}
