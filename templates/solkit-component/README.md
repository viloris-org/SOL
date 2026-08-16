# SolKit component library

A library-only template for a reusable SolUI component. It uses only the
current Public-tier semantic crates:

```text
sol-ui       semantic controls, focus, and accessibility behavior
sol-design   token and accessibility preferences
```

It does not create an application, window, renderer, native surface, or
backend dependency. `ApplyAction` demonstrates a library component that
returns a `SemanticControl`, exposes a token-only visual contract, and honors
reduced-motion through `TokenMode`.

## Use in this repository

```bash
cargo test --manifest-path templates/solkit-component/Cargo.toml
```

## Scaffold an external component library

From a SOL checkout, create a named library outside this repository:

```bash
./scripts/new-solkit-component.sh ../solkit-notes-controls solkit-notes-controls
```

The scaffolder rejects existing or in-repository destinations, validates the
Cargo package name, and rewrites every local SolKit SDK path in the copied
manifest and lockfile. Then run the command it prints.

## Copy outside this repository manually

Copy the directory, then point both `path` dependencies at the SDK checkout
that will build the component. For a SOL checkout at `/work/sol`:

```toml
[dependencies]
sol-design = { path = "/work/sol/sdk/sol-design", version = "0.1.0" }
sol-ui = { path = "/work/sol/sdk/sol-ui", version = "0.1.0" }
```

The SDK crates are not published to crates.io. When a matching SDK release is
published, replace both path dependencies together with its registry versions.
This template makes no stability, publication, native-rendering, or packaging
claim.
