#![allow(clippy::expect_used)]

use sol_boot_core::{
    AttemptId, BootAction, BootObservation, BootPlan, BootState, BootSuccessReport, DeploymentId,
    DeploymentRecord, DeploymentSlot, DeploymentStatus, RecoveryReason, ReportError, StateError,
    ValidatedDeployments, prepare_boot,
};

fn deployment(slot: DeploymentSlot, generation: u64) -> DeploymentId {
    DeploymentId::new(slot, generation).expect("deployment identity")
}

fn observations(ids: &[DeploymentId]) -> ValidatedDeployments {
    let mut slot_a = None;
    let mut slot_b = None;
    for id in ids {
        match id.slot() {
            DeploymentSlot::A => slot_a = Some(*id),
            DeploymentSlot::B => slot_b = Some(*id),
        }
    }
    ValidatedDeployments::new(slot_a, slot_b).expect("validated observations")
}

fn plan(state: &BootState, ids: &[DeploymentId]) -> BootPlan {
    prepare_boot(
        state,
        BootObservation {
            deployments: observations(ids),
            recovery_requested: false,
        },
    )
}

#[test]
fn trial_attempt_is_exposed_only_after_its_consumed_state() {
    let known_good = deployment(DeploymentSlot::A, 7);
    let trial = deployment(DeploymentSlot::B, 8);
    let state = BootState::new(known_good)
        .stage_trial(trial, 2)
        .expect("stage trial");

    let BootPlan::PersistTrial(pending) = plan(&state, &[known_good, trial]) else {
        panic!("verified trial should require persistence");
    };
    assert_eq!(pending.deployment(), trial);
    assert_eq!(pending.attempt().get(), 1);
    assert_eq!(
        pending
            .next_state()
            .record(DeploymentSlot::B)
            .map(DeploymentRecord::status),
        Some(DeploymentStatus::Trial {
            remaining_attempts: 1,
            pending_attempt: Some(pending.attempt()),
        })
    );
    assert_eq!(
        pending.confirm_persisted(),
        BootAction::BootTrial {
            deployment: trial,
            attempt: pending.attempt(),
        }
    );

    let retry_without_persisting = plan(&state, &[known_good, trial]);
    assert_eq!(retry_without_persisting, BootPlan::PersistTrial(pending));
}

#[test]
fn exact_success_report_promotes_and_replay_is_rejected() {
    let known_good = deployment(DeploymentSlot::A, 7);
    let trial = deployment(DeploymentSlot::B, 8);
    let state = BootState::new(known_good)
        .stage_trial(trial, 2)
        .expect("stage trial");
    assert_eq!(
        state.apply_success_report(BootSuccessReport {
            deployment: trial,
            attempt: AttemptId::new(1).expect("attempt identity"),
        }),
        Err(ReportError::NoPendingTrial)
    );
    let BootPlan::PersistTrial(pending) = plan(&state, &[known_good, trial]) else {
        panic!("trial plan");
    };
    let consumed = pending.next_state();
    let report = BootSuccessReport {
        deployment: trial,
        attempt: pending.attempt(),
    };
    let promoted = consumed
        .apply_success_report(report)
        .expect("promote trial");

    assert_eq!(promoted.preferred_slot(), DeploymentSlot::B);
    assert_eq!(
        promoted
            .record(DeploymentSlot::B)
            .map(DeploymentRecord::status),
        Some(DeploymentStatus::KnownGood)
    );
    assert_eq!(
        promoted.apply_success_report(report),
        Err(ReportError::NoPendingTrial)
    );
    assert_eq!(
        plan(&promoted, &[known_good, trial]),
        BootPlan::Ready(BootAction::BootKnownGood(trial))
    );
}

