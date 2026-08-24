#![allow(clippy::expect_used)]

use ed25519_dalek::{Signer, SigningKey};
use sha2::{Digest, Sha256};
use sol_boot::{BootManager, BootManagerError, BootStorage, SlotFiles, StateFile};
use sol_boot_core::{
    ArtifactBinding, BootAction, BootState, BootSuccessReport, DeploymentDescriptor, DeploymentId,
    DeploymentSlot, DurableBootState, SignedDeploymentDescriptor,
};
use std::collections::BTreeMap;

#[derive(Debug, Default)]
struct MemoryStorage {
    files: BTreeMap<String, Vec<u8>>,
    writes: Vec<String>,
    corrupt_writes: bool,
}

impl BootStorage for MemoryStorage {
    type Error = &'static str;

    fn read(&mut self, path: &str) -> Result<Option<Vec<u8>>, Self::Error> {
        Ok(self.files.get(path).cloned())
    }

    fn write_durable(&mut self, path: &str, bytes: &[u8]) -> Result<(), Self::Error> {
        let mut bytes = bytes.to_vec();
        if self.corrupt_writes {
            bytes[0] ^= 1;
        }
        self.files.insert(path.to_owned(), bytes);
        self.writes.push(path.to_owned());
        Ok(())
    }

    fn remove(&mut self, path: &str) -> Result<(), Self::Error> {
        self.files.remove(path);
        Ok(())
    }
}

fn identity(slot: DeploymentSlot, generation: u64) -> DeploymentId {
    DeploymentId::new(slot, generation).expect("identity")
}

fn install_slot(
    storage: &mut MemoryStorage,
    key: &SigningKey,
    deployment: DeploymentId,
    manifest: &[u8],
    uki: &[u8],
) {
    let files = SlotFiles::for_slot(deployment.slot());
    let descriptor = DeploymentDescriptor::new(deployment, binding(manifest), binding(uki));
    let signature = key.sign(&descriptor.canonical_payload()).to_bytes();
    storage.files.insert(
        files.descriptor.to_owned(),
        SignedDeploymentDescriptor::new(descriptor, signature)
            .canonical_bytes()
            .to_vec(),
    );
    storage
        .files
        .insert(files.manifest.to_owned(), manifest.to_vec());
    storage.files.insert(files.uki.to_owned(), uki.to_vec());
}

fn binding(bytes: &[u8]) -> ArtifactBinding {
    ArtifactBinding::new(bytes.len() as u64, Sha256::digest(bytes).into())
}

fn manager_with_state(state: BootState) -> (BootManager<MemoryStorage>, SigningKey) {
    let key = SigningKey::from_bytes(&[7; 32]);
    let envelope = DurableBootState::new(state).expect("state");
    let mut storage = MemoryStorage::default();
    for path in [StateFile::A.path(), StateFile::B.path()] {
        storage
            .files
            .insert(path.to_owned(), envelope.canonical_bytes().to_vec());
    }
    let manager = BootManager::new(storage, key.verifying_key().to_bytes()).expect("manager");
    (manager, key)
}

#[test]
fn verified_known_good_uki_is_selected() {
    let known_good = identity(DeploymentSlot::A, 4);
    let (mut manager, key) = manager_with_state(BootState::new(known_good));
    install_slot(
        manager.storage_mut(),
        &key,
        known_good,
        b"canonical manifest",
        b"signed UKI",
    );

    let selection = manager.select(false).expect("selection");
    assert_eq!(selection.action(), BootAction::BootKnownGood(known_good));
    assert_eq!(
        selection.uki_path(),
        Some(SlotFiles::for_slot(DeploymentSlot::A).uki)
    );
    assert_eq!(
        manager.load_selected_uki(selection).expect("transfer UKI"),
        b"signed UKI"
    );
}

#[test]
fn selected_uki_is_reverified_into_the_transfer_buffer() {
    let known_good = identity(DeploymentSlot::A, 5);
    let (mut manager, key) = manager_with_state(BootState::new(known_good));
    install_slot(
        manager.storage_mut(),
        &key,
        known_good,
        b"canonical manifest",
        b"signed UKI",
    );
    let selection = manager.select(false).expect("selection");
    manager.storage_mut().files.insert(
        SlotFiles::for_slot(DeploymentSlot::A).uki.to_owned(),
        b"changed after selection".to_vec(),
    );
    assert_eq!(
        manager.load_selected_uki(selection),
        Err(BootManagerError::SelectedArtifactChanged)
    );
}

#[test]
fn corrupt_preferred_artifact_falls_back_without_consuming_trial() {
    let known_good = identity(DeploymentSlot::A, 10);
    let trial = identity(DeploymentSlot::B, 11);
    let state = BootState::new(known_good)
        .stage_trial(trial, 2)
        .expect("stage");
    let (mut manager, key) = manager_with_state(state);
    install_slot(
        manager.storage_mut(),
        &key,
        known_good,
        b"manifest A",
        b"UKI A",
    );
    install_slot(manager.storage_mut(), &key, trial, b"manifest B", b"UKI B");
    manager.storage_mut().files.insert(
        SlotFiles::for_slot(DeploymentSlot::B).uki.to_owned(),
        b"tampered".to_vec(),
    );

    let selection = manager.select(false).expect("fallback");
    assert_eq!(selection.action(), BootAction::BootKnownGood(known_good));
    assert!(manager.storage_mut().writes.is_empty());
}

#[test]
fn trial_is_exposed_only_after_exact_redundant_readback() {
    let known_good = identity(DeploymentSlot::A, 20);
    let trial = identity(DeploymentSlot::B, 21);
    let state = BootState::new(known_good)
        .stage_trial(trial, 1)
        .expect("stage");
    let (mut manager, key) = manager_with_state(state);
    install_slot(
        manager.storage_mut(),
        &key,
        known_good,
        b"manifest A",
        b"UKI A",
    );
    install_slot(manager.storage_mut(), &key, trial, b"manifest B", b"UKI B");
    manager.storage_mut().corrupt_writes = true;

    assert_eq!(
        manager.select(false),
        Err(BootManagerError::StateReadbackMismatch)
    );
}

#[test]
fn exact_success_report_promotes_before_next_selection() {
    let known_good = identity(DeploymentSlot::A, 30);
    let trial = identity(DeploymentSlot::B, 31);
    let state = BootState::new(known_good)
        .stage_trial(trial, 2)
        .expect("stage");
    let (mut first, key) = manager_with_state(state);
    install_slot(
        first.storage_mut(),
        &key,
        known_good,
        b"manifest A",
        b"UKI A",
    );
    install_slot(first.storage_mut(), &key, trial, b"manifest B", b"UKI B");
    let BootAction::BootTrial { attempt, .. } = first.select(false).expect("trial").action() else {
        panic!("trial expected");
    };
    assert_eq!(
        BootSuccessReport::from_canonical_bytes(
            first
                .storage_mut()
                .files
                .get("\\EFI\\SOL\\state\\current.bin")
                .expect("current trial identity")
        )
        .expect("current report template")
        .attempt,
        attempt
    );
    first.storage_mut().files.insert(
        "\\EFI\\SOL\\state\\success.bin".to_owned(),
        BootSuccessReport {
            deployment: trial,
            attempt,
        }
        .canonical_bytes()
        .to_vec(),
    );

    let selection = first.select(false).expect("promoted selection");
    assert_eq!(selection.action(), BootAction::BootKnownGood(trial));
    assert!(
        !first
            .storage_mut()
            .files
            .contains_key("\\EFI\\SOL\\state\\success.bin")
    );
}
