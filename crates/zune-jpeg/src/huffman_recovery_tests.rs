use zune_core::bytestream::ZCursor;
use zune_core::colorspace::ColorSpace;
use zune_core::options::DecoderOptions;

use crate::JpegDecoder;

#[test]
fn missing_mcus_do_not_execute_idct() {
    let mut data = include_bytes!("../../../test-images/jpeg/recovery/early_eoi_444.jpg").to_vec();
    let sof = data
        .windows(2)
        .position(|bytes| bytes == [255, 192])
        .unwrap();
    data[sof + 5..sof + 7].copy_from_slice(&60_000u16.to_be_bytes());
    let mut reference = None;
    for use_simd in [false, true] {
        let options = DecoderOptions::default()
            .set_use_unsafe(use_simd)
            .set_max_height(65_535)
            .set_strict_mode(false)
            .jpeg_set_out_colorspace(ColorSpace::RGBA);
        let mut decoder = JpegDecoder::new_with_options(ZCursor::new(&data), options);
        let output = decoder.decode().unwrap();
        assert_eq!(output.len(), 32 * 60_000 * 4);
        assert_eq!(decoder.idct_calls, 6);
        assert!(output[..32 * 8 * 4]
            .chunks_exact(4)
            .any(|pixel| pixel != [128, 128, 128, 255]));
        assert!(output[32 * 8 * 4..]
            .chunks_exact(4)
            .all(|pixel| pixel == [128, 128, 128, 255]));
        if let Some(expected) = &reference {
            assert_eq!(&output, expected);
        }
        reference = Some(output);
    }
}
