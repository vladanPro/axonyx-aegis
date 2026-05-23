# Axonyx Aegis

Rust-first E2E and QA runner for Axonyx applications.

Aegis is the future native testing layer behind:

```bash
cargo ax test
```

The first goal is not to clone Playwright immediately. The first goal is to
define a small, reliable Rust core that can grow into:

- fast HTTP smoke checks
- component and route validation
- browser automation for Axonyx sites
- compatibility checks for React/Next sites that consume Axonyx UI
- CI-friendly reports that can plug back into `cargo ax`

## Current Commands

```bash
cargo run -- doctor
cargo run -- smoke --url http://127.0.0.1:3000 --expect Axonyx
cargo run -- browser
```

`smoke` supports both local `http://` URLs and deployed `https://` URLs. A real
browser engine is reserved for the next phase.

## Intended Axonyx Integration

Framework users should eventually run:

```bash
cargo ax test routes
cargo ax test components
cargo ax test browser
```

Internally, `cargo-axonyx` can delegate those checks to Aegis while keeping the
developer experience inside the Axonyx CLI.

## Philosophy

Aegis should follow the same Foundry direction as Axonyx:

- Rust-first
- explicit diagnostics
- deterministic CI behavior
- no unnecessary JavaScript in the test runner core
- browser automation only when a real browser is needed

## Development

```bash
cargo fmt
cargo test
cargo run -- --help
```
