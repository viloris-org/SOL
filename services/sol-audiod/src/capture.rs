//! Policy for producing a recordable mix without protected playback.
//!
//! The physical-output monitor is intentionally absent from this API. Once
//! protected and ordinary playback have reached that final mix, subtracting
//! the protected samples is neither reliable nor a security boundary. A
//! PipeWire adapter must instead build the recording node from the returned
//! per-playback-node plan.

use std::collections::BTreeSet;

/// Stable audio-graph node identifier assigned by the trusted audio adapter.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct PlaybackNodeId(pub u32);

/// Why a playback node must not contribute samples to recording or sharing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AudioProtectionReason {
    Drm,
    Privacy,
    Authentication,
}

/// Broker-owned recording policy for one independently routed playback node.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum AudioCapturePolicy {
    #[default]
    Allowed,
    Excluded(AudioProtectionReason),
}

/// One playback stream before it reaches the physical output mix.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlaybackNode {
    pub id: PlaybackNodeId,
    pub owner: String,
    pub capture_policy: AudioCapturePolicy,
}

/// Inputs that a PipeWire recording-mix node is allowed to link.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CaptureMixPlan {
    pub included: Vec<PlaybackNodeId>,
    pub excluded: Vec<(PlaybackNodeId, AudioProtectionReason)>,
}

/// Invalid broker inventory. Failing closed avoids publishing an ambiguous
/// graph where a node's protection status depends on entry order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CapturePlanError {
    InvalidNodeId,
    DuplicateNode(PlaybackNodeId),
    InvalidOwner,
}

/// Build a capture-only mix from independently classified playback nodes.
///
/// An empty `included` list is valid and represents silence. Protected streams
/// remain connected to the physical output by the normal routing path; this
/// plan only determines what may also be linked into recording.
pub fn plan_capture_mix(nodes: &[PlaybackNode]) -> Result<CaptureMixPlan, CapturePlanError> {
    let mut seen = BTreeSet::new();
    let mut plan = CaptureMixPlan::default();

    for node in nodes {
        if node.id.0 == 0 {
            return Err(CapturePlanError::InvalidNodeId);
        }
        if node.owner.trim().is_empty() || node.owner.chars().any(char::is_control) {
            return Err(CapturePlanError::InvalidOwner);
        }
        if !seen.insert(node.id) {
            return Err(CapturePlanError::DuplicateNode(node.id));
        }

        match node.capture_policy {
            AudioCapturePolicy::Allowed => plan.included.push(node.id),
            AudioCapturePolicy::Excluded(reason) => plan.excluded.push((node.id, reason)),
        }
    }

    Ok(plan)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn node(id: u32, policy: AudioCapturePolicy) -> PlaybackNode {
        PlaybackNode {
            id: PlaybackNodeId(id),
            owner: format!("org.sol.app-{id}"),
            capture_policy: policy,
        }
    }

    #[test]
    fn protected_playback_is_excluded_while_unrelated_audio_remains_recordable() {
        let plan = plan_capture_mix(&[
            node(1, AudioCapturePolicy::Allowed),
            node(2, AudioCapturePolicy::Excluded(AudioProtectionReason::Drm)),
            node(3, AudioCapturePolicy::Allowed),
        ])
        .expect("plan capture mix");

        assert_eq!(plan.included, vec![PlaybackNodeId(1), PlaybackNodeId(3)]);
        assert_eq!(
            plan.excluded,
            vec![(PlaybackNodeId(2), AudioProtectionReason::Drm)]
        );
    }

    #[test]
    fn a_mix_with_only_protected_playback_becomes_silence() {
        let plan = plan_capture_mix(&[node(
            7,
            AudioCapturePolicy::Excluded(AudioProtectionReason::Privacy),
        )])
        .expect("plan capture mix");

        assert!(plan.included.is_empty());
        assert_eq!(plan.excluded.len(), 1);
    }

    #[test]
    fn duplicate_graph_nodes_fail_closed() {
        assert_eq!(
            plan_capture_mix(&[
                node(9, AudioCapturePolicy::Allowed),
                node(9, AudioCapturePolicy::Excluded(AudioProtectionReason::Drm),),
            ]),
            Err(CapturePlanError::DuplicateNode(PlaybackNodeId(9)))
        );
    }
}
