//! Durable boot orchestration over a minimal firmware storage boundary.

use alloc::vec::Vec;
use core::error::Error;
use core::fmt;
use ed25519_dalek::{Signature, Verifier, VerifyingKey};
use sha2::{Digest, Sha256};
use sol_boot_core::{
    BOOT_SUCCESS_V1_SIZE, BootAction, BootObservation, BootPlan, BootSuccessReport, CodecError,
    DeploymentId, DeploymentSlot, DurableStateCopy, SignedDeploymentDescriptor,
    ValidatedDeployments, prepare_boot, select_redundant_state,
};

/// Conventional files belonging to one physical deployment slot.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SlotFiles {
    /// Signed descriptor.
    pub descriptor: &'static str,
    /// Canonical deployment manifest.
    pub manifest: &'static str,
    /// Slot-specific unified kernel image.
    pub uki: &'static str,
}

impl SlotFiles {
    /// Returns the fixed ESP layout for a slot.
    #[must_use]
    pub const fn for_slot(slot: DeploymentSlot) -> Self {
        match slot {
            DeploymentSlot::A => Self {
                descriptor: "\\EFI\\SOL\\slots\\A\\deployment.bin",
                manifest: "\\EFI\\SOL\\slots\\A\\manifest.json",
                uki: "\\EFI\\SOL\\slots\\A\\system.efi",
            },
            DeploymentSlot::B => Self {
                descriptor: "\\EFI\\SOL\\slots\\B\\deployment.bin",
                manifest: "\\EFI\\SOL\\slots\\B\\manifest.json",
                uki: "\\EFI\\SOL\\slots\\B\\system.efi",
            },
        }
    }
}

/// Redundant durable state file.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StateFile {
    /// First copy.
    A,
    /// Second copy.
    B,
}

impl StateFile {
    /// Returns the conventional ESP path.
    #[must_use]
    pub const fn path(self) -> &'static str {
        match self {
            Self::A => "\\EFI\\SOL\\state\\state-a.bin",
            Self::B => "\\EFI\\SOL\\state\\state-b.bin",
        }
    }

    const fn from_copy(copy: DurableStateCopy) -> Self {
        match copy {
            DurableStateCopy::A => Self::A,
            DurableStateCopy::B => Self::B,
        }
    }
}

/// File operations required by the firmware-independent manager.
pub trait BootStorage {
    /// Adapter-specific I/O error.
    type Error: fmt::Debug + fmt::Display;

    /// Reads a complete file; missing files return `Ok(None)`.
    ///
    /// # Errors
    ///
    /// Returns the adapter error when the path cannot be inspected or read.
    fn read(&mut self, path: &str) -> Result<Option<Vec<u8>>, Self::Error>;
    /// Replaces, flushes, and closes a complete durable file.
    ///
    /// # Errors
    ///
    /// Returns the adapter error unless the complete bytes were flushed.
    fn write_durable(&mut self, path: &str, bytes: &[u8]) -> Result<(), Self::Error>;
    /// Removes a consumed report. Missing files should be accepted.
    ///
    /// # Errors
    ///
    /// Returns the adapter error when an existing file cannot be removed.
    fn remove(&mut self, path: &str) -> Result<(), Self::Error>;
}

/// A boot action whose slot artifacts were authenticated in this invocation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SelectedBoot {
    action: BootAction,
}

impl SelectedBoot {
    /// Returns the policy action.
    #[must_use]
    pub const fn action(self) -> BootAction {
        self.action
    }

    /// Returns the verified UKI path for deployment actions.
    #[must_use]
    pub const fn uki_path(self) -> Option<&'static str> {
        match self.action {
            BootAction::BootKnownGood(deployment) | BootAction::BootTrial { deployment, .. } => {
                Some(SlotFiles::for_slot(deployment.slot()).uki)
            }
            BootAction::Recovery(_) => None,
        }
    }
}

/// Verifies installed artifacts, applies success, consumes trials, and selects a boot action.
pub struct BootManager<S> {
    storage: S,
    release_key: VerifyingKey,
}

