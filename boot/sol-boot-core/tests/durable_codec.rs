#![allow(clippy::expect_used)]

use sol_boot_core::{
    BOOT_STATE_FORMAT_V1, BOOT_STATE_V1_SIZE, BOOT_SUCCESS_FORMAT_V1, BOOT_SUCCESS_V1_SIZE,
    BootObservation, BootPlan, BootState, BootSuccessReport, CodecError, DeploymentId,
    DeploymentSlot, DurableBootState, DurableStateCopy, ValidatedDeployments, prepare_boot,
    select_redundant_state,
};

fn deployment(slot: DeploymentSlot, generation: u64) -> DeploymentId {
    DeploymentId::new(slot, generation).expect("deployment identity")
}

fn pending_trial() -> (DurableBootState, sol_boot_core::PendingTrialBoot) {
    let known_good = deployment(DeploymentSlot::A, 7);
    let trial = deployment(DeploymentSlot::B, 8);
    let state = BootState::new(known_good)
        .stage_trial(trial, 3)
        .expect("stage trial");
    let envelope = DurableBootState::new(state).expect("initial envelope");
    let observations =
        ValidatedDeployments::new(Some(known_good), Some(trial)).expect("validated deployments");
    let BootPlan::PersistTrial(pending) = prepare_boot(
        &state,
        BootObservation {
            deployments: observations,
            recovery_requested: false,
        },
    ) else {
        panic!("trial must require persistence");
    };
    (envelope, pending)
}

#[test]
fn state_and_success_formats_round_trip_exactly() {
    let (initial, pending) = pending_trial();
    let consumed = initial
        .advance(pending.next_state())
        .expect("advance durable state");
    let state_bytes = consumed.canonical_bytes();

    assert_eq!(BOOT_STATE_FORMAT_V1, 1);
    assert_eq!(state_bytes.len(), BOOT_STATE_V1_SIZE);
    assert_eq!(
        DurableBootState::from_canonical_bytes(&state_bytes).expect("decode state"),
        consumed
    );

    let report = BootSuccessReport {
        deployment: pending.deployment(),
        attempt: pending.attempt(),
    };
    let report_bytes = report.canonical_bytes();
    assert_eq!(BOOT_SUCCESS_FORMAT_V1, 1);
    assert_eq!(report_bytes.len(), BOOT_SUCCESS_V1_SIZE);
    assert_eq!(
        BootSuccessReport::from_canonical_bytes(&report_bytes).expect("decode report"),
        report
    );
}

#[test]
fn versioned_golden_fixtures_remain_byte_stable() {
    let (initial, pending) = pending_trial();
    let consumed = initial
        .advance(pending.next_state())
        .expect("advance durable state");
    let state_fixture =
        decode_hex::<BOOT_STATE_V1_SIZE>(include_str!("fixtures/boot-state-v1.hex"));
    assert_eq!(consumed.canonical_bytes(), state_fixture);
    assert_eq!(
        DurableBootState::from_canonical_bytes(&state_fixture).expect("state fixture"),
        consumed
    );

    let report = BootSuccessReport {
        deployment: pending.deployment(),
        attempt: pending.attempt(),
    };
    let report_fixture =
        decode_hex::<BOOT_SUCCESS_V1_SIZE>(include_str!("fixtures/boot-success-v1.hex"));
    assert_eq!(report.canonical_bytes(), report_fixture);
    assert_eq!(
        BootSuccessReport::from_canonical_bytes(&report_fixture).expect("report fixture"),
        report
    );
}

#[test]
fn every_deployment_lifecycle_state_round_trips() {
    let known_good = deployment(DeploymentSlot::A, 40);
    let trial = deployment(DeploymentSlot::B, 41);
    let initial_state = BootState::new(known_good);
    let initial = DurableBootState::new(initial_state).expect("initial envelope");
    assert_round_trip(initial);

    let staged_state = initial_state
        .stage_trial(trial, 2)
        .expect("staged trial state");
    let staged = initial.advance(staged_state).expect("staged envelope");
    assert_round_trip(staged);

    let observations =
        ValidatedDeployments::new(Some(known_good), Some(trial)).expect("validated deployments");
    let BootPlan::PersistTrial(pending) = prepare_boot(
        &staged_state,
        BootObservation {
            deployments: observations,
            recovery_requested: false,
        },
    ) else {
        panic!("trial plan");
    };
    let consumed = staged
        .advance(pending.next_state())
        .expect("consumed envelope");
    assert_round_trip(consumed);

    let promoted_state = consumed
        .state()
        .apply_success_report(BootSuccessReport {
            deployment: pending.deployment(),
            attempt: pending.attempt(),
        })
        .expect("promoted state");
    assert_round_trip(consumed.advance(promoted_state).expect("promoted envelope"));
}

