# Release readiness checklist

Use this checklist for each Master Time release. Complete it from the `master-time`
repository, record the evidence in the release notes, and do not publish an
artifact while a release gate is unresolved.

## Release record

- [ ] Release version and tag are agreed and match `Cargo.toml`.
- [ ] Supported Windows target(s), Rust toolchain version, build commands, and
      artifact names are recorded.
- [ ] Known limitations and user-visible changes are recorded.
- [ ] The final commit is identified, and generated binaries, installer output,
      certificates, and secrets are not added to the repository.

## License decision

- [ ] The project owner has explicitly approved the license for source,
      binaries, and redistribution.
- [ ] The complete, approved license text has replaced
      `LICENSE-PLACEHOLDER.txt` in the release source tree.
- [ ] Any third-party notices or attribution required by the dependency
      licenses are included with the distributable package.
- [ ] The license is included in the published source and is discoverable from
      the release documentation.

**Gate:** Do not distribute source, binaries, or installers while the repository
still contains only the license placeholder or licensing approval is ambiguous.

## Build and Windows runtime validation

- [ ] Run the standard checks:

      cargo fmt -- --check
      cargo check
      cargo test
      cargo build --release

- [ ] Build on the exact Windows target and architecture intended for release
      using a supported Visual Studio C++ build environment and Windows SDK.
- [ ] Test on a clean Windows environment, or document the controlled test
      image and its installed prerequisites.
- [ ] Launch the release executable from its packaged location.
- [ ] Verify the window opens, can be resized, and closes cleanly.
- [ ] Start polling and verify the displayed server, status, stratum, offset,
      round-trip delay, and root distance.
- [ ] Stop polling and confirm the application remains responsive and exits
      without an orphaned worker or unexpected error.
- [ ] Verify a successful local or approved NTP test path and a deliberately
      unreachable or blocked server path. Confirm failures are reported without
      changing the Windows system clock.
- [ ] Verify outbound DNS and UDP/123 behavior, the five-second receive bound,
      and that no unexpected network or telemetry request occurs.
- [ ] Record the Windows version, architecture, executable hash, test date,
      and tester.

## Signing secrets and trust

- [ ] Decide whether the Windows executable and installer will be Authenticode
      signed; document the decision before packaging.
- [ ] Use an organization-approved secret store or signing service for the
      certificate, private key, token, and any timestamp-service credentials.
- [ ] Confirm the signing identity, certificate chain, expiration, and
      timestamping policy with the release owner.
- [ ] Limit signing access to authorized release personnel and use short-lived
      credentials where supported.
- [ ] Sign only the final, checksum-bound artifacts; verify the signature on a
      clean Windows machine and check the certificate subject, validity, chain,
      and timestamp.
- [ ] Revoke or rotate temporary credentials after the release and record the
      key identifier, never the secret value.

**Gate:** Never commit, upload to an issue, place in an artifact directory, or
paste into release notes any private key, password, token, certificate bundle
containing private material, or secret-store export.

## Installer and package testing

The repository does not currently define an installer format or provide a
prebuilt installer. Before advertising an installer, choose the format,
installer owner, update behavior, and supported install scope.

- [ ] If the release includes an installer, build it from the exact verified
      release executable and record the installer tool/version and options.
- [ ] Install on a clean Windows test machine as a standard user and, if
      supported, as an administrator.
- [ ] Verify install location, shortcuts, displayed version, file permissions,
      uninstall behavior, and removal of application-owned files.
- [ ] Launch the installed application and repeat the polling, success, failure,
      stop, and clean-exit checks above.
- [ ] Test upgrade from the previous supported release and confirm that user
      configuration is preserved or that the documented migration behavior is
      followed. Do not delete or overwrite user files during rollback testing.
- [ ] Test a canceled, interrupted, or failed installation and confirm it does
      not leave a misleading shortcut or unusable partial installation.
- [ ] Record installer and uninstaller logs, test environment, result, and
      artifact hashes.

If no installer is shipped, mark the installer section not applicable in the
release record and publish the standalone executable instructions instead.

## Checksum verification

- [ ] Generate SHA-256 checksums only after signing and packaging are complete.
- [ ] Include one checksum entry for every published artifact, including the
      installer and any archive.
- [ ] Verify the generated checksum file against the local final artifacts.
- [ ] Independently download the published artifacts and checksum file, then
      verify them on a separate machine or release workspace. On Windows, one
      supported check is:

      certutil -hashfile Master-Time-<version>-windows-<arch>.exe SHA256

- [ ] Confirm the independently calculated digest exactly matches the published
      value, artifact name, version, and target architecture.
- [ ] Publish checksums beside the artifacts and retain the verification output
      with the release record.

A changed artifact requires a new checksum and a new release review; never reuse
checksums after signing, repackaging, or reissuing an artifact.

## Rollback and incident readiness

- [ ] Identify the last verified release and retain access to its exact signed
      artifacts and checksums.
- [ ] Confirm the distribution owner can hide, unpublish, or stop promoting the
      affected artifact and can communicate a rollback.
- [ ] Prepare a rollback note naming the affected version and platforms, the
      known impact, the safe version, workaround, and user action required.
- [ ] If an installer or update channel is used, verify that rollback does not
      remove user configuration or downgrade it into an incompatible format.
- [ ] Preserve logs and evidence needed to investigate, and document whether
      signing credentials or distribution accounts may have been exposed.
- [ ] After rollback, reproduce the issue, approve the corrective release, and
      rerun the relevant checklist sections before republishing.

## NTS status and release wording

- [ ] Confirm the release describes the current transport accurately: Master
      Time supports ordinary NTP over UDP; it does **not** provide authenticated
      NTS.
- [ ] Do not describe plain UDP as NTS and do not claim NTS-KE, TLS 1.3
      certificate verification, packet AEAD protection, cookies, authenticated
      NTP extension fields, or replay protection.
- [ ] If NTS is mentioned in release notes or UI documentation, state that the
      NTS boundary is not an implementation and that NTS-required endpoints are
      unsupported.
- [ ] Do not add or enable an NTS dependency as part of release packaging
      without a dedicated security review and interoperability test plan.
- [ ] Record NTS as **not supported in this release** unless all required
      protocol components have been implemented, reviewed, and tested.

## Final approval

- [ ] License gate is complete.
- [ ] Windows runtime gate is complete.
- [ ] Signing and installer decisions are documented, with secrets handled only
      through approved systems.
- [ ] Checksums have been independently verified.
- [ ] A tested rollback path and last-known-good release are recorded.
- [ ] NTS status and all known limitations are stated accurately.
- [ ] Release owner approves publication.

See [Release workflow](release.md), [Installation and running](installation.md),
and [Testing](testing.md) for the repository's current build and runtime context.
