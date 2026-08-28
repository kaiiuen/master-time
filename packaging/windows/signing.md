# Windows Authenticode signing

Signing is optional and is intentionally separate from the existing packaging
scripts. `sign.ps1` does not build an artifact or alter any packaging workflow;
it signs supported files in a file or directory that you explicitly provide.

## Prerequisites

- Windows PowerShell 5.1 or PowerShell 7 on Windows
- `signtool.exe` from the Windows SDK, either on `PATH` or supplied with
  `-SignToolPath`
- A PFX certificate containing a private key
- The PFX SHA-1 thumbprint, supplied explicitly with `-CertificateThumbprint`
- The PFX password in the environment variable
  `MASTER_TIME_SIGNING_PASSWORD`

The password is read from the environment and is not stored in this repository.
Do not put it in a command line, script, shell history, CI log, or source file.
For an interactive PowerShell session, set it without displaying it:

```powershell
$securePassword = Read-Host 'PFX password' -AsSecureString
$env:MASTER_TIME_SIGNING_PASSWORD = (New-Object System.Net.NetworkCredential('', $securePassword)).Password
```

Use a secret-store or masked environment variable in CI. Clear the variable
when finished (`Remove-Item Env:MASTER_TIME_SIGNING_PASSWORD`).

## Usage

Sign one artifact:

```powershell
$thumbprint = '0123456789ABCDEF0123456789ABCDEF01234567'
.\packaging\windows\sign.ps1 `
    -Path .\packaging\windows\dist\master-time.exe `
    -CertificatePath C:\secure\release-signing.pfx `
    -CertificateThumbprint $thumbprint
```

Sign all supported files below a directory:

```powershell
.\packaging\windows\sign.ps1 `
    -Path .\packaging\windows\dist\master-time-0.1.0-windows-x86_64 `
    -CertificatePath C:\secure\release-signing.pfx `
    -CertificateThumbprint $thumbprint `
    -SignToolPath 'C:\Program Files (x86)\Windows Kits\10\bin\x64\signtool.exe'
```

Supported extensions are `.exe`, `.dll`, `.msi`, `.cab`, `.cat`, `.ps1`,
`.psm1`, and `.psd1`. A directory with no supported files is rejected.
`-WhatIf` can be used to review the files that would be signed without making
changes.

The script validates that the PFX thumbprint matches the explicit thumbprint,
requires a private key, signs with SHA-256, and verifies each file with
`signtool verify /pa /all`. The PFX is temporarily imported into the current
user's `My` certificate store so the password is not passed to `signtool.exe`;
it is removed again after signing, including on failure. If the same thumbprint
was already present in that store, it is reused and is not removed.

## Safe refusal and verification

The script refuses to sign when any of these are missing or invalid:

- the input path or PFX path;
- `MASTER_TIME_SIGNING_PASSWORD` (including an empty value);
- `signtool.exe` or an explicitly supplied tool path;
- a 40-character hexadecimal thumbprint;
- a matching certificate with a private key; or
- at least one supported input file.

To independently verify a signed file with the Windows SDK:

```powershell
signtool verify /pa /all .\packaging\windows\dist\master-time.exe
```

You can also inspect the signature in Explorer: **Properties → Digital
Signatures → Details**. Verification may still report a chain/trust warning
when the signing certificate is private, expired, or not trusted on the machine;
that is distinct from the file being structurally signed. Confirm the publisher,
certificate chain, timestamp policy, and intended release artifact before
publishing.
