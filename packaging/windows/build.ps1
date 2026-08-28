[CmdletBinding()]
param(
    [Parameter(Mandatory = $true, Position = 0)]
    [ValidateNotNullOrEmpty()]
    [string]$Version
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

if ($Version -notmatch '^[0-9A-Za-z][0-9A-Za-z._-]*$' -or $Version -in @('.', '..')) {
    throw "Version must contain only letters, numbers, '.', '_' or '-' and must not be a path segment."
}

$scriptDirectory = [System.IO.Path]::GetFullPath($PSScriptRoot)
$packagingDirectory = [System.IO.Path]::GetFullPath((Join-Path $scriptDirectory '..\..'))
$repositoryRoot = $packagingDirectory
$manifestPath = Join-Path $repositoryRoot 'Cargo.toml'
$readmePath = Join-Path $scriptDirectory 'README.md'
$distDirectory = Join-Path $scriptDirectory 'dist'
$stagingDirectory = Join-Path $distDirectory ("master-time-{0}-windows-x86_64" -f $Version)
$archivePath = Join-Path $distDirectory ("master-time-{0}-windows-x86_64.zip" -f $Version)
$binaryPath = Join-Path $repositoryRoot 'target\release\master-time.exe'

if (-not (Test-Path -LiteralPath $manifestPath -PathType Leaf)) {
    throw "Cargo manifest not found: $manifestPath"
}
if (-not (Test-Path -LiteralPath $readmePath -PathType Leaf)) {
    throw "Packaging README not found: $readmePath"
}

$cargo = Get-Command cargo -CommandType Application -ErrorAction SilentlyContinue
if ($null -eq $cargo) {
    throw 'Required tool not found: cargo'
}
$rustc = Get-Command rustc -CommandType Application -ErrorAction SilentlyContinue
if ($null -eq $rustc) {
    throw 'Required tool not found: rustc'
}
$compressArchive = Get-Command Compress-Archive -CommandType Cmdlet -ErrorAction SilentlyContinue
if ($null -eq $compressArchive) {
    throw 'Required PowerShell cmdlet not found: Compress-Archive'
}

if (-not (Test-Path -LiteralPath $distDirectory -PathType Container)) {
    New-Item -ItemType Directory -Path $distDirectory | Out-Null
}

Write-Host "Building master-time $Version in release mode..."
Push-Location $repositoryRoot
try {
    & $cargo.Source build --release --locked
    if ($LASTEXITCODE -ne 0) {
        throw "cargo build failed with exit code $LASTEXITCODE."
    }
}
finally {
    Pop-Location
}

if (-not (Test-Path -LiteralPath $binaryPath -PathType Leaf)) {
    throw "Release binary was not produced: $binaryPath"
}

if (Test-Path -LiteralPath $stagingDirectory) {
    Remove-Item -LiteralPath $stagingDirectory -Recurse -Force
}
if (Test-Path -LiteralPath $archivePath) {
    Remove-Item -LiteralPath $archivePath -Force
}
New-Item -ItemType Directory -Path $stagingDirectory | Out-Null

Copy-Item -LiteralPath $binaryPath -Destination (Join-Path $stagingDirectory 'master-time.exe')
Copy-Item -LiteralPath $readmePath -Destination (Join-Path $stagingDirectory 'README.md')
@'
LICENSE PLACEHOLDER - DO NOT DISTRIBUTE AS A FINAL LICENSE

This repository does not currently include an applicable license file. Before
redistributing this build, obtain confirmation of the intended license, add
the complete license text to the repository, and replace this placeholder in
the package. If redistribution is not authorized, do not distribute this zip.
'@ | Set-Content -LiteralPath (Join-Path $stagingDirectory 'LICENSE-PLACEHOLDER.txt') -Encoding UTF8

Compress-Archive -Path (Join-Path $stagingDirectory '*') -DestinationPath $archivePath -CompressionLevel Optimal
Write-Host "Created $archivePath"
