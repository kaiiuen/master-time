[CmdletBinding(SupportsShouldProcess = $true)]
param(
    [Parameter(Mandatory = $true, Position = 0)]
    [ValidateNotNullOrEmpty()]
    [string]$Path,

    [Parameter(Mandatory = $true)]
    [ValidateNotNullOrEmpty()]
    [string]$CertificatePath,

    [Parameter(Mandatory = $true)]
    [ValidateNotNullOrEmpty()]
    [string]$CertificateThumbprint,

    [string]$SignToolPath
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

$PasswordEnvironmentVariable = 'MASTER_TIME_SIGNING_PASSWORD'
$normalizedThumbprint = ($CertificateThumbprint -replace '\s', '').ToUpperInvariant()
if ($normalizedThumbprint -notmatch '^[0-9A-F]{40}$') {
    throw 'CertificateThumbprint must be a 40-character SHA-1 thumbprint.'
}

if (-not (Test-Path -LiteralPath $Path)) {
    throw "Signing input was not found: $Path"
}
if (-not (Test-Path -LiteralPath $CertificatePath -PathType Leaf)) {
    throw "Certificate file was not found: $CertificatePath"
}

$password = [Environment]::GetEnvironmentVariable($PasswordEnvironmentVariable)
if ([string]::IsNullOrEmpty($password)) {
    throw "The $PasswordEnvironmentVariable environment variable must contain the PFX password; refusing to sign."
}

if ([string]::IsNullOrWhiteSpace($SignToolPath)) {
    $signTool = Get-Command 'signtool.exe' -CommandType Application -ErrorAction SilentlyContinue
    if ($null -eq $signTool) {
        throw 'Required tool not found: signtool.exe. Install the Windows SDK or pass -SignToolPath.'
    }
    $SignToolPath = $signTool.Source
}
elseif (-not (Test-Path -LiteralPath $SignToolPath -PathType Leaf)) {
    throw "signtool.exe was not found: $SignToolPath"
}

$signableExtensions = @('.exe', '.dll', '.msi', '.cab', '.cat', '.ps1', '.psm1', '.psd1')
$inputItem = Get-Item -LiteralPath $Path
if ($inputItem.PSIsContainer) {
    $files = @(Get-ChildItem -LiteralPath $inputItem.FullName -File -Recurse |
        Where-Object { $signableExtensions -contains $_.Extension.ToLowerInvariant() })
}
else {
    if ($signableExtensions -contains $inputItem.Extension.ToLowerInvariant()) {
        $files = @($inputItem)
    }
    else {
        $files = @()
    }
}

if ($files.Count -eq 0) {
    throw 'No signable files were found (supported extensions: .exe, .dll, .msi, .cab, .cat, .ps1, .psm1, .psd1).'
}

$securePassword = ConvertTo-SecureString -String $password -AsPlainText -Force
$certificateStore = New-Object System.Security.Cryptography.X509Certificates.X509Store('My', 'CurrentUser')
$certificate = $null
$temporaryCertificate = $null
$temporaryCertificateAdded = $false

try {
    $certificateStore.Open([System.Security.Cryptography.X509Certificates.OpenFlags]::ReadWrite)
    $existing = $certificateStore.Certificates.Find(
        [System.Security.Cryptography.X509Certificates.X509FindType]::FindByThumbprint,
        $normalizedThumbprint,
        $false)
    $certificateStore.Close()
    if ($existing.Count -eq 0) {
        $importedCertificates = @(Import-PfxCertificate `
            -FilePath (Resolve-Path -LiteralPath $CertificatePath).Path `
            -CertStoreLocation 'Cert:\CurrentUser\My' -Password $securePassword)
        $certificate = @($importedCertificates | Where-Object {
            $_.Thumbprint.ToUpperInvariant() -eq $normalizedThumbprint
        })[0]
        if ($null -eq $certificate) {
            throw 'The PFX did not contain the supplied certificate thumbprint.'
        }
        $temporaryCertificate = $certificate
        $temporaryCertificateAdded = $true
    }
    else {
        $certificate = $existing[0]
    }

    if ($certificate.Thumbprint.ToUpperInvariant() -ne $normalizedThumbprint) {
        throw 'Certificate thumbprint does not match the supplied CertificateThumbprint.'
    }
    if (-not $certificate.HasPrivateKey) {
        throw 'The supplied certificate does not contain a private key; refusing to sign.'
    }
    foreach ($file in $files) {
        if ($PSCmdlet.ShouldProcess($file.FullName, 'Authenticode sign')) {
            & $SignToolPath sign /sha1 $normalizedThumbprint /fd SHA256 /td SHA256 $file.FullName
            if ($LASTEXITCODE -ne 0) {
                throw "signtool sign failed for $($file.FullName) with exit code $LASTEXITCODE."
            }

            & $SignToolPath verify /pa /all $file.FullName
            if ($LASTEXITCODE -ne 0) {
                throw "Authenticode verification failed for $($file.FullName) with exit code $LASTEXITCODE."
            }
        }
    }
}
finally {
    if ($certificateStore.IsOpen) {
        $certificateStore.Close()
    }
    if ($temporaryCertificateAdded -and $null -ne $temporaryCertificate) {
        $certificateStore.Open([System.Security.Cryptography.X509Certificates.OpenFlags]::ReadWrite)
        try {
            $certificateStore.Remove($temporaryCertificate)
        }
        finally {
            $certificateStore.Close()
        }
    }
    if ($null -ne $securePassword) {
        $securePassword.Dispose()
    }
    if ($null -ne $certificate) {
        $certificate.Dispose()
    }
}

Write-Host "Successfully signed and verified $($files.Count) file(s)."
