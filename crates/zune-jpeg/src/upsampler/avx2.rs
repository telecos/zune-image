/*
 * Copyright (c) 2023.
 *
 * This software is free software;
 *
 * You can redistribute it or modify it under terms of the MIT, Apache License or Zlib license
 */

#[cfg(target_arch = "x86")]
use core::arch::x86::*;
#[cfg(target_arch = "x86_64")]
use core::arch::x86_64::*;

#[target_feature(enable = "avx2")]
pub unsafe fn upsample_horizontal_avx2(
    input: &[i16], _near: &[i16], _far: &[i16], _scratch: &mut [i16], output: &mut [i16]
) {
    unsafe { upsample_horizontal_filtered::<2, 1, 2>(input, output) }
}

#[target_feature(enable = "avx2")]
unsafe fn upsample_horizontal_filtered<const SHIFT: i32, const EVEN: i16, const ODD: i16>(
    input: &[i16], output: &mut [i16]
) {
    assert_eq!(input.len() * 2, output.len());
    assert!(input.len() > 2);
    let len = input.len();
    if len < 18 {
        return super::scalar::upsample_horizontal_filtered::<SHIFT, EVEN, ODD>(input, output);
    }
    let three = _mm256_set1_epi16(3);
    let even_bias = _mm256_set1_epi16(EVEN);
    let odd_bias = _mm256_set1_epi16(ODD);
    let ptr = input.as_ptr();
    let out = unsafe { output.as_mut_ptr().add(2) };
    let store = |previous, current, next, pos: usize| unsafe {
        let common = _mm256_mullo_epi16(current, three);
        let even = _mm256_srai_epi16::<SHIFT>(_mm256_add_epi16(
            _mm256_add_epi16(common, previous),
            even_bias
        ));
        let odd =
            _mm256_srai_epi16::<SHIFT>(_mm256_add_epi16(_mm256_add_epi16(common, next), odd_bias));
        store_interleaved(out.add(pos * 2), even, odd);
    };
    let mut previous = unsafe { _mm256_loadu_si256(ptr.cast()) };
    let mut pos = 0;
    while pos + 32 <= len {
        let next = unsafe { _mm256_loadu_si256(ptr.add(pos + 16).cast()) };
        let middle = _mm256_permute2x128_si256::<0x21>(previous, next);
        store(
            previous,
            _mm256_alignr_epi8::<2>(middle, previous),
            _mm256_alignr_epi8::<4>(middle, previous),
            pos
        );
        previous = next;
        pos += 16;
    }
    let tail = |pos: usize| unsafe {
        store(
            _mm256_loadu_si256(ptr.add(pos).cast()),
            _mm256_loadu_si256(ptr.add(pos + 1).cast()),
            _mm256_loadu_si256(ptr.add(pos + 2).cast()),
            pos
        );
    };
    let last = len - 18;
    if pos < last {
        tail(pos);
    }
    tail(last);
    output[0] = (4 * input[0] + EVEN) >> SHIFT;
    output[1] = (3 * input[0] + input[1] + ODD) >> SHIFT;
    output[len * 2 - 2] = (3 * input[len - 1] + input[len - 2] + EVEN) >> SHIFT;
    output[len * 2 - 1] = (4 * input[len - 1] + ODD) >> SHIFT;
}

#[target_feature(enable = "avx2")]
unsafe fn store_interleaved(output: *mut i16, even: __m256i, odd: __m256i) {
    let low = _mm256_unpacklo_epi16(even, odd);
    let high = _mm256_unpackhi_epi16(even, odd);
    unsafe {
        _mm256_storeu_si256(output.cast(), _mm256_permute2x128_si256::<0x20>(low, high));
        _mm256_storeu_si256(
            output.add(16).cast(),
            _mm256_permute2x128_si256::<0x31>(low, high)
        );
    }
}

#[target_feature(enable = "avx2")]
pub unsafe fn upsample_vertical_avx2(
    input: &[i16], near: &[i16], far: &[i16], _scratch: &mut [i16], output: &mut [i16]
) {
    unsafe { upsample_vertical_filtered::<true>(input, near, far, output) }
}

