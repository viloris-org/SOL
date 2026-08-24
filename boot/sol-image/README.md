# sol-image

`sol-image` creates the deterministic deployment manifest consumed by the
future `sol-boot` verifier and A/B slot state machine. A manifest binds one
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

The canonical development encoding is compact UTF-8 JSON followed by one
newline. Runtime majors and feature sets are sorted, and the manifest contains
no timestamps or build-host paths, so the same artifacts and semantic inputs
produce byte-identical output. Parsing rejects alternate JSON encodings,
unknown fields, invalid identifiers, empty artifacts, and zero generations.

This is the deployment identity and artifact-verification foundation, not a
claim of a signed or bootable OS image. Final signature envelope, filesystem,
UKI/UEFI encoding, key enrollment, root-image composition, and `sol-boot`
selection policy remain Phase 7 work.
