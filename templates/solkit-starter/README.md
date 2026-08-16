# SolKit starter

A small headless SolKit app using only public SDK crates:

```text
sol-app      lifecycle and commands
sol-ui       semantic controls and keyboard behavior
sol-design   token and accessibility preferences
```

It intentionally has no renderer, Wayland, Slint, or GPU dependency. The
workflow starts an app, runs a command, focuses and activates a semantic
button, and checks reduced-motion token resolution.

## Use in this repository

```bash
cargo run --manifest-path templates/solkit-starter/Cargo.toml
cargo test --manifest-path templates/solkit-starter/Cargo.toml
```

Rename the package and replace `APP_ID` in `src/lib.rs` before making an app.
The identifier must be one owned by the publisher, for example
`com.example.notes`.

## Scaffold an external project

From a SOL checkout, create a named app outside this repository:

```bash
./scripts/new-solkit-project.sh ../notes-app notes-app com.example.notes
```

The scaffolder rejects existing or in-repository destinations, validates the
Cargo package name and reverse-DNS app ID, rewrites both values, and points the
copied project at the local SDK checkout. It also updates the binary's Rust
crate identifier when the package name contains hyphens. Then run the command
it prints.

## Copy outside this repository manually

Copy the directory, then change every `path` dependency in `Cargo.toml` to
the SDK checkout that the application will build against. For example, when a
SOL checkout lives at `/work/sol`:

```toml
[dependencies]
sol-app = { path = "/work/sol/sdk/sol-app", version = "0.1.0" }
sol-design = { path = "/work/sol/sdk/sol-design", version = "0.1.0" }
sol-ui = { path = "/work/sol/sdk/sol-ui", version = "0.1.0" }
```

Then run `cargo test` from the copied directory. The SDK crates are not yet
published to crates.io. Once an SDK release is published, replace those three
tables with its matching registry requirements, for example `sol-app = "0.1"`.
Keep all SolKit crates on the same compatible release line.

The template validates only deterministic semantic behavior. A native surface
and packaging/distribution integration are separate platform milestones.
