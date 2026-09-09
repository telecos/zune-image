#[cfg(target_arch = "x86")]
use core::arch::x86::{_mm_lfence, _rdtsc};
#[cfg(target_arch = "x86_64")]
use core::arch::x86_64::{_mm_lfence, _rdtsc};
use std::time::Instant;

use crate::decoder::MAX_COMPONENTS;

pub const PROFILE_SCAN_CAPACITY: usize = 64;

#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
pub(crate) type ProfileTick = u64;
#[cfg(not(any(target_arch = "x86", target_arch = "x86_64")))]
pub(crate) type ProfileTick = Instant;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ProfileScanKind {
    #[default]
    DcFirst,
    DcRefine,
    AcFirst,
    AcRefine
}

#[derive(Clone, Copy, Debug, Default)]
pub struct ProfileScan {
    pub kind:      ProfileScanKind,
    pub component: i8,
    pub ticks:     u64,
    pub mcus:      u64
}

#[derive(Clone, Debug)]
pub struct DecodeProfile {
    pub enabled:                       bool,
    pub width:                         u16,
    pub height:                        u16,
    pub sampling:                      [(u8, u8); MAX_COMPONENTS],
    pub components:                    u8,
    pub mcu_rows:                      u64,
    pub mcus:                          u64,
    pub blocks_by_component:           [u64; MAX_COMPONENTS],
    pub idct_1x1_calls:                u64,
    pub idct_4x4_calls:                u64,
    pub idct_8x8_calls:                u64,
    pub idct_1x1_samples:              u64,
    pub idct_4x4_samples:              u64,
    pub idct_8x8_samples:              u64,
    pub idct_1x1_ticks:                u64,
    pub idct_4x4_ticks:                u64,
    pub idct_8x8_ticks:                u64,
    pub entropy_samples:               u64,
    pub entropy_ticks:                 u64,
    pub baseline_mcu_ticks:            u64,
    pub sampled_nonzero_coefficients:  u64,
    pub dequant_samples:               u64,
    pub dequant_ticks:                 u64,
    pub coefficient_extent:            [u64; 65],
    pub upsample_none_calls:           u64,
    pub upsample_horizontal_calls:     u64,
    pub upsample_vertical_calls:       u64,
    pub upsample_hv_calls:             u64,
    pub upsample_ticks:                u64,
    pub color_convert_calls:           u64,
    pub color_convert_ticks:           u64,
    pub post_process_ticks:            u64,
    pub progressive_reconstruct_ticks: u64,
    pub progressive_transform_ticks:   u64,
    pub checkpoint_creates:            u64,
    pub checkpoint_restores:           u64,
    pub checkpoint_ticks:              u64,
    pub restart_markers:               u64,
    pub cancellation_polls:            u64,
    pub raw_coeff_bytes:               u64,
    pub raw_output_bytes:              u64,
    pub raw_copy_ticks:                u64,
    pub upsample_scratch_bytes:        u64,
    pub upsample_destination_bytes:    u64,
    pub color_input_bytes:             u64,
    pub packed_output_bytes:           u64,
    pub progressive_init_bytes:        u64,
    pub progressive_copy_bytes:        u64,
    pub capacity_growths:              u64,
    pub idct_pointer_calls:            u64,
    pub upsample_pointer_calls:        u64,
    pub color_pointer_calls:           u64,
    pub pull_calls:                    u64,
    pub scan_count:                    usize,
    pub scans:                         [ProfileScan; PROFILE_SCAN_CAPACITY],
    pub total_nanos:                   u64,
    pub total_ticks:                   u64,
    pub timer_overhead_ticks:          u64,
    started:                           Option<Instant>,
    started_ticks:                     Option<ProfileTick>
}

