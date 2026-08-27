#![allow(clippy::expect_used)]

use sol_boot::{
    EdidError, GraphicsDecision, GraphicsMode, PreferredResolution, SplashProgress,
    edid_preferred_mode, redraw_boot_frame, render_boot_frame, select_graphics_mode,
};

#[test]
fn preferred_mode_is_selected_at_most_once_and_current_is_preserved() {
    let modes = [
        GraphicsMode {
            index: 0,
            width: 1024,
            height: 768,
            stride: 1024,
        },
        GraphicsMode {
            index: 1,
            width: 1920,
            height: 1080,
            stride: 2048,
        },
    ];
    let preferred = Some(PreferredResolution {
        width: 1920,
        height: 1080,
    });
    assert_eq!(
        select_graphics_mode(&modes, 0, preferred),
        GraphicsDecision::SetOnce(modes[1])
    );
    assert_eq!(
        select_graphics_mode(&modes, 1, preferred),
        GraphicsDecision::Preserve(modes[1])
    );
    assert_eq!(
        select_graphics_mode(&modes, 0, None),
        GraphicsDecision::Preserve(modes[0])
    );
}

#[test]
fn invalid_or_absent_preferred_mode_never_guesses_largest() {
    let modes = [GraphicsMode {
        index: 4,
        width: 1280,
        height: 800,
        stride: 1280,
    }];
    assert_eq!(
        select_graphics_mode(
            &modes,
            4,
            Some(PreferredResolution {
                width: 2560,
                height: 1600,
            })
        ),
        GraphicsDecision::Preserve(modes[0])
    );
}

#[test]
fn edid_preferred_timing_is_checksum_validated() {
    let mut edid = [0_u8; 128];
    edid[..8].copy_from_slice(&[0x00, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0x00]);
    edid[54] = 1;
    edid[56] = 0x80;
    edid[58] = 0x70;
    edid[59] = 0x38;
    edid[61] = 0x40;
    let checksum = edid[..127]
        .iter()
        .fold(0_u8, |sum, byte| sum.wrapping_add(*byte));
    edid[127] = 0_u8.wrapping_sub(checksum);
    assert_eq!(
        edid_preferred_mode(&edid),
        Ok(PreferredResolution {
            width: 1920,
            height: 1080,
        })
    );
    edid[20] ^= 1;
    assert_eq!(edid_preferred_mode(&edid), Err(EdidError::Invalid));
}

#[test]
fn renderer_rejects_overflow_and_bounds_every_fixture() {
    assert!(render_boot_frame(usize::MAX, 2, SplashProgress::Hidden).is_none());
    assert!(render_boot_frame(0, 1080, SplashProgress::Hidden).is_none());
    for (width, height) in [(640, 480), (1920, 1080), (1920, 1200), (2160, 1440)] {
        let frame =
            render_boot_frame(width, height, SplashProgress::Fraction(0.35)).expect("frame");
        assert_eq!(frame.len(), width * height);
        assert!(frame.iter().all(|pixel| pixel.reserved == 0));
    }
}

const fn pixel(red: u8, green: u8, blue: u8) -> sol_boot::BootPixel {
    sol_boot::BootPixel {
        blue,
        green,
        red,
        reserved: 0,
    }
}

const BLACK: sol_boot::BootPixel = pixel(0, 0, 0);
const WHITE: sol_boot::BootPixel = pixel(255, 255, 255);
const TRACK_COLOR: sol_boot::BootPixel = pixel(60, 60, 67);

fn count(frame: &[sol_boot::BootPixel], expected: sol_boot::BootPixel) -> usize {
    frame
        .iter()
        .filter(|pixel| {
            pixel.red == expected.red
                && pixel.green == expected.green
                && pixel.blue == expected.blue
        })
        .count()
}

fn luminance_sum(frame: &[sol_boot::BootPixel]) -> u64 {
    frame
        .iter()
        .map(|pixel| u64::from(pixel.red) + u64::from(pixel.green) + u64::from(pixel.blue))
        .sum()
}

