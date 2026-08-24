# sol-image

`sol-image` creates the deterministic deployment manifest consumed by the
`sol-boot` verifier and A/B slot state machine. A manifest binds one
physical slot and generation to the exact SHA-256 digest and byte length of its
kernel, initrd, and immutable root image, plus the runtime major/revision/
feature contracts exposed by that deployment.

```bash
cargo run -p sol-image -- manifest \
  --slot B \
  --generation 42 \
  --version 0.2.0-dev \
  --kernel build/vmlinuz \
  --initrd build/initrd.img \
  --root-image build/root.img \
  --runtime 'sol-runtime-1:12:accessibility.tree-v1,documents.v2' \
  --output build/deployments/B/manifest.json

cargo run -p sol-image -- verify \
  --manifest build/deployments/B/manifest.json \
  --kernel build/vmlinuz \
  --initrd build/initrd.img \
  --root-image build/root.img
```

Format 2 adds the complete UKI and dm-verity boot identity without changing
format 1:

```bash
cargo run -p sol-image -- manifest \
  --slot B \
  --generation 43 \
  --version 0.3.0-dev \
  --kernel build/vmlinuz \
  --initrd build/initrd.img \
  --root-image build/root.img \
  --uki build/sol-B.efi \
  --kernel-component kernel-x86_64:slot-b-gen-43-kernel \
  --initrd-component initrd-base:slot-b-gen-43-initrd \
  --dm-verity-root-hash "$ROOT_HASH" \
  --dm-verity-slot-root slot-b-gen-43-root \
  --runtime 'sol-runtime-1:12:documents.v2' \
  --output build/deployments/B/manifest.json

cargo run -p sol-image -- verify \
  --manifest build/deployments/B/manifest.json \
  --kernel build/vmlinuz \
  --initrd build/initrd.img \
  --root-image build/root.img \
  --uki build/sol-B.efi
```

The canonical development encoding is compact UTF-8 JSON followed by one
newline. Runtime majors and feature sets are sorted, and the manifest contains
no timestamps or build-host paths, so the same artifacts and semantic inputs
produce byte-identical output. Parsing rejects alternate JSON encodings,
unknown fields, invalid identifiers, empty artifacts, and zero generations.

Format 1 remains parseable byte-for-byte, while format 2 requires the UKI
digest/length, logical kernel and initrd identities, dm-verity root hash, and a
slot-specific root identity. Verification of format 2 always re-hashes the UKI;
omitting `--uki` fails instead of reporting partial verification. Versioned
fixtures in `tests/fixtures/` protect both encodings.

For bootable development images, the CLI also signs the canonical format-2
manifest/UKI binding (`boot-descriptor`), initializes redundant state
(`init-boot-state`), stages an inactive-slot trial (`stage-boot-trial`), emits
the exact health report (`success-report`), and derives the build-time public
key (`release-public-key`). See the [sol-boot deployment
guide](../sol-boot/README.md) for the complete ESP workflow.
