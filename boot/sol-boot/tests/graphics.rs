#![allow(clippy::expect_used)]

use sol_boot::{
    EdidError, GraphicsDecision, GraphicsMode, PreferredResolution, edid_preferred_mode,
    render_boot_frame, select_graphics_mode,
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
    assert!(render_boot_frame(usize::MAX, 2).is_none());
    assert!(render_boot_frame(0, 1080).is_none());
    for (width, height) in [(640, 480), (1920, 1080), (1920, 1200), (2160, 1440)] {
        let frame = render_boot_frame(width, height).expect("frame");
        assert_eq!(frame.len(), width * height);
        assert!(frame.iter().all(|pixel| pixel.reserved == 0));
    }
}