impl<S: BootStorage> BootManager<S> {
    /// Creates a manager using the pinned SOL deployment release key.
    ///
    /// # Errors
    ///
    /// Returns [`BootManagerError::InvalidReleaseKey`] for a malformed point.
    pub fn new(storage: S, release_key: [u8; 32]) -> Result<Self, BootManagerError> {
        let release_key = VerifyingKey::from_bytes(&release_key)
            .map_err(|_| BootManagerError::InvalidReleaseKey)?;
        Ok(Self {
            storage,
            release_key,
        })
    }

    /// Runs the complete pre-transfer boot transaction.
    ///
    /// # Errors
    ///
    /// Fails closed on storage, durable-state, or observation errors. No trial
    /// action is returned unless its consumed state passes exact read-back.
    pub fn select(&mut self, recovery_requested: bool) -> Result<SelectedBoot, BootManagerError> {
        let mut selected = self.load_state()?;
        if let Some(report) = self.load_success_report()?
            && let Ok(promoted) = selected.envelope().state().apply_success_report(report)
        {
            selected = self.persist_state(selected, promoted)?;
            self.storage
                .remove("\\EFI\\SOL\\state\\success.bin")
                .map_err(|_| BootManagerError::Storage)?;
        }

        let slot_a = self.verify_slot(DeploymentSlot::A)?;
        let slot_b = self.verify_slot(DeploymentSlot::B)?;
        let deployments = ValidatedDeployments::new(slot_a, slot_b)
            .map_err(|_| BootManagerError::InvalidObservation)?;
        let plan = prepare_boot(
            &selected.envelope().state(),
            BootObservation {
                deployments,
                recovery_requested,
            },
        );
        let action = match plan {
            BootPlan::Ready(action) => {
                let _ = self.storage.remove("\\EFI\\SOL\\state\\current.bin");
                action
            }
            BootPlan::PersistTrial(pending) => {
                self.persist_state(selected, pending.next_state())?;
                let report = BootSuccessReport {
                    deployment: pending.deployment(),
                    attempt: pending.attempt(),
                };
                self.storage
                    .write_durable("\\EFI\\SOL\\state\\current.bin", &report.canonical_bytes())
                    .map_err(|_| BootManagerError::Storage)?;
                pending.confirm_persisted()
            }
        };
        Ok(SelectedBoot { action })
    }

    /// Exposes storage after selection so the adapter can load the selected image.
    pub const fn storage_mut(&mut self) -> &mut S {
        &mut self.storage
    }

    /// Reads the selected UKI into an immutable transfer buffer and verifies it
    /// again immediately before the adapter calls firmware `LoadImage`.
    ///
    /// # Errors
    ///
    /// Rejects recovery actions, storage failures, or a slot whose descriptor,
    /// manifest, UKI, signature, or identity changed after selection.
    pub fn load_selected_uki(
        &mut self,
        selected: SelectedBoot,
    ) -> Result<Vec<u8>, BootManagerError> {
        let deployment = match selected.action {
            BootAction::BootKnownGood(deployment) | BootAction::BootTrial { deployment, .. } => {
                deployment
            }
            BootAction::Recovery(_) => return Err(BootManagerError::NoSelectedUki),
        };
        let Some((verified, uki)) = self.load_verified_slot(deployment.slot())? else {
            return Err(BootManagerError::SelectedArtifactChanged);
        };
        if verified != deployment {
            return Err(BootManagerError::SelectedArtifactChanged);
        }
        Ok(uki)
    }

    fn load_state(&mut self) -> Result<sol_boot_core::SelectedDurableState, BootManagerError> {
        let a = self
            .storage
            .read(StateFile::A.path())
            .map_err(|_| BootManagerError::Storage)?;
        let b = self
            .storage
            .read(StateFile::B.path())
            .map_err(|_| BootManagerError::Storage)?;
        select_redundant_state(a.as_deref(), b.as_deref()).map_err(BootManagerError::State)
    }

    fn load_success_report(&mut self) -> Result<Option<BootSuccessReport>, BootManagerError> {
        let Some(bytes) = self
            .storage
            .read("\\EFI\\SOL\\state\\success.bin")
            .map_err(|_| BootManagerError::Storage)?
        else {
            return Ok(None);
        };
        if bytes.len() != BOOT_SUCCESS_V1_SIZE {
            return Ok(None);
        }
        Ok(BootSuccessReport::from_canonical_bytes(&bytes).ok())
    }

