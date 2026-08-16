# SolKit getting started

SolKit applications use public semantic SDK crates rather than compositor,
Wayland, Slint, or GPU APIs directly. Start from the
[`solkit-starter`](../templates/solkit-starter) template.

## Run the starter

From a SOL checkout:

```bash
cargo run --manifest-path templates/solkit-starter/Cargo.toml
cargo test --manifest-path templates/solkit-starter/Cargo.toml
```

The starter is headless and deterministic. It demonstrates the minimal app
shape: create an `App` with a stable `AppId`, add a window, register a
`Command`, express controls through `sol-ui`, and use `sol-design` token modes
for accessibility preferences. Its output is a stable report, so no graphical
session is required.

## Make it your app

1. Copy `templates/solkit-starter` to a directory outside this workspace.
2. Rename the Cargo package and replace `APP_ID` with an identifier owned by
   the publisher.
3. Point the three SolKit `path` dependencies at the SDK checkout used to
   build the application.
4. Run `cargo test` in the copied project before adding application behavior.

See the template [README](../templates/solkit-starter/README.md) for the exact
dependency replacement. SolKit crates are currently local SDK crates, not
crates.io packages. When a release line is published, replace all three path
dependencies together with matching registry versions.

## Development contract

Use `sol-ui` semantic components and `sol-design` roles. Do not introduce
renderer, protocol, or raw visual-metric dependencies in application code.
The default template deliberately validates lifecycle, command, keyboard, and
reduced-motion behavior without a desktop session. Native-surface and
distribution work remain outside this starter's scope.

## Validate the copy-out path

The repository check copies the template into a temporary external directory,
rewrites only the documented local SDK dependency prefix, then tests and runs
it:

```bash
./scripts/validate-solkit-starter.sh
```
