# SOL Portal

`sol-portal` is the typed authorization boundary for document-open and screen
capture requests. It turns a caller's `PortalRequest` into the existing
caller-attributed `sol-system` action contract; only an explicit
`PortalAuthorization` may be handed to a platform adapter.

ScreenCast lifecycle separates protected-content-aware `CaptureProducer` work
from `StreamTransport` publication. PipeWire is a transport implementation; it
may only publish `SafeCaptureFeed` values produced by the compositor capture
path and never a display scanout buffer.

The service does not yet implement the XDG Desktop Portal D-Bus interfaces,
file chooser UI, real PipeWire stream creation, compositor capture adapters,
recording, or file grant persistence. Those adapters must retain the request's
caller and may not bypass `SystemActionApi` authorization.

```bash
cargo test -p sol-portal
cargo run -p sol-portal
```

The default executable policy denies ungranted work. A system consent surface
and production permission store remain separate service work.
