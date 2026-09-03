#![no_std]
//! Firmware-independent SOL boot selection and trial policy.
//!
//! All observations accepted here have already passed manifest, signature, and
//! artifact verification. Firmware adapters own I/O and must persist the state
//! from [`PendingTrialBoot::next_state`] before calling
//! [`PendingTrialBoot::confirm_persisted`] and starting the selected UKI.

use core::error::Error;
use core::fmt;

mod auth;
mod codec;
mod deployment;
mod rollback;

pub use auth::{
    ATTEMPT_NONCE_SIZE, AUTH_TAG_SIZE, AuthError, AuthenticatedBootState, AuthenticatedStorage,
    AuthenticatedSuccessReport, HealthCheckpoints, SoftwareAuthenticatedStorage,
};
#[cfg(feature = "tpm")]
pub use auth::{TpmAuthenticatedStorage, TpmError};
pub use codec::{
    BOOT_STATE_FORMAT_V1, BOOT_STATE_V1_SIZE, BOOT_SUCCESS_FORMAT_V1, BOOT_SUCCESS_V1_SIZE,
    CodecError, DurableBootState, DurableStateCopy, SelectedDurableState, select_redundant_state,
};
pub use deployment::{
    ArtifactBinding, DEPLOYMENT_FORMAT_V1, DEPLOYMENT_SIGNED_V1_SIZE, DEPLOYMENT_V1_PAYLOAD_SIZE,
    DeploymentDescriptor, DeploymentDescriptorError, SignedDeploymentDescriptor,
};
pub use rollback::{RollbackProtection, SecurityPolicy};

/// Physical A/B system deployment slot.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeploymentSlot {
    /// Deployment slot A.
    A,
    /// Deployment slot B.
    B,
}

impl DeploymentSlot {
    /// Returns the other physical deployment slot.
    #[must_use]
    pub const fn other(self) -> Self {
        match self {
            Self::A => Self::B,
            Self::B => Self::A,
        }
    }
}

/// Independently addressable signed boot-authority copy.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BootAuthorityCopy {
    /// Boot-authority copy A.
    A,
    /// Boot-authority copy B.
    B,
}

/// Independently addressable non-graphical recovery copy.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RecoveryCopy {
    /// Recovery copy A.
    A,
    /// Recovery copy B.
    B,
}

/// Exact identity of one installed deployment.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DeploymentId {
    slot: DeploymentSlot,
    generation: u64,
}

impl DeploymentId {
    /// Creates a slot-bound, non-zero generation identity.
    ///
    /// # Errors
    ///
    /// Returns [`StateError::ZeroGeneration`] for generation zero.
    pub const fn new(slot: DeploymentSlot, generation: u64) -> Result<Self, StateError> {
        if generation == 0 {
            return Err(StateError::ZeroGeneration);
        }
        Ok(Self { slot, generation })
    }

    /// Returns the physical deployment slot.
    #[must_use]
    pub const fn slot(self) -> DeploymentSlot {
        self.slot
    }

    /// Returns the monotonic slot generation.
    #[must_use]
    pub const fn generation(self) -> u64 {
        self.generation
    }
}

/// Identity assigned to exactly one durably consumed trial attempt.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct AttemptId(u64);

impl AttemptId {
    /// Parses a non-zero durable/report attempt identity.
    ///
    /// # Errors
    ///
    /// Returns [`StateError::ZeroAttemptIdentity`] for zero.
    pub const fn new(value: u64) -> Result<Self, StateError> {
        if value == 0 {
            return Err(StateError::ZeroAttemptIdentity);
        }
        Ok(Self(value))
    }

    /// Returns the non-zero numeric attempt identity.
    #[must_use]
    pub const fn get(self) -> u64 {
        self.0
    }
}

/// Durable lifecycle state of an installed deployment.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeploymentStatus {
    /// The userspace health gate has promoted this deployment.
    KnownGood,
    /// A staged deployment is undergoing a bounded boot trial.
    Trial {
        /// Attempts that can still be durably consumed.
        remaining_attempts: u8,
        /// Most recently consumed attempt, if any.
        pending_attempt: Option<AttemptId>,
    },
}

/// Durable record for one physical deployment slot.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DeploymentRecord {
    id: DeploymentId,
    status: DeploymentStatus,
}

impl DeploymentRecord {
    /// Returns the exact slot and generation identity.
    #[must_use]
    pub const fn id(self) -> DeploymentId {
        self.id
    }

    /// Returns the deployment lifecycle state.
    #[must_use]
    pub const fn status(self) -> DeploymentStatus {
        self.status
    }
}