impl Default for DecodeProfile {
    fn default() -> Self {
        Self {
            enabled:                       false,
            width:                         0,
            height:                        0,
            sampling:                      [(0, 0); MAX_COMPONENTS],
            components:                    0,
            mcu_rows:                      0,
            mcus:                          0,
            blocks_by_component:           [0; MAX_COMPONENTS],
            idct_1x1_calls:                0,
            idct_4x4_calls:                0,
            idct_8x8_calls:                0,
            idct_1x1_samples:              0,
            idct_4x4_samples:              0,
            idct_8x8_samples:              0,
            idct_1x1_ticks:                0,
            idct_4x4_ticks:                0,
            idct_8x8_ticks:                0,
            entropy_samples:               0,
            entropy_ticks:                 0,
            baseline_mcu_ticks:            0,
            sampled_nonzero_coefficients:  0,
            dequant_samples:               0,
            dequant_ticks:                 0,
            coefficient_extent:            [0; 65],
            upsample_none_calls:           0,
            upsample_horizontal_calls:     0,
            upsample_vertical_calls:       0,
            upsample_hv_calls:             0,
            upsample_ticks:                0,
            color_convert_calls:           0,
            color_convert_ticks:           0,
            post_process_ticks:            0,
            progressive_reconstruct_ticks: 0,
            progressive_transform_ticks:   0,
            checkpoint_creates:            0,
            checkpoint_restores:           0,
            checkpoint_ticks:              0,
            restart_markers:               0,
            cancellation_polls:            0,
            raw_coeff_bytes:               0,
            raw_output_bytes:              0,
            raw_copy_ticks:                0,
            upsample_scratch_bytes:        0,
            upsample_destination_bytes:    0,
            color_input_bytes:             0,
            packed_output_bytes:           0,
            progressive_init_bytes:        0,
            progressive_copy_bytes:        0,
            capacity_growths:              0,
            idct_pointer_calls:            0,
            upsample_pointer_calls:        0,
            color_pointer_calls:           0,
            pull_calls:                    0,
            scan_count:                    0,
            scans:                         [ProfileScan::default(); PROFILE_SCAN_CAPACITY],
            total_nanos:                   0,
            total_ticks:                   0,
            timer_overhead_ticks:          0,
            started:                       None,
            started_ticks:                 None
        }
    }
}

impl DecodeProfile {
    #[cfg(feature = "profile-active")]
    pub(crate) fn reset(&mut self, enabled: bool) {
        *self = Self::default();
        self.enabled = enabled;
    }

    #[cfg(feature = "profile-active")]
    pub(crate) fn begin(&mut self) {
        if self.enabled {
            self.timer_overhead_ticks = (0..256).map(|_| elapsed_ticks(tick())).min().unwrap_or(0);
            self.started = Some(Instant::now());
            self.started_ticks = Some(tick());
        }
    }

    #[cfg(feature = "profile-active")]
    pub(crate) fn snapshot(&self) -> Self {
        let mut snapshot = self.clone();
        if let Some(started) = self.started {
            snapshot.total_nanos = wall_nanos(started);
        }
        if let Some(started) = self.started_ticks {
            snapshot.total_ticks = elapsed_ticks(started);
        }
        snapshot.started = None;
        snapshot.started_ticks = None;
        snapshot
    }

    #[cfg(feature = "profile-active")]
    pub(crate) fn sample_kind(count: u64) -> u8 {
        match count.wrapping_mul(0x9e37_79b9_7f4a_7c15) >> 56 {
            0 => 1,
            1 => 2,
            _ => 0
        }
    }

    #[cfg(feature = "profile-active")]
    pub(crate) fn sampled_ticks(&self, started: ProfileTick) -> u64 {
        elapsed_ticks(started).saturating_sub(self.timer_overhead_ticks)
    }

    #[cfg(feature = "profile-active")]
    pub(crate) fn push_scan(&mut self, scan: ProfileScan) {
        if self.scan_count < self.scans.len() {
            self.scans[self.scan_count] = scan;
            self.scan_count += 1;
        }
    }
}

#[cfg(feature = "profile-active")]
pub(crate) fn tick() -> ProfileTick {
    #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
    unsafe {
        _mm_lfence();
        _rdtsc()
    }
    #[cfg(not(any(target_arch = "x86", target_arch = "x86_64")))]
    Instant::now()
}

#[cfg(feature = "profile-active")]
pub(crate) fn elapsed_ticks(started: ProfileTick) -> u64 {
    #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
    {
        let ended = unsafe {
            _mm_lfence();
            _rdtsc()
        };
        ended.wrapping_sub(started)
    }
    #[cfg(not(any(target_arch = "x86", target_arch = "x86_64")))]
    {
        wall_nanos(started)
    }
}

#[cfg(feature = "profile-active")]
fn wall_nanos(started: Instant) -> u64 {
    u64::try_from(started.elapsed().as_nanos()).unwrap_or(u64::MAX)
}
