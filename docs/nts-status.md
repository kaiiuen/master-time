# NTS status

**Audit date:** 2026-08-28  
**Scope:** `master-time` NTS boundary and its RFC 8915 readiness

## Security and implementation finding

NTS is **not implemented** and must not be reported as supported. The current
boundary is an explicit refusal path, not a partial protocol implementation:

- `src/nts.rs` models `PlainUdp`, `NtsRequired`, and `NtsPreferred`, validates an
  NTS-KE endpoint as `host:port` (default port 4460), and reports all five NTS
  security capabilities as unavailable.
- `src/nts_transport.rs` provides a typed endpoint/policy/timeout boundary and
  refuses execution. It neither connects to NTS-KE nor sends NTS-like UDP.
- The `nts-boundary` feature only enables that explicit `NotImplemented` result;
  it does not enable NTS.
- `Cargo.toml` has no NTS dependency. Ordinary NTP remains a separate plain-UDP
  transport and is not an NTS result.

This is a safe current posture: there is no unauthenticated fallback hidden
inside the NTS boundary and no claim that a parsed NTS-shaped packet is secure.
The endpoint parser is input validation only; it is not TLS certificate
validation or proof that the endpoint supports NTS.

## Required before RFC 8915 can be enabled

All items below are release-blocking. “NTS preferred” must not be treated as a
way around any item; fallback to plain UDP must remain an explicit policy choice
and must be visible in results and diagnostics.

### 1. NTS-KE and wire interoperability

- Implement the RFC 8915 NTS-KE state machine over TLS 1.3, including the
  required next-protocol and AEAD negotiation, end-of-message handling, and
  rejection of malformed, duplicated, unsupported, or out-of-order records.
- Establish the NTS-KE endpoint separately from the NTP endpoint when required;
  do not infer NTS support from the `host:port` parser or an NTP extension-field
  label.
- Add interoperability tests against at least two independent, maintained RFC
  8915 implementations, covering both IPv4 and IPv6 and a normal exchange,
  fresh process, reconnect, timeout, and server rejection. Record versions,
  configuration, packet traces or equivalent evidence, and expected outcomes.
  Include a known-good public NTS service where practical, without making tests
  depend on an unowned production service.
- Add deterministic RFC/implementation test vectors and negative wire tests for
  truncation, bad lengths, unknown critical values, unsupported AEADs, bad
  cookies, bad tags, and unexpected extension fields.

### 2. TLS and certificate validation

- Use a maintained TLS 1.3 implementation with certificate verification enabled;
  never use an “accept any certificate” verifier, disable hostname checking, or
  silently continue after verification failure.
- Validate the server certificate chain against an explicitly chosen trust
  store, validity period, key usage/EKU as applicable, hostname/SNI identity,
  and TLS signature/key parameters. Define the trust-store update and platform
  behavior for this Windows application.
- Require the TLS peer identity used for NTS-KE to be policy-controlled and
  auditable. IP literals, redirects, proxies, and an NTP hostname differing
  from the NTS-KE hostname need explicit rules and tests.
- Test expired, not-yet-valid, wrong-host, untrusted-root, missing-chain,
  revoked/blocked (where the chosen TLS stack supports it), and TLS-version/
  cipher-policy failures. Fail closed and expose a distinct diagnostic from a
  plain-UDP result.

### 3. Key negotiation and derivation

- Negotiate only RFC 8915-supported AEADs, with an explicit allowlist and a
  documented minimum. Implement the mandatory algorithm and reject an empty or
  downgraded negotiation; do not invent an algorithm identifier.
- Derive the client-to-server and server-to-client packet keys from the TLS
  exporter specified by RFC 8915 (the exact exporter label, empty context, key
  lengths, ordering, and byte handling must be covered by test vectors). Do not
  use the TLS traffic keys directly, passwords, endpoint text, or a general
  purpose hash as a substitute.
- Keep directions, algorithm IDs, key lengths, and zeroization/lifetime rules
  explicit in the API. Never log keys, cookies containing key material, TLS
  exporter output, or authenticator plaintext.
- Test derivation against independent vectors, both supported AEAD sizes, key
  separation/directionality, reconnect rotation, and rejection of mismatched
  algorithm or key-length metadata.

### 4. Cookies and authenticated NTP packets

- Parse and emit the RFC 8915 cookie, cookie-placeholder, and authenticator/
  encrypted-extension fields with exact field types, lengths, ordering, and
  padding rules. Do not classify an extension by a display string alone.
- Store cookies as opaque sensitive values, bound to the negotiated server
  context as required by the protocol, with bounded count/size and an explicit
  replacement/expiration policy. Do not persist them in diagnostics, logs, or
  ordinary configuration exports.
- Send a cookie from NTS-KE on protected NTP requests; handle server-issued
  replacement cookies and the no-cookie/placeholder path correctly. Never send
  a protected packet without the required authenticator inputs.
- Authenticate the complete RFC-defined packet/extension coverage before using
  time data. Reject bad tags, wrong direction keys, altered headers or
  extensions, malformed padding, unknown critical fields, and unauthenticated
  responses. Verify the authenticated server response is matched to the
  outstanding request and endpoint.
- Add round-trip, cookie rotation, exhausted-cookie, oversized-cookie,
  corrupted-cookie, bad-authenticator, and downgrade/fallback tests.

### 5. Replay protection and failure policy

- Generate a fresh, unpredictable per-request unique identifier/nonce as
  required by RFC 8915; enforce the protocol’s uniqueness and length rules and
  never reuse it across retries or process state unless the protocol explicitly
  permits that behavior.
- Verify the response authenticator and request binding before accepting the
  timestamp. Track outstanding requests and reject duplicates, delayed
  responses, cross-endpoint responses, nonce reuse, and replayed packets within
  the retry/acceptance window. Bound memory and expiry for replay state.
- Test duplicate capture/replay, reordered responses, retry races, process
  restart, clock rollback, and concurrent polling. A failed verification must
  be an NTS failure, not an implicit plain-UDP success; any configured fallback
  must be explicit and reported as unauthenticated.

### 6. Dependency and supply-chain review

Before selecting an implementation, document a dependency decision and review
its complete lockfile closure. Approval requires, at minimum:

- maintained upstream with compatible Rust/MSRV, API/runtime model, licensing,
  and support for this synchronous transport boundary;
- demonstrated RFC 8915 coverage and the interoperability/negative tests above,
  plus reviewable source, release provenance, and reproducible locked builds;
- a maintained TLS and AEAD backend with no insecure-default verifier or
  algorithm downgrade, clear `unsafe`/FFI exposure, sensible zeroization, and
  no unexpected network, telemetry, persistence, or secret logging;
- security-advisory and release history review for direct and transitive
  dependencies, including TLS, certificate parsing, trust roots, random number
  generation, AEAD, and serialization code; and
- a documented owner for upgrades, vulnerability response, trust-store changes,
  and regression testing. Do not add a crate merely because it advertises NTS or
  because it parses NTS extension fields.

The current dependency review is therefore **not an approval**: no NTS crate is
in `Cargo.toml`, and the existing boundary intentionally stops before all
security-sensitive operations listed above.

## Enablement gate

RFC 8915 may be enabled only after a reviewed backend implements every section,
all tests pass in CI (including interoperability tests), diagnostics distinguish
authenticated NTS from plain UDP and failure, and a maintainer signs off on the
TLS/certificate, cryptographic, cookie/replay, and dependency reviews. Until
then, keep NTS modes unsupported and do not change the current refusal behavior.