/// Durable A/B deployment policy state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BootState {
    slots: [Option<DeploymentRecord>; 2],
    preferred: DeploymentSlot,
    next_attempt: u64,
}

impl BootState {
    /// Initializes state with one retained known-good deployment.
    #[must_use]
    pub const fn new(known_good: DeploymentId) -> Self {
        let record = DeploymentRecord {
            id: known_good,
            status: DeploymentStatus::KnownGood,
        };
        let slots = match known_good.slot {
            DeploymentSlot::A => [Some(record), None],
            DeploymentSlot::B => [None, Some(record)],
        };
        Self {
            slots,
            preferred: known_good.slot,
            next_attempt: 1,
        }
    }

    /// Stages a bounded trial while retaining a known-good deployment in the
    /// other physical slot.
    ///
    /// # Errors
    ///
    /// Rejects zero attempts, a non-monotonic replacement generation, or a
    /// stage operation that would not retain a known-good opposite slot.
    pub fn stage_trial(
        &self,
        deployment: DeploymentId,
        maximum_attempts: u8,
    ) -> Result<Self, StateError> {
        if maximum_attempts == 0 {
            return Err(StateError::ZeroAttempts);
        }
        let fallback = self.record(deployment.slot.other());
        if !fallback.is_some_and(|record| record.status == DeploymentStatus::KnownGood) {
            return Err(StateError::NoRetainedKnownGood);
        }
        if self
            .record(deployment.slot)
            .is_some_and(|record| record.id.generation >= deployment.generation)
        {
            return Err(StateError::NonMonotonicGeneration);
        }

        let mut next = *self;
        next.set_record(DeploymentRecord {
            id: deployment,
            status: DeploymentStatus::Trial {
                remaining_attempts: maximum_attempts,
                pending_attempt: None,
            },
        });
        next.preferred = deployment.slot;
        Ok(next)
    }

    /// Applies a health report only when slot, generation, and attempt all
    /// match the most recently consumed trial attempt.
    ///
    /// # Errors
    ///
    /// Rejects stale generations, stale attempt identities, reports for a
    /// non-trial deployment, and reports received before an attempt was
    /// durably consumed.
    pub fn apply_success_report(&self, report: BootSuccessReport) -> Result<Self, ReportError> {
        let record = self
            .record(report.deployment.slot)
            .ok_or(ReportError::DeploymentMismatch)?;
        if record.id != report.deployment {
            return Err(ReportError::DeploymentMismatch);
        }
        let DeploymentStatus::Trial {
            pending_attempt, ..
        } = record.status
        else {
            return Err(ReportError::NoPendingTrial);
        };
        match pending_attempt {
            Some(attempt) if attempt == report.attempt => {}
            Some(_) => return Err(ReportError::AttemptMismatch),
            None => return Err(ReportError::NoPendingTrial),
        }

        let mut next = *self;
        next.set_record(DeploymentRecord {
            id: record.id,
            status: DeploymentStatus::KnownGood,
        });
        next.preferred = record.id.slot;
        Ok(next)
    }

    /// Returns the preferred physical deployment slot.
    #[must_use]
    pub const fn preferred_slot(self) -> DeploymentSlot {
        self.preferred
    }

    /// Returns the durable record for a physical slot.
    #[must_use]
    pub const fn record(self, slot: DeploymentSlot) -> Option<DeploymentRecord> {
        self.slots[slot_index(slot)]
    }

    const fn set_record(&mut self, record: DeploymentRecord) {
        self.slots[slot_index(record.id.slot)] = Some(record);
    }

    fn validate(&self) -> Result<(), StateError> {
        if self.next_attempt == 0 {
            return Err(StateError::AttemptSequenceInvalid);
        }
        let preferred = self
            .record(self.preferred)
            .ok_or(StateError::PreferredDeploymentMissing)?;
        let mut known_good_count = 0_u8;
        let mut trial_slot = None;
        for slot in [DeploymentSlot::A, DeploymentSlot::B] {
            if let Some(record) = self.record(slot) {
                match record.status {
                    DeploymentStatus::KnownGood => known_good_count += 1,
                    DeploymentStatus::Trial {
                        pending_attempt, ..
                    } => {
                        if trial_slot.replace(slot).is_some() {
                            return Err(StateError::InvalidTrialLayout);
                        }
                        if pending_attempt.is_some_and(|attempt| attempt.0 >= self.next_attempt) {
                            return Err(StateError::AttemptSequenceInvalid);
                        }
                    }
                }
            }
        }
        if known_good_count == 0 {
            return Err(StateError::NoKnownGood);
        }
        match trial_slot {
            Some(slot)
                if slot == self.preferred
                    && self
                        .record(slot.other())
                        .is_some_and(|record| record.status == DeploymentStatus::KnownGood) =>
            {
                Ok(())
            }
            None if preferred.status == DeploymentStatus::KnownGood => Ok(()),
            Some(_) | None => Err(StateError::InvalidTrialLayout),
        }
    }
}