#[test]
fn stale_generation_and_attempt_reports_cannot_promote() {
    let known_good = deployment(DeploymentSlot::A, 10);
    let trial = deployment(DeploymentSlot::B, 11);
    let state = BootState::new(known_good)
        .stage_trial(trial, 2)
        .expect("stage trial");
    let BootPlan::PersistTrial(first) = plan(&state, &[known_good, trial]) else {
        panic!("first trial");
    };
    let BootPlan::PersistTrial(second) = plan(&first.next_state(), &[known_good, trial]) else {
        panic!("second trial");
    };

    assert_eq!(
        second.next_state().apply_success_report(BootSuccessReport {
            deployment: deployment(DeploymentSlot::B, 9),
            attempt: second.attempt(),
        }),
        Err(ReportError::DeploymentMismatch)
    );
    assert_eq!(
        second.next_state().apply_success_report(BootSuccessReport {
            deployment: trial,
            attempt: first.attempt(),
        }),
        Err(ReportError::AttemptMismatch)
    );
}

#[test]
fn exhausted_or_unverified_trial_falls_back_without_consuming_an_attempt() {
    let known_good = deployment(DeploymentSlot::A, 4);
    let trial = deployment(DeploymentSlot::B, 5);
    let state = BootState::new(known_good)
        .stage_trial(trial, 1)
        .expect("stage trial");

    assert_eq!(
        plan(&state, &[known_good]),
        BootPlan::Ready(BootAction::BootKnownGood(known_good))
    );

    let BootPlan::PersistTrial(first) = plan(&state, &[known_good, trial]) else {
        panic!("trial plan");
    };
    assert_eq!(
        plan(&first.next_state(), &[known_good, trial]),
        BootPlan::Ready(BootAction::BootKnownGood(known_good))
    );
}

#[test]
fn every_configured_trial_budget_is_bounded_exactly() {
    for budget in 1..=4 {
        let known_good = deployment(DeploymentSlot::A, 20);
        let trial = deployment(DeploymentSlot::B, 21);
        let mut state = BootState::new(known_good)
            .stage_trial(trial, budget)
            .expect("stage trial");

        for expected_attempt in 1..=u64::from(budget) {
            let BootPlan::PersistTrial(pending) = plan(&state, &[known_good, trial]) else {
                panic!("available trial budget should be consumed");
            };
            assert_eq!(pending.attempt().get(), expected_attempt);
            state = pending.next_state();
        }

        assert_eq!(
            plan(&state, &[known_good, trial]),
            BootPlan::Ready(BootAction::BootKnownGood(known_good))
        );
    }
}

#[test]
fn recovery_request_and_missing_fallback_do_not_mutate_state() {
    let known_good = deployment(DeploymentSlot::A, 1);
    let state = BootState::new(known_good);
    let observation = BootObservation {
        deployments: observations(&[known_good]),
        recovery_requested: true,
    };
    assert_eq!(
        prepare_boot(&state, observation),
        BootPlan::Ready(BootAction::Recovery(RecoveryReason::Requested))
    );
    assert_eq!(
        plan(&state, &[]),
        BootPlan::Ready(BootAction::Recovery(RecoveryReason::NoVerifiedKnownGood))
    );
}

#[test]
fn staging_preserves_a_monotonic_known_good_fallback() {
    let known_good = deployment(DeploymentSlot::A, 7);
    let state = BootState::new(known_good);

    assert_eq!(
        state.stage_trial(deployment(DeploymentSlot::A, 8), 2),
        Err(StateError::NoRetainedKnownGood)
    );
    assert_eq!(
        state.stage_trial(deployment(DeploymentSlot::B, 8), 0),
        Err(StateError::ZeroAttempts)
    );

    let trial = deployment(DeploymentSlot::B, 8);
    let state = state.stage_trial(trial, 1).expect("stage trial");
    assert_eq!(
        state.stage_trial(deployment(DeploymentSlot::B, 8), 1),
        Err(StateError::NonMonotonicGeneration)
    );
}

#[test]
fn validated_observations_enforce_physical_slot_identity() {
    assert_eq!(AttemptId::new(0), Err(StateError::ZeroAttemptIdentity));
    assert_eq!(
        ValidatedDeployments::new(Some(deployment(DeploymentSlot::B, 1)), None),
        Err(StateError::ObservationSlotMismatch)
    );
}