#[target_feature(enable = "avx2")]
unsafe fn upsample_vertical_filtered<const ROUND: bool>(
    input: &[i16], near: &[i16], far: &[i16], output: &mut [i16]
) {
    let len = input.len();
    assert_eq!(len * 2, output.len());
    assert_eq!(near.len(), len);
    assert_eq!(far.len(), len);
    if len < 16 {
        return super::scalar::upsample_vertical_filtered::<ROUND>(input, near, far, output);
    }
    let three = _mm256_set1_epi16(3);
    let one = _mm256_set1_epi16(1);
    let two = _mm256_set1_epi16(2);
    let (top, bottom) = output.split_at_mut(len);
    let top_ptr = top.as_mut_ptr();
    let bottom_ptr = bottom.as_mut_ptr();
    let write = |pos: usize| unsafe {
        let common = _mm256_mullo_epi16(_mm256_loadu_si256(input.as_ptr().add(pos).cast()), three);
        let top_sum = _mm256_add_epi16(common, _mm256_loadu_si256(near.as_ptr().add(pos).cast()));
        let bottom_sum = _mm256_add_epi16(common, _mm256_loadu_si256(far.as_ptr().add(pos).cast()));
        let top_value = if ROUND {
            _mm256_srai_epi16::<2>(_mm256_add_epi16(top_sum, one))
        } else {
            top_sum
        };
        let bottom_value = if ROUND {
            _mm256_srai_epi16::<2>(_mm256_add_epi16(bottom_sum, two))
        } else {
            bottom_sum
        };
        _mm256_storeu_si256(top_ptr.add(pos).cast(), top_value);
        _mm256_storeu_si256(bottom_ptr.add(pos).cast(), bottom_value);
    };
    let mut pos = 0;
    while pos + 16 <= len {
        write(pos);
        pos += 16;
    }
    if pos < len {
        write(len - 16);
    }
}

#[target_feature(enable = "avx2")]
pub unsafe fn upsample_hv_avx2(
    input: &[i16], near: &[i16], far: &[i16], scratch: &mut [i16], output: &mut [i16]
) {
    let len = input.len();
    assert_eq!(len * 4, output.len());
    assert_eq!(near.len(), len);
    assert_eq!(far.len(), len);
    if len < 32 {
        assert!(scratch.len() >= len * 2);
        let scratch = &mut scratch[..len * 2];
        unsafe { upsample_vertical_filtered::<false>(input, near, far, scratch) };
        let (top, bottom) = output.split_at_mut(len * 2);
        unsafe {
            upsample_horizontal_filtered::<4, 8, 7>(&scratch[..len], top);
            upsample_horizontal_filtered::<4, 8, 7>(&scratch[len..], bottom);
        }
        return;
    }
    // Fuse vertical sums and horizontal interpolation without intermediate rounding.
    let three = _mm256_set1_epi16(3);
    let even_bias = _mm256_set1_epi16(8);
    let odd_bias = _mm256_set1_epi16(7);
    let (top, bottom) = output.split_at_mut(len * 2);
    let top_ptr = top.as_mut_ptr();
    let bottom_ptr = bottom.as_mut_ptr();
    let sums = |pos: usize| unsafe {
        let common = _mm256_mullo_epi16(_mm256_loadu_si256(input.as_ptr().add(pos).cast()), three);
        (
            _mm256_add_epi16(common, _mm256_loadu_si256(near.as_ptr().add(pos).cast())),
            _mm256_add_epi16(common, _mm256_loadu_si256(far.as_ptr().add(pos).cast()))
        )
    };
    let pack = |out: *mut i16, previous, current, next| unsafe {
        let common = _mm256_mullo_epi16(current, three);
        let even = _mm256_srai_epi16::<4>(_mm256_add_epi16(
            _mm256_add_epi16(common, previous),
            even_bias
        ));
        let odd =
            _mm256_srai_epi16::<4>(_mm256_add_epi16(_mm256_add_epi16(common, next), odd_bias));
        store_interleaved(out, even, odd);
    };
    let write = |pos: usize,
                 previous: (__m256i, __m256i),
                 current: (__m256i, __m256i),
                 next: (__m256i, __m256i)| unsafe {
        pack(top_ptr.add(2 + pos * 2), previous.0, current.0, next.0);
        pack(bottom_ptr.add(2 + pos * 2), previous.1, current.1, next.1);
    };
    let mut previous = sums(0);
    let mut pos = 0;
    while pos + 32 <= len {
        let next = sums(pos + 16);
        let mid_top = _mm256_permute2x128_si256::<0x21>(previous.0, next.0);
        let mid_bottom = _mm256_permute2x128_si256::<0x21>(previous.1, next.1);
        write(
            pos,
            previous,
            (
                _mm256_alignr_epi8::<2>(mid_top, previous.0),
                _mm256_alignr_epi8::<2>(mid_bottom, previous.1)
            ),
            (
                _mm256_alignr_epi8::<4>(mid_top, previous.0),
                _mm256_alignr_epi8::<4>(mid_bottom, previous.1)
            )
        );
        previous = next;
        pos += 16;
    }
    let last = len - 18;
    if pos < last {
        write(pos, sums(pos), sums(pos + 1), sums(pos + 2));
    }
    write(last, sums(last), sums(last + 1), sums(last + 2));
    for (other, row) in [(near, top), (far, bottom)] {
        let first = 3 * input[0] + other[0];
        let second = 3 * input[1] + other[1];
        row[0] = (4 * first + 8) >> 4;
        row[1] = (3 * first + second + 7) >> 4;
        let end = 3 * input[len - 1] + other[len - 1];
        let before = 3 * input[len - 2] + other[len - 2];
        row[len * 2 - 2] = (3 * end + before + 8) >> 4;
        row[len * 2 - 1] = (4 * end + 7) >> 4;
    }
}

