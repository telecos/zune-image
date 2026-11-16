use alloc::vec;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicUsize, Ordering};

use zune_core::bytestream::ZCursor;
use zune_core::colorspace::ColorSpace;
use zune_core::options::DecoderOptions;

use crate::JpegDecoder;

static IDCT_CALLS: AtomicUsize = AtomicUsize::new(0);

fn count_full(block: &mut [i32; 64], output: &mut [i16], stride: usize) {
    IDCT_CALLS.fetch_add(1, Ordering::Relaxed);
    crate::idct::scalar::idct_int(block, output, stride);
}

fn count_sparse(block: &mut [i32; 64], output: &mut [i16], stride: usize) {
    IDCT_CALLS.fetch_add(1, Ordering::Relaxed);
    crate::idct::scalar::idct4x4(block, output, stride);
}

fn count_dc(block: &mut [i32; 64], output: &mut [i16], stride: usize) {
    IDCT_CALLS.fetch_add(1, Ordering::Relaxed);
    crate::idct::scalar::idct_int_1x1(block, output, stride);
}

#[test]
fn missing_mcus_do_not_execute_idct() {
    // Nonzero AC keeps this assertion meaningful when DC-only IDCT is inlined.
    let mut data = vec![255, 216];
    let segment = |data: &mut Vec<u8>, marker, body: &[u8]| {
        data.extend_from_slice(&[255, marker]);
        data.extend_from_slice(&u16::try_from(body.len() + 2).unwrap().to_be_bytes());
        data.extend_from_slice(body);
    };
    let mut quantization = vec![1; 65];
    quantization[0] = 0;
    segment(&mut data, 219, &quantization);
    let mut dc = vec![0; 18];
    dc[1] = 1;
    segment(&mut data, 196, &dc);
    let mut ac = vec![0; 19];
    ac[0] = 16;
    ac[2] = 2;
    ac[17] = 1;
    segment(&mut data, 196, &ac);
    segment(
        &mut data,
        192,
        &[8, 234, 96, 0, 32, 3, 1, 17, 0, 2, 17, 0, 3, 17, 0]
    );
    segment(&mut data, 218, &[3, 1, 0, 2, 0, 3, 0, 0, 63, 0]);
    let mut bits = Vec::new();
    for _ in 0..6 {
        bits.extend_from_slice(&[0, 0, 0, 1, 0, 1]);
    }
    while bits.len() % 8 != 0 {
        bits.push(1);
    }
    for byte in bits.chunks_exact(8) {
        data.push(byte.iter().fold(0, |value, bit| value * 2 + bit));
    }
    data.extend_from_slice(&[255, 217]);
    let options = DecoderOptions::default()
        .set_use_unsafe(false)
        .set_max_height(65_535)
        .set_strict_mode(false)
        .jpeg_set_out_colorspace(ColorSpace::RGBA);
    let mut decoder = JpegDecoder::new_with_options(ZCursor::new(data), options);
    decoder.decode_headers().unwrap();
    // Only this decoder uses these counters, so other tests cannot affect them.
    IDCT_CALLS.store(0, Ordering::Relaxed);
    decoder.idct_func = count_full;
    decoder.idct_4x4_func = count_sparse;
    decoder.idct_1x1_func = count_dc;
    let output = decoder.decode().unwrap();
    assert_eq!(output.len(), 32 * 60_000 * 4);
    // Two coded MCUs and the partial marker-boundary MCU, never the missing rows.
    assert_eq!(IDCT_CALLS.load(Ordering::Relaxed), 9);
    assert!(output[32 * 8 * 4..]
        .chunks_exact(4)
        .all(|pixel| pixel == [128, 128, 128, 255]));
}
