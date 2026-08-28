# Windows packaging

`build.ps1` builds the `master-time` release binary and creates a Windows zip
archive.

## Usage

Run this script from any directory with PowerShell:

```powershell
.\packaging\windows\build.ps1 -Version 0.1.0
```

The script derives the repository root from the script location, so the current
working directory does not matter. The archive is written to
`packaging\windows\dist\master-time-<version>-windows-x86_64.zip`. Existing
archives and staging content for the same version are replaced; other files in
`dist` are left untouched.

The script validates that `cargo`, `rustc`, and the PowerShell
`Compress-Archive` cmdlet are available, then runs:

```text
cargo build --release --locked
```

The staged archive contains:

- `master-time.exe`
- this README
- `LICENSE-PLACEHOLDER.txt`

## License before distribution

This repository currently does not include a license file. The generated
`LICENSE-PLACEHOLDER.txt` is deliberately not a license and must not be
replaced by silently distributing the archive. Before publishing a build,
confirm the intended license with the project owner, add the complete license
text to the repository, and update the packaging script to include that file.
If the project is not licensed for redistribution, do not distribute the zip.

The application does not change the operating system clock. It sends NTP
requests over UDP port 123; see the repository documentation for product and
network details.
