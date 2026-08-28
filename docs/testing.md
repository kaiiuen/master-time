# Testing

Master Time keeps protocol parsing, calculations, policy, state, and UI-facing models testable without requiring a live public time service.

## Standard checks

Run these commands from `master-time`:

```text
cargo check
cargo test
cargo fmt -- --check
```

`cargo check` verifies compilation without producing a final binary. `cargo test` runs unit tests in the library and the test-enabled binary. `cargo fmt -- --check` verifies Rust formatting without changing files.

For a production-like compilation, also run:

```text
cargo build --release
```

## What is covered

- **NTP primitives:** request construction, timestamp encoding, header parsing, and malformed-packet rejection.
- **Measurement:** four-timestamp offset, delay, root-distance calculations, and invalid inputs.
- **Transport:** a local UDP test server checks request/response behavior; no public NTP host is contacted by the test.
- **Service and polling:** timestamp assembly, server-mode validation, health evaluation, recurring work, and clean shutdown.
- **Configuration and storage:** bounds checking, server validation, text parsing, malformed-file rejection, and atomic-save behavior.
- **Application state and settings:** bounded history, error handling, server switching, draft/apply/cancel semantics.
- **Presentation models:** formatting, localization, diagnostics, and history-view behavior.

## Network-aware testing

The transport tests bind a loopback UDP socket and use an explicit local address. They are suitable for normal CI environments and do not depend on DNS, internet access, or the current public time. Avoid replacing them with tests against `time.google.com`, `pool.ntp.org`, or another external service: public services introduce latency, availability, rate-limit, and reproducibility problems.

When investigating a real deployment issue, use `cargo run` and the application error/status display. Treat live-server testing as manual verification, not as a required automated test.

## Before submitting a change

1. Run formatting checks.
2. Run the complete test suite.
3. Build the release profile.
4. If networking changed, inspect the request destination, timeout, and error path manually.
5. Record the exact commands and target platform in the review or release notes.
