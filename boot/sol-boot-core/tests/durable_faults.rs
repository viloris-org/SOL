#![allow(clippy::expect_used)]

use sol_boot_core::{
    BOOT_STATE_V1_SIZE, BootAction, BootObservation, BootPlan, BootState, DeploymentId,
    DeploymentRecord, DeploymentSlot, DeploymentStatus, DurableBootState, DurableStateCopy,
    PendingTrialBoot, ValidatedDeployments, prepare_boot, select_redundant_state,
};

#[derive(Debug, Clone, Copy)]
enum Fault {
    BeforeWrite,
    TornWrite(usize),
    BeforeSync,
    AfterSync,
    ReadbackFailure,
    None,
}

#[derive(Clone)]
struct RedundantStore {
    copies: [Option<[u8; BOOT_STATE_V1_SIZE]>; 2],
}

impl RedundantStore {
    fn new(envelope: DurableBootState) -> Self {
        let bytes = envelope.canonical_bytes();
        Self {
            copies: [Some(bytes), Some(bytes)],
        }
    }

    fn load(&self) -> sol_boot_core::SelectedDurableState {
        let copy_a = self.copies[0].as_ref().map(|bytes| &bytes[..]);
        let copy_b = self.copies[1].as_ref().map(|bytes| &bytes[..]);
        select_redundant_state(copy_a, copy_b).expect("at least one valid durable copy")
    }

    const fn write(&mut self, copy: DurableStateCopy, bytes: [u8; BOOT_STATE_V1_SIZE]) {
        self.copies[copy_index(copy)] = Some(bytes);
    }

    fn persist_trial(&mut self, pending: PendingTrialBoot, fault: Fault) -> Option<BootAction> {
        let selected = self.load();
        let next = selected
            .envelope()
            .advance(pending.next_state())
            .expect("advance envelope");
        let bytes = next.canonical_bytes();
        let target = selected.copy().other();

        match fault {
            Fault::BeforeWrite | Fault::BeforeSync => None,
            Fault::TornWrite(length) => {
                let target_index = copy_index(target);
                let mut torn = self.copies[target_index].unwrap_or([0; BOOT_STATE_V1_SIZE]);
                torn[..length].copy_from_slice(&bytes[..length]);
                self.copies[target_index] = Some(torn);
                None
            }
            Fault::AfterSync | Fault::ReadbackFailure => {
                self.write(target, bytes);
                None
            }
            Fault::None => {
                self.write(target, bytes);
                let verified = self.load();
                assert_eq!(
                    verified.envelope(),
                    next,
                    "read-back must select exact state"
                );
                Some(pending.confirm_persisted())
            }
        }
    }
}

#[test]
fn power_loss_at_every_attempt_commit_boundary_retains_a_safe_state() {
    for fault in [
        Fault::BeforeWrite,
        Fault::BeforeSync,
        Fault::AfterSync,
        Fault::ReadbackFailure,
    ] {
        assert_fault_is_safe(fault);
    }
    for written in 0..=BOOT_STATE_V1_SIZE {
        assert_fault_is_safe(Fault::TornWrite(written));
    }
}

#[test]
fn trial_action_is_returned_only_after_exact_readback() {
    let (mut store, pending, _, trial) = fixture();
    assert_eq!(
        store.persist_trial(pending, Fault::None),
        Some(BootAction::BootTrial {
            deployment: trial,
            attempt: pending.attempt(),
        })
    );
    assert_eq!(store.load().envelope().sequence(), 2);
}

fn assert_fault_is_safe(fault: Fault) {
    let (mut store, pending, known_good, trial) = fixture();
    assert_eq!(store.persist_trial(pending, fault), None);

    let recovered = store.load().envelope();
    assert!(matches!(
        recovered
            .state()
            .record(DeploymentSlot::A)
            .map(DeploymentRecord::status),
        Some(DeploymentStatus::KnownGood)
    ));
    assert!(matches!(recovered.sequence(), 1 | 2));

    let observations =
        ValidatedDeployments::new(Some(known_good), Some(trial)).expect("validated deployments");
    match prepare_boot(
        &recovered.state(),
        BootObservation {
            deployments: observations,
            recovery_requested: false,
        },
    ) {
        BootPlan::PersistTrial(next) => {
            let expected = if recovered.sequence() == 1 { 1 } else { 2 };
            assert_eq!(next.attempt().get(), expected);
        }
        BootPlan::Ready(BootAction::BootKnownGood(id)) => assert_eq!(id, known_good),
        BootPlan::Ready(other) => panic!("fault recovered to unsafe plan: {other:?}"),
    }
}

fn fixture() -> (RedundantStore, PendingTrialBoot, DeploymentId, DeploymentId) {
    let known_good = DeploymentId::new(DeploymentSlot::A, 30).expect("known-good identity");
    let trial = DeploymentId::new(DeploymentSlot::B, 31).expect("trial identity");
    let state = BootState::new(known_good)
        .stage_trial(trial, 2)
        .expect("stage trial");
    let store = RedundantStore::new(DurableBootState::new(state).expect("durable state"));
    let observations =
        ValidatedDeployments::new(Some(known_good), Some(trial)).expect("validated deployments");
    let BootPlan::PersistTrial(pending) = prepare_boot(
        &state,
        BootObservation {
            deployments: observations,
            recovery_requested: false,
        },
    ) else {
        panic!("trial plan");
    };
    (store, pending, known_good, trial)
}

const fn copy_index(copy: DurableStateCopy) -> usize {
    match copy {
        DurableStateCopy::A => 0,
        DurableStateCopy::B => 1,
    }
}
