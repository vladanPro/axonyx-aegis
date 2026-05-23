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
cargo run -- fast --config aegis.toml
cargo run -- browser
```

`fast` runs HTTP/response checks without launching a browser. It is designed for
quick route, text, status, and deployment checks. A real browser engine is
reserved for login flows, JavaScript interactions, forms, redirects, screenshots,
and user journeys.

## Config Smoke Suites

For React, Next, or Axonyx sites, create `aegis.toml`:

```toml
base_url = "https://react.axonyx.dev"

[[smoke]]
name = "home"
path = "/"
expect = "Axonyx"

[[smoke]]
name = "docs"
path = "/docs/getting-started"
expect = "Getting Started"
```

Then run:

```bash
aegis fast --config aegis.toml
```

`aegis test --config aegis.toml` currently works as an alias for `fast`, but
`fast` is the clearer command when you specifically want no-browser checks.

## Fast vs Browser

Use `fast` when you want quick response checks:

- status code
- route exists
- HTML contains text
- deployed site is alive
- generated docs/components are reachable

Use future `browser` checks when the test needs a real user session:

- login form
- cookies/session redirects
- client-side navigation
- dropdowns/modals/theme switchers
- JavaScript validation
- screenshots and traces

## Rust Test Files

Aegis can also be used as a Rust test DSL. A project can keep the same mental
model as Playwright, but split fast response tests from real browser tests:

```text
tests/
  fast.rs
  fast/
    opens_docs.rs
  browser.rs
  browser/
    login.rs
```

`tests/fast.rs`:

```rust
#[path = "fast/opens_docs.rs"]
mod opens_docs;
```

`tests/fast/opens_docs.rs`:

```rust
#[test]
fn opens_docs() {
    aegis::fast("opens docs", |page| {
        page.goto("https://react.axonyx.dev");
        page.click("a[href='/docs/getting-started']");
        page.expect_text("Getting Started");
    });
}
```

Fast tests do not launch a browser. `click` currently supports normal anchor
navigation such as `a[href='/docs/getting-started']` and follows that link with a
new HTTP request.

Future browser tests keep the same shape, but will run through a headless
browser engine:

```rust
#[test]
fn login() {
    aegis::browser("login", |page| {
        page.goto("https://react.axonyx.dev/login");
        page.click("a[href='/profile/settings']");
        page.expect_text("Change theme");
    });
}
```

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