#[cfg(all(test, feature = "std"))]
mod tests {
    use super::*;
    use crate::upsampler::scalar;

    #[test]
    fn sliding_windows_and_fused_hv_match_scalar() {
        if !is_x86_feature_detected!("avx2") {
            return;
        }
        for len in (3..=160).chain([256, 1280]) {
            for pattern in 0..4 {
                let row = |seed| -> Vec<i16> {
                    (0..len)
                        .map(|x| match pattern {
                            0 => 0,
                            1 => 255,
                            2 => {
                                if (x + seed) % 2 == 0 {
                                    0
                                } else {
                                    255
                                }
                            }
                            _ => ((x * 79 + seed * 53) % 256) as i16
                        })
                        .collect()
                };
                let input = row(0);
                let near = row(1);
                let far = row(2);
                for kind in 0..3 {
                    let size = len * if kind == 2 { 4 } else { 2 };
                    let mut actual = vec![-1234; size + 17];
                    let mut expected = actual.clone();
                    let mut scratch = vec![0; len * 2];
                    let out = &mut actual[5..5 + size];
                    let reference = &mut expected[5..5 + size];
                    unsafe {
                        match kind {
                            0 => {
                                scalar::upsample_horizontal(
                                    &input,
                                    &near,
                                    &far,
                                    &mut scratch,
                                    reference
                                );
                                upsample_horizontal_avx2(&input, &near, &far, &mut [], out);
                            }
                            1 => {
                                scalar::upsample_vertical(
                                    &input,
                                    &near,
                                    &far,
                                    &mut scratch,
                                    reference
                                );
                                upsample_vertical_avx2(&input, &near, &far, &mut [], out);
                            }
                            _ => {
                                scalar::upsample_hv(&input, &near, &far, &mut scratch, reference);
                                if len >= 32 {
                                    upsample_hv_avx2(&input, &near, &far, &mut [], out);
                                } else {
                                    upsample_hv_avx2(&input, &near, &far, &mut scratch, out);
                                }
                            }
                        }
                    }
                    assert_eq!(
                        actual, expected,
                        "len {len}, pattern {pattern}, kind {kind}"
                    );
                }
            }
        }
    }
}
