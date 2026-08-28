# Release workflow

There is no checked-in CI or release workflow in this repository today. Until one is added, use the following manual workflow for a reproducible product release.

## Prepare

1. Confirm the intended version in `Cargo.toml` and update it before packaging. Keep the package version and release tag aligned.
2. Review user-facing changes, especially network behavior, configuration compatibility, and platform support.
3. Update the root README or a release note with behavior changes and known limitations.
4. Confirm the repository's licensing decision. The current tree does not include a license file.

Source and workflow files are outside the documentation change scope; do not add release automation without a separate product decision.

## Verify

From `master-time`, run:

```text
cargo fmt -- --check
cargo check
cargo test
cargo build --release
```

Run the release build on each target platform you intend to support. Launch the resulting binary, start one poll, verify the metrics and error display, stop polling, and close the application cleanly. Test at least one unreachable or blocked server path without relying on an internet outage to create the failure.

For a network-related release, verify that requests still use UDP/123, the five-second receive bound remains effective, malformed responses are rejected, and no unexpected endpoint or telemetry request was introduced. For a storage-related release, test both a valid file and malformed/old files, and document compatibility expectations.

## Package

Package the platform-specific binary from `target/release/` with only the files needed for users to run it. Do not distribute `target/` itself as an application bundle. Include:

- the executable;
- the final version and target platform in the artifact name;
- user-facing documentation or a link to it;
- the applicable license once established;
- checksums generated from the final artifacts.

The repository currently does not define installer formats, signing certificates, update channels, or an artifact hosting service. Decide and document those before promising installers or automatic updates.

## Publish and record

Create a release tag only after verification and packaging are complete. Publish the artifacts and checksums through the chosen distribution channel, then record the exact Rust version, target triple, commands run, and known limitations. Do not commit generated binaries or secrets to the repository.

## Rollback

If a release has a regression, stop promoting the affected artifact, identify the last verified version, and communicate the affected platforms and workaround. Configuration format changes should be rolled back carefully: preserve user files and provide a migration path rather than deleting or overwriting them.