#[test]
fn every_single_byte_corruption_is_detected() {
    let (initial, pending) = pending_trial();
    let state = initial
        .advance(pending.next_state())
        .expect("advance durable state");
    let original = state.canonical_bytes();
    for index in 0..original.len() {
        let mut corrupted = original;
        corrupted[index] ^= 0x80;
        assert!(
            DurableBootState::from_canonical_bytes(&corrupted).is_err(),
            "state byte {index} was not protected"
        );
    }

    let report = BootSuccessReport {
        deployment: pending.deployment(),
        attempt: pending.attempt(),
    };
    let original = report.canonical_bytes();
    for index in 0..original.len() {
        let mut corrupted = original;
        corrupted[index] ^= 0x40;
        assert!(
            BootSuccessReport::from_canonical_bytes(&corrupted).is_err(),
            "report byte {index} was not protected"
        );
    }
}

#[test]
fn reserved_bytes_and_unknown_versions_are_rejected_even_with_valid_crc() {
    let (envelope, _) = pending_trial();
    let mut reserved = envelope.canonical_bytes();
    reserved[12] = 1;
    rewrite_state_checksum(&mut reserved);
    assert_eq!(
        DurableBootState::from_canonical_bytes(&reserved),
        Err(CodecError::NonCanonical)
    );

    let mut unsupported = envelope.canonical_bytes();
    unsupported[8..10].copy_from_slice(&2_u16.to_le_bytes());
    rewrite_state_checksum(&mut unsupported);
    assert_eq!(
        DurableBootState::from_canonical_bytes(&unsupported),
        Err(CodecError::UnsupportedVersion(2))
    );

    let mut exhausted = envelope.canonical_bytes();
    exhausted[16..24].copy_from_slice(&u64::MAX.to_le_bytes());
    rewrite_state_checksum(&mut exhausted);
    let exhausted =
        DurableBootState::from_canonical_bytes(&exhausted).expect("maximum sequence envelope");
    assert_eq!(
        exhausted.advance(exhausted.state()),
        Err(CodecError::SequenceExhausted)
    );
}

#[test]
fn redundant_selection_uses_newest_valid_copy_and_rejects_split_brain() {
    let (initial, pending) = pending_trial();
    let newer = initial
        .advance(pending.next_state())
        .expect("advance durable state");
    let old_bytes = initial.canonical_bytes();
    let new_bytes = newer.canonical_bytes();

    let selected =
        select_redundant_state(Some(&old_bytes), Some(&new_bytes)).expect("select newest state");
    assert_eq!(selected.copy(), DurableStateCopy::B);
    assert_eq!(selected.envelope(), newer);

    let mut corrupt = new_bytes;
    corrupt[40] ^= 1;
    let selected =
        select_redundant_state(Some(&old_bytes), Some(&corrupt)).expect("use valid fallback");
    assert_eq!(selected.copy(), DurableStateCopy::A);
    assert_eq!(selected.envelope(), initial);

    let other_state = DurableBootState::new(BootState::new(deployment(DeploymentSlot::B, 99)))
        .expect("conflicting envelope");
    assert_eq!(
        select_redundant_state(
            Some(&initial.canonical_bytes()),
            Some(&other_state.canonical_bytes())
        ),
        Err(CodecError::ConflictingSequence)
    );
    assert_eq!(
        select_redundant_state(None, None),
        Err(CodecError::NoValidStateCopies)
    );
}

fn rewrite_state_checksum(bytes: &mut [u8; BOOT_STATE_V1_SIZE]) {
    let checksum = crc32fast::hash(&bytes[..BOOT_STATE_V1_SIZE - 4]);
    bytes[BOOT_STATE_V1_SIZE - 4..].copy_from_slice(&checksum.to_le_bytes());
}

fn decode_hex<const N: usize>(source: &str) -> [u8; N] {
    let source = source.trim();
    assert_eq!(source.len(), N * 2, "fixture hex length");
    let mut bytes = [0_u8; N];
    for (index, output) in bytes.iter_mut().enumerate() {
        let offset = index * 2;
        *output = u8::from_str_radix(&source[offset..offset + 2], 16).expect("fixture hex byte");
    }
    bytes
}

fn assert_round_trip(envelope: DurableBootState) {
    let bytes = envelope.canonical_bytes();
    assert_eq!(
        DurableBootState::from_canonical_bytes(&bytes).expect("lifecycle state"),
        envelope
    );
}