    fn verify_slot(
        &mut self,
        slot: DeploymentSlot,
    ) -> Result<Option<DeploymentId>, BootManagerError> {
        Ok(self
            .load_verified_slot(slot)?
            .map(|(deployment, _)| deployment))
    }

    fn load_verified_slot(
        &mut self,
        slot: DeploymentSlot,
    ) -> Result<Option<(DeploymentId, Vec<u8>)>, BootManagerError> {
        let files = SlotFiles::for_slot(slot);
        let Some(bytes) = self
            .storage
            .read(files.descriptor)
            .map_err(|_| BootManagerError::Storage)?
        else {
            return Ok(None);
        };
        let Ok(signed) = SignedDeploymentDescriptor::from_canonical_bytes(&bytes) else {
            return Ok(None);
        };
        let descriptor = signed.descriptor();
        if descriptor.deployment().slot() != slot {
            return Ok(None);
        }
        let signature = Signature::from_bytes(&signed.signature());
        if self
            .release_key
            .verify(&descriptor.canonical_payload(), &signature)
            .is_err()
        {
            return Ok(None);
        }
        let Some(manifest) = self
            .storage
            .read(files.manifest)
            .map_err(|_| BootManagerError::Storage)?
        else {
            return Ok(None);
        };
        let Some(uki) = self
            .storage
            .read(files.uki)
            .map_err(|_| BootManagerError::Storage)?
        else {
            return Ok(None);
        };
        if !matches_binding(&manifest, descriptor.manifest())
            || !matches_binding(&uki, descriptor.uki())
        {
            return Ok(None);
        }
        Ok(Some((descriptor.deployment(), uki)))
    }

    fn persist_state(
        &mut self,
        selected: sol_boot_core::SelectedDurableState,
        state: sol_boot_core::BootState,
    ) -> Result<sol_boot_core::SelectedDurableState, BootManagerError> {
        let next = selected
            .envelope()
            .advance(state)
            .map_err(BootManagerError::State)?;
        let target = selected.copy().other();
        self.storage
            .write_durable(StateFile::from_copy(target).path(), &next.canonical_bytes())
            .map_err(|_| BootManagerError::Storage)?;
        let reread = self.load_state()?;
        if reread.copy() != target || reread.envelope() != next {
            return Err(BootManagerError::StateReadbackMismatch);
        }
        Ok(reread)
    }
}

fn matches_binding(bytes: &[u8], binding: sol_boot_core::ArtifactBinding) -> bool {
    u64::try_from(bytes.len()).ok() == Some(binding.length())
        && Sha256::digest(bytes).as_slice() == binding.sha256()
}

/// Failure before the adapter is authorized to transfer control.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BootManagerError {
    /// Compiled release public key is malformed.
    InvalidReleaseKey,
    /// Firmware storage failed.
    Storage,
    /// No safe durable state could be selected or advanced.
    State(CodecError),
    /// Durable read-back did not select the exact just-written state.
    StateReadbackMismatch,
    /// Slot observations violated physical identity constraints.
    InvalidObservation,
    /// A recovery selection has no deployment UKI.
    NoSelectedUki,
    /// Selected artifacts no longer match the authenticated selection.
    SelectedArtifactChanged,
}

impl fmt::Display for BootManagerError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidReleaseKey => formatter.write_str("invalid compiled release key"),
            Self::Storage => formatter.write_str("firmware storage operation failed"),
            Self::State(error) => write!(formatter, "durable boot state failed: {error}"),
            Self::StateReadbackMismatch => {
                formatter.write_str("durable boot state read-back mismatch")
            }
            Self::InvalidObservation => {
                formatter.write_str("verified deployments occupy invalid slots")
            }
            Self::NoSelectedUki => formatter.write_str("recovery selection has no deployment UKI"),
            Self::SelectedArtifactChanged => {
                formatter.write_str("selected deployment artifacts changed before transfer")
            }
        }
    }
}

impl Error for BootManagerError {}