/// Exact verified deployment identities observed by the firmware adapter.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ValidatedDeployments {
    slots: [Option<DeploymentId>; 2],
}

impl ValidatedDeployments {
    /// Creates an observation after validating that identities occupy their
    /// declared physical slots.
    ///
    /// # Errors
    ///
    /// Returns [`StateError::ObservationSlotMismatch`] for a misplaced
    /// identity.
    pub const fn new(
        slot_a: Option<DeploymentId>,
        slot_b: Option<DeploymentId>,
    ) -> Result<Self, StateError> {
        if matches!(
            slot_a,
            Some(DeploymentId {
                slot: DeploymentSlot::B,
                ..
            })
        ) || matches!(
            slot_b,
            Some(DeploymentId {
                slot: DeploymentSlot::A,
                ..
            })
        ) {
            return Err(StateError::ObservationSlotMismatch);
        }
        Ok(Self {
            slots: [slot_a, slot_b],
        })
    }

    /// Returns whether the exact slot and generation passed verification.
    #[must_use]
    pub const fn contains(self, deployment: DeploymentId) -> bool {
        matches!(self.slots[slot_index(deployment.slot)], Some(id) if id.generation == deployment.generation)
    }
}

/// User/firmware observations that affect deployment selection.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BootObservation {
    /// Exact deployments that passed all verification.
    pub deployments: ValidatedDeployments,
    /// An explicit request to bypass graphical/system deployment boot.
    pub recovery_requested: bool,
}

/// Pre-transfer plan returned by the deterministic policy.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BootPlan {
    /// No durable attempt mutation is required before executing this action.
    Ready(BootAction),
    /// Persist and verify the returned state before exposing the trial action.
    PersistTrial(PendingTrialBoot),
}

/// A trial whose attempt has been consumed in `next_state`, but whose UKI boot
/// action is withheld until the adapter confirms durable persistence.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PendingTrialBoot {
    next_state: BootState,
    deployment: DeploymentId,
    attempt: AttemptId,
}

impl PendingTrialBoot {
    /// Returns the state the adapter must durably write and read back.
    #[must_use]
    pub const fn next_state(self) -> BootState {
        self.next_state
    }

    /// Returns the selected trial deployment for diagnostics.
    #[must_use]
    pub const fn deployment(self) -> DeploymentId {
        self.deployment
    }

    /// Returns the newly consumed attempt identity for diagnostics.
    #[must_use]
    pub const fn attempt(self) -> AttemptId {
        self.attempt
    }

    /// Exposes the UKI boot action after the adapter has durably persisted and
    /// verified [`Self::next_state`].
    #[must_use]
    pub const fn confirm_persisted(self) -> BootAction {
        BootAction::BootTrial {
            deployment: self.deployment,
            attempt: self.attempt,
        }
    }
}

/// Action the firmware adapter may execute.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BootAction {
    /// Start a retained, verified known-good deployment.
    BootKnownGood(DeploymentId),
    /// Start a trial only after its attempt mutation is durable.
    BootTrial {
        /// Exact slot and generation to start.
        deployment: DeploymentId,
        /// Attempt identity early userspace must report.
        attempt: AttemptId,
    },
    /// Enter the independently bootable non-graphical recovery path.
    Recovery(RecoveryReason),
}

/// Why deployment selection chose recovery.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RecoveryReason {
    /// A user or firmware control explicitly requested recovery.
    Requested,
    /// No retained known-good deployment passed verification.
    NoVerifiedKnownGood,
    /// No further unique trial attempt identity can be allocated.
    AttemptIdentityExhausted,
}

/// Authenticated userspace report for one completed trial attempt.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BootSuccessReport {
    /// Exact slot and generation reported by early userspace.
    pub deployment: DeploymentId,
    /// Exact attempt reported by early userspace.
    pub attempt: AttemptId,
}

