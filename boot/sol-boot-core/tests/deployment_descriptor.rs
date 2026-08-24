#![allow(clippy::expect_used)]

use sol_boot_core::{
    ArtifactBinding, DEPLOYMENT_SIGNED_V1_SIZE, DeploymentDescriptor, DeploymentDescriptorError,
    DeploymentId, DeploymentSlot, SignedDeploymentDescriptor,
};

#[test]
fn signed_descriptor_round_trips_canonically() {
    let descriptor = DeploymentDescriptor::new(
        DeploymentId::new(DeploymentSlot::B, 42).expect("identity"),
        ArtifactBinding::new(123, [0x11; 32]),
        ArtifactBinding::new(456, [0x22; 32]),
    );
    let signed = SignedDeploymentDescriptor::new(descriptor, [0x33; 64]);
    let bytes = signed.canonical_bytes();
    assert_eq!(bytes.len(), DEPLOYMENT_SIGNED_V1_SIZE);
    assert_eq!(
        SignedDeploymentDescriptor::from_canonical_bytes(&bytes).expect("decode"),
        signed
    );
}

#[test]
fn descriptor_rejects_wrong_architecture_slot_and_reserved_bytes() {
    let descriptor = DeploymentDescriptor::new(
        DeploymentId::new(DeploymentSlot::A, 1).expect("identity"),
        ArtifactBinding::new(1, [1; 32]),
        ArtifactBinding::new(2, [2; 32]),
    );
    for (index, value) in [(12, 2), (13, 2), (14, 1), (15, 1)] {
        let mut payload = descriptor.canonical_payload();
        payload[index] = value;
        assert_eq!(
            DeploymentDescriptor::from_canonical_payload(&payload),
            Err(DeploymentDescriptorError::NonCanonical)
        );
    }
}
