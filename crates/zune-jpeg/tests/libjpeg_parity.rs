use zune_core::bytestream::ZCursor;
use zune_core::colorspace::ColorSpace;
use zune_core::options::DecoderOptions;
use zune_jpeg::JpegDecoder;

struct ParityCase {
    name: &'static str,
    jpeg: &'static [u8],
    reference: &'static [u8],
    max_delta: u8,
    mean_delta: f64,
    min_psnr: f64,
}

fn ppm_payload(data: &[u8]) -> (usize, usize, &[u8]) {
    fn token<'a>(data: &'a [u8], position: &mut usize) -> &'a [u8] {
        loop {
            while data.get(*position).is_some_and(u8::is_ascii_whitespace) {
                *position += 1;
            }
            if data.get(*position) != Some(&b'#') {
                break;
            }
            while data.get(*position).is_some_and(|byte| *byte != b'\n') {
                *position += 1;
            }
        }
        let start = *position;
        while data
            .get(*position)
            .is_some_and(|byte| !byte.is_ascii_whitespace())
        {
            *position += 1;
        }
        &data[start..*position]
    }

    let mut position = 0;
    assert_eq!(token(data, &mut position), b"P6");
    let width = std::str::from_utf8(token(data, &mut position))
        .unwrap()
        .parse()
        .unwrap();
    let height = std::str::from_utf8(token(data, &mut position))
        .unwrap()
        .parse()
        .unwrap();
    assert_eq!(token(data, &mut position), b"255");
    assert!(data.get(position).is_some_and(u8::is_ascii_whitespace));
    position += 1;
    (width, height, &data[position..])
}

fn assert_parity(case: &ParityCase) {
    let options = DecoderOptions::default()
        .set_strict_mode(true)
        .jpeg_set_out_colorspace(ColorSpace::RGB);
    let mut decoder = JpegDecoder::new_with_options(ZCursor::new(case.jpeg), options);
    let actual = decoder
        .decode()
        .unwrap_or_else(|error| panic!("{} failed to decode: {error:?}", case.name));
    let (width, height, expected) = ppm_payload(case.reference);

    assert_eq!(decoder.dimensions(), Some((width, height)), "{}", case.name);
    assert_eq!(actual.len(), expected.len(), "{}", case.name);

    let mut max_delta = 0_u8;
    let mut total_delta = 0_u64;
    let mut squared_error = 0_f64;
    for (&actual, &expected) in actual.iter().zip(expected) {
        let delta = actual.abs_diff(expected);
        max_delta = max_delta.max(delta);
        total_delta += u64::from(delta);
        squared_error += f64::from(delta).powi(2);
    }
    let mean_delta = total_delta as f64 / actual.len() as f64;
    let mean_squared_error = squared_error / actual.len() as f64;
    let psnr = if mean_squared_error == 0.0 {
        f64::INFINITY
    } else {
        10.0 * (255.0_f64.powi(2) / mean_squared_error).log10()
    };
    assert!(
        max_delta <= case.max_delta,
        "{} max channel delta {max_delta} exceeds {}",
        case.name,
        case.max_delta
    );
    assert!(
        mean_delta <= case.mean_delta,
        "{} mean channel delta {mean_delta} exceeds {}",
        case.name,
        case.mean_delta
    );
    assert!(
        psnr >= case.min_psnr,
        "{} PSNR {psnr} dB is below {} dB",
        case.name,
        case.min_psnr
    );
}

macro_rules! quality_case {
    ($name:literal, $max_delta:literal, $mean_delta:literal, $min_psnr:literal) => {
        ParityCase {
            name: $name,
            jpeg: include_bytes!(concat!(
                "../../../test-images/jpeg/libjpeg-reference/",
                $name,
                ".jpg"
            )),
            reference: include_bytes!(concat!(
                "../../../test-images/jpeg/libjpeg-reference/",
                $name,
                ".ppm"
            )),
            max_delta: $max_delta,
            mean_delta: $mean_delta,
            min_psnr: $min_psnr,
        }
    };
}

#[test]
fn huffman_outputs_remain_close_to_libjpeg_turbo() {
    for case in [
        quality_case!("sampling_444", 3, 0.3, 51.0),
        quality_case!("sampling_422", 3, 0.3, 51.0),
        quality_case!("sampling_420", 3, 0.3, 51.0),
        quality_case!("sampling_440", 3, 0.3, 51.0),
        quality_case!("sampling_411", 3, 0.3, 51.0),
        quality_case!("sampling_410", 3, 0.3, 51.0),
        quality_case!("noninterleaved_420", 3, 0.3, 51.0),
        quality_case!("progressive_420", 3, 0.3, 51.0),
        quality_case!("restart_420", 3, 0.3, 51.0),
    ] {
        assert_parity(&case);
    }
}

#[cfg(feature = "arith")]
#[test]
fn arithmetic_outputs_remain_close_to_libjpeg_turbo() {
    for case in [
        quality_case!("arithmetic_420", 3, 0.3, 51.0),
        quality_case!("arithmetic_progressive_420", 3, 0.3, 51.0),
    ] {
        assert_parity(&case);
    }
}