/// Ink threshold above the dimmest anti-alias ramp step.
fn is_ink(pixel: sol_boot::BootPixel) -> bool {
    u16::from(pixel.red) + u16::from(pixel.green) + u16::from(pixel.blue) > 72
}

/// Disc-core coordinates mirroring the renderer's placement math exactly, so
/// probes land on pixels whose coverage is unambiguous.
#[allow(clippy::cast_possible_truncation)]
fn mark_core(width: usize, height: usize) -> (usize, usize) {
    let size = (width.min(height)) * 17 / 100;
    let origin_x = width / 2 - size / 2;
    let origin_y = height * 46 / 100 - size / 2;
    (origin_x + 256 * size / 512, origin_y + 256 * size / 512)
}

#[test]
fn brand_mark_centers_with_eclipse_notch_on_the_right() {
    const WIDTH: usize = 1280;
    const HEIGHT: usize = 720;
    let frame = render_boot_frame(WIDTH, HEIGHT, SplashProgress::Hidden).expect("frame");
    let (core_x, core_y) = mark_core(WIDTH, HEIGHT);

    // The disc interior stays pure brand ink far from every anti-aliased rim.
    assert_eq!(
        frame[core_y * WIDTH + core_x],
        WHITE,
        "expected solid ink at disc core"
    );

    // Scanning one row band through the silhouette reveals the classic
    // eclipse profile: ink, background notch offset right, thin crescent.
    let start = core_x.saturating_sub(core_x / 2);
    let end = WIDTH - 1;
    let mut runs: Vec<(bool, u32)> = Vec::new();
    for x in start..=end {
        let lit = is_ink(frame[core_y * WIDTH + x]);
        match runs.last_mut() {
            Some((state, length)) if *state == lit => *length += 1,
            _ => runs.push((lit, 1)),
        }
    }
    assert_eq!(runs.len(), 5, "unexpected silhouette profile: {runs:?}");
    assert!(runs.iter().all(|(_, length)| *length >= 2), "{runs:?}");
    assert!(!runs[0].0 && runs[1].0 && !runs[2].0 && runs[3].0 && !runs[4].0);
    // Locate the single interior dark gap between the two ink runs.
    #[allow(clippy::cast_possible_truncation)]
    let run_length = |index: usize| runs[index].1 as usize;
    let gap_start = start + run_length(0) + run_length(1);
    let gap_end = gap_start + run_length(2);
    assert!(
        gap_start >= WIDTH / 2 && gap_end <= WIDTH * 3 / 4,
        "notch misplaced"
    );
}

#[test]
fn progress_capsule_tracks_stages_and_fills_monotonically() {
    const WIDTH: usize = 1024;
    const HEIGHT: usize = 600;
    let hidden = render_boot_frame(WIDTH, HEIGHT, SplashProgress::Hidden).expect("hidden");
    let empty = render_boot_frame(WIDTH, HEIGHT, SplashProgress::Fraction(0.0)).expect("empty");
    let quarter = render_boot_frame(WIDTH, HEIGHT, SplashProgress::Fraction(0.25)).expect("q");
    let three_quarters =
        render_boot_frame(WIDTH, HEIGHT, SplashProgress::Fraction(0.75)).expect("tq");
    let full = render_boot_frame(WIDTH, HEIGHT, SplashProgress::Fraction(1.0)).expect("full");

    // Hidden draws no rail; the rail exists before any fill lands.
    assert_eq!(count(&hidden, TRACK_COLOR), 0);
    assert!(count(&empty, TRACK_COLOR) > 0);

    // The fill is white, same as the logo ink, so count fill pixels by
    // comparing frames: empty should have fewer white pixels than quarter.
    let white_in_empty = count(&empty, WHITE);
    let white_in_quarter = count(&quarter, WHITE);
    let white_in_three_quarters = count(&three_quarters, WHITE);
    let white_in_full = count(&full, WHITE);

    // Fill grows monotonically
    assert!(white_in_empty < white_in_quarter);
    assert!(white_in_quarter < white_in_three_quarters);
    assert!(white_in_three_quarters <= white_in_full);
    assert!(luminance_sum(&empty) <= luminance_sum(&quarter));
    assert!(luminance_sum(&quarter) < luminance_sum(&three_quarters));
    assert!(luminance_sum(&three_quarters) <= luminance_sum(&full));
}

