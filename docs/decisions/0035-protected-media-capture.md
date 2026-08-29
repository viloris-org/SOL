# ADR-0035: Protected media and privacy-safe capture

- **Status:** Accepted (foundation implemented)
- **Date:** 2026-08-28
- **Target phase:** Phase 5 / screen sharing and protected playback
- **Extends:** ADR-0021 application permissions, ADR-0027 SCP, and ADR-0031
  device control plane

## Context

SOL must allow ordinary areas of a display and ordinary audio streams to be
recorded while DRM, privacy-sensitive, and authentication content remains
unobservable. Replacing PipeWire would not establish that property. PipeWire
transports buffers and samples after a producer has decided what they contain;
capturing the final display framebuffer or physical-output audio monitor would
already have crossed the security boundary too late.

The requirement applies to every path by which content can leave the local
display/audio route: screenshots, recording, screen sharing, remote desktop,
window previews, OCR/machine vision, and diagnostic capture.

## Decision

SOL retains PipeWire as a replaceable media transport and audio graph. Content
eligibility is decided before PipeWire by the authoritative producer.

### Video

The compositor has separate display and capture composition purposes:

```text
surface tree ──→ display composition ──→ DRM/KMS scanout
       └──────→ capture composition ──→ safe feed ──→ PipeWire/consumer
```

Display composition may show protected surfaces. Capture composition replaces
each excluded surface with an opaque compositor-owned placeholder before its
buffer is mapped or sampled. It must not merely omit the surface, because that
would reveal windows hidden behind it on the physical display.

Protection is compositor-owned state. Ordinary SCP clients cannot self-assert
or clear it. A trusted protected-media, privacy, or authentication broker must
authenticate the grant and apply it through an in-process compositor adapter.
Effects derived from protected pixels must be recomputed from the placeholder
or redacted; protected pixels may never enter an intermediate capture texture.

All capture consumers use the same capture composition API. Direct reads of a
display framebuffer are not a supported screenshot or recording mechanism.

### DRM display path

Protected decode buffers will remain non-CPU-mappable and outside capture
composition. The native backend will present them through a protected GPU/KMS
path and request connector content protection. Loss of the required protected
link blanks or pauses protected playback; it never falls back to an
unprotected composited buffer.

Connector/HDCP protection secures the physical link. It does not replace the
capture composition rule.

### Audio

`sol-audiod` never builds recording from a physical sink monitor. Protected and
ordinary playback must remain separate nodes until policy is applied:

```text
ordinary playback ──┬──→ physical-output mix
                    └──→ capture-only mix
protected playback ─────→ physical-output mix
```

A broker-owned policy classifies each playback node. The capture-only mix links
only allowed nodes; if every active node is protected, the result is silence.
Applications receiving a Portal-granted PipeWire connection see only the safe
capture node, never protected playback nodes or the physical sink monitor.

### Portal and transport

`sol-portal` treats capture production and stream transport as separate trusted
roles. A transport can publish only a `SafeCaptureFeed` returned by the capture
producer. If publication or validation fails, the producer is stopped and the
session does not enter the streaming state.

## Security invariants

1. Protection is fail-closed and cannot be asserted through an ordinary client
   protocol request.
2. Capture redaction happens before reading protected content.
3. A redaction is opaque and covers the protected surface's displayed bounds.
4. Unrelated pixels and independently routed audio remain capturable.
5. PipeWire permission control limits who can see safe nodes, but PipeWire is
   not the authority that classifies content.
6. A physical-output audio monitor and a display scanout framebuffer are never
   accepted as recording sources.

## Current implementation boundary

The SCP software compositor implements display/capture purposes, protected
surface replacement, and regression tests for background non-disclosure.
`sol-portal` separates `CaptureProducer` from `StreamTransport` and validates
their one-to-one feed mapping. `sol-audiod` implements the capture-mix planner.

Native GPU protected buffers, KMS/HDCP state handling, the authenticated broker
IPC, the PipeWire safe-video publisher, and the PipeWire capture-mix graph
adapter remain Phase 5 hardware/session work. These missing adapters may not be
represented as completed DRM protection.

## Consequences

- PipeWire remains available for device routing, low-latency graph processing,
  and restricted stream export.
- Screenshot and screencast implementations cannot reuse presentation buffers
  as a shortcut.
- Protected playback needs distinct video surfaces and audio nodes; content
  mixed inside an untrusted application before reaching SOL cannot be separated
  securely afterwards.
- Effects, hardware overlays, multi-output capture, hotplug, and HDCP loss need
  explicit protected-content test coverage.