/// Selects a verified deployment without reading firmware, clocks, files, or
/// graphics state.
///
/// # Attempt Identity Exhaustion
///
/// If the u64 attempt counter reaches its maximum value (2^64 - 1), selection
/// enters recovery instead of wrapping. This represents a theoretical scenario
/// requiring ~5 billion years at 1 attempt/second, but ensures monotonicity
/// guarantees remain intact. Production systems should log and investigate if
/// this condition is ever observed (likely indicates state corruption).
#[must_use]
pub fn prepare_boot(state: &BootState, observation: BootObservation) -> BootPlan {
    if observation.recovery_requested {
        return BootPlan::Ready(BootAction::Recovery(RecoveryReason::Requested));
    }

    if let Some(preferred) = state.record(state.preferred)
        && observation.deployments.contains(preferred.id)
    {
        match preferred.status {
            DeploymentStatus::KnownGood => {
                return BootPlan::Ready(BootAction::BootKnownGood(preferred.id));
            }
            DeploymentStatus::Trial {
                remaining_attempts, ..
            } if remaining_attempts > 0 => {
                // Check for attempt identity exhaustion (theoretical edge case).
                let Some(next_attempt) = state.next_attempt.checked_add(1) else {
                    return BootPlan::Ready(BootAction::Recovery(
                        RecoveryReason::AttemptIdentityExhausted,
                    ));
                };
                let attempt = AttemptId(state.next_attempt);
                let mut next_state = *state;
                next_state.next_attempt = next_attempt;
                next_state.set_record(DeploymentRecord {
                    id: preferred.id,
                    status: DeploymentStatus::Trial {
                        remaining_attempts: remaining_attempts - 1,
                        pending_attempt: Some(attempt),
                    },
                });
                return BootPlan::PersistTrial(PendingTrialBoot {
                    next_state,
                    deployment: preferred.id,
                    attempt,
                });
            }
            DeploymentStatus::Trial { .. } => {}
        }
    }

    for slot in [state.preferred.other(), state.preferred] {
        if let Some(record) = state.record(slot)
            && record.status == DeploymentStatus::KnownGood
            && observation.deployments.contains(record.id)
        {
            return BootPlan::Ready(BootAction::BootKnownGood(record.id));
        }
    }

    BootPlan::Ready(BootAction::Recovery(RecoveryReason::NoVerifiedKnownGood))
}

const fn slot_index(slot: DeploymentSlot) -> usize {
    match slot {
        DeploymentSlot::A => 0,
        DeploymentSlot::B => 1,
    }
}

/// Invalid durable state construction or validated observation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StateError {
    /// Deployment generations start at one.
    ZeroGeneration,
    /// Trial attempt budgets start at one.
    ZeroAttempts,
    /// Durable attempt identities start at one.
    ZeroAttemptIdentity,
    /// Staging would overwrite the only retained known-good deployment.
    NoRetainedKnownGood,
    /// A replacement generation did not increase within its physical slot.
    NonMonotonicGeneration,
    /// An observed deployment identity was placed in the wrong physical slot.
    ObservationSlotMismatch,
    /// Decoded state did not retain any known-good deployment.
    NoKnownGood,
    /// The preferred slot did not contain a deployment record.
    PreferredDeploymentMissing,
    /// Trial placement did not preserve the required opposite fallback.
    InvalidTrialLayout,
    /// The next/pending attempt identities were zero or out of order.
    AttemptSequenceInvalid,
}

impl fmt::Display for StateError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::ZeroGeneration => "deployment generation must be greater than zero",
            Self::ZeroAttempts => "trial attempts must be greater than zero",
            Self::ZeroAttemptIdentity => "attempt identity must be greater than zero",
            Self::NoRetainedKnownGood => "trial staging must retain a known-good opposite slot",
            Self::NonMonotonicGeneration => "replacement generation must increase within its slot",
            Self::ObservationSlotMismatch => "validated deployment was observed in the wrong slot",
            Self::NoKnownGood => "boot state must retain a known-good deployment",
            Self::PreferredDeploymentMissing => "preferred slot must contain a deployment record",
            Self::InvalidTrialLayout => {
                "trial must be preferred and retain a known-good opposite slot"
            }
            Self::AttemptSequenceInvalid => "attempt identities are zero or out of order",
        })
    }
}

impl Error for StateError {}

/// Rejected boot-success report.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReportError {
    /// The reported slot/generation is not the installed trial identity.
    DeploymentMismatch,
    /// The deployment has no consumed trial awaiting promotion.
    NoPendingTrial,
    /// The report names a stale or otherwise different attempt.
    AttemptMismatch,
}

impl fmt::Display for ReportError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::DeploymentMismatch => "success report deployment does not match",
            Self::NoPendingTrial => "no consumed trial attempt awaits promotion",
            Self::AttemptMismatch => "success report attempt does not match",
        })
    }
}

impl Error for ReportError {}