#[test]
fn progress_fractions_clamp_into_the_closed_unit_interval() {
    let negative = render_boot_frame(800, 500, SplashProgress::Fraction(-0.4)).expect("frame");
    let zero = render_boot_frame(800, 500, SplashProgress::Fraction(0.0)).expect("frame");
    let overshoot = render_boot_frame(800, 500, SplashProgress::Fraction(1.9)).expect("frame");
    let full = render_boot_frame(800, 500, SplashProgress::Fraction(1.0)).expect("frame");
    let nan = render_boot_frame(800, 500, SplashProgress::Fraction(f32::NAN)).expect("frame");
    assert_eq!(negative, zero);
    assert_eq!(overshoot, full);
    assert_eq!(nan, zero);
}

#[test]
fn panels_too_small_for_the_mark_degrade_to_a_clean_black_field() {
    let frame = render_boot_frame(12, 12, SplashProgress::Fraction(1.0)).expect("frame");
    assert!(frame.iter().all(|pixel| *pixel == BLACK));
}

#[test]
fn redraw_matches_allocating_variant_and_guards_mismatched_buffers() {
    const WIDTH: usize = 960;
    const HEIGHT: usize = 540;
    let reference =
        render_boot_frame(WIDTH, HEIGHT, SplashProgress::Fraction(0.6)).expect("reference");
    let mut reused = vec![BLACK; WIDTH * HEIGHT];
    redraw_boot_frame(&mut reused, WIDTH, HEIGHT, SplashProgress::Fraction(0.6))
        .expect("compatible buffer");
    assert_eq!(reused, reference);

    let mut guarded = vec![BLACK; WIDTH * HEIGHT];
    assert!(
        redraw_boot_frame(
            &mut guarded,
            WIDTH + 1,
            HEIGHT,
            SplashProgress::Fraction(0.6)
        )
        .is_none()
    );
    assert!(guarded.iter().all(|pixel| *pixel == BLACK));
    assert!(
        render_boot_frame(sol_boot::MAX_FRAME_EDGE + 1, 4, SplashProgress::Hidden).is_none(),
        "frames beyond the supported edge bound must be rejected"
    );
}

// Every asserted point evaluates over dyadic rationals (exact in `f32`), and
// loop steps stay far below the mantissa limit, so precise equality is sound.
#[allow(
    clippy::float_cmp,
    clippy::cast_precision_loss,
    reason = "curve samples here are chosen to be exactly representable"
)]
#[test]
fn ease_curve_hits_exact_endpoints_and_rises_monotonically() {
    assert_eq!(sol_boot::ease_out_cubic(0.0), 0.0);
    assert_eq!(sol_boot::ease_out_cubic(0.25), 0.578_125);
    assert_eq!(sol_boot::ease_out_cubic(0.5), 0.875);
    assert_eq!(sol_boot::ease_out_cubic(0.75), 0.984_375);
    assert_eq!(sol_boot::ease_out_cubic(1.0), 1.0);

    let mut previous = -0.0;
    for step in 0..=100 {
        let progress = step as f32 / 100.0;
        let eased = sol_boot::ease_out_cubic(progress);
        assert!(
            eased >= previous && (0.0..=1.0).contains(&eased),
            "curve regressed at {progress}: {previous} -> {eased}"
        );
        previous = eased;
    }

    // Non-finite and out-of-range inputs land on endpoints instead of NaN.
    assert_eq!(sol_boot::ease_out_cubic(f32::NAN), 0.0);
    assert_eq!(sol_boot::ease_out_cubic(f32::INFINITY), 1.0);
    assert_eq!(sol_boot::ease_out_cubic(-3.5), 0.0);
    assert_eq!(sol_boot::ease_out_cubic(9.25), 1.0);
}
