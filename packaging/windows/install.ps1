[CmdletBinding()]
param(
    [Parameter(Position = 0)]
    [ValidateNotNullOrEmpty()]
    [string]$InstallDirectory = (Join-Path $env:LOCALAPPDATA 'Programs\Master Time'),

    [Parameter()]
    [ValidateNotNullOrEmpty()]
    [string]$ExecutablePath = (Join-Path $PSScriptRoot 'master-time.exe'),

    [Parameter()]
    [ValidateNotNullOrEmpty()]
    [string]$StartMenuShortcutPath = (Join-Path ([Environment]::GetFolderPath('Programs')) 'Master Time.lnk'),

    [switch]$NoStartMenuShortcut,
    [switch]$RunAsAdministrator
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

function Test-Administrator {
    $identity = [Security.Principal.WindowsIdentity]::GetCurrent()
    $principal = [Security.Principal.WindowsPrincipal]::new($identity)
    return $principal.IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)
}

if ($RunAsAdministrator -and -not (Test-Administrator)) {
    $powershell = (Get-Command powershell.exe -CommandType Application -ErrorAction Stop).Source
    $arguments = @(
        '-NoProfile'
        '-ExecutionPolicy', 'Bypass'
        '-File', $PSCommandPath
        '-InstallDirectory', $InstallDirectory
        '-ExecutablePath', $ExecutablePath
        '-StartMenuShortcutPath', $StartMenuShortcutPath
    )
    if ($NoStartMenuShortcut) { $arguments += '-NoStartMenuShortcut' }
    $process = Start-Process -FilePath $powershell -Verb RunAs -ArgumentList $arguments -Wait -PassThru
    exit $process.ExitCode
}

$source = [System.IO.Path]::GetFullPath($ExecutablePath)
$destinationDirectory = [System.IO.Path]::GetFullPath($InstallDirectory)
$destination = Join-Path $destinationDirectory 'master-time.exe'
$shortcut = [System.IO.Path]::GetFullPath($StartMenuShortcutPath)

if (-not (Test-Path -LiteralPath $source -PathType Leaf)) {
    throw "Packaged executable was not found: $source"
}
if ([System.IO.Path]::GetExtension($source) -ine '.exe') {
    throw "ExecutablePath must point to an .exe file: $source"
}
if ($source -eq $destination) {
    throw 'ExecutablePath and InstallDirectory must not refer to the same file.'
}
if ($destinationDirectory -eq [System.IO.Path]::GetPathRoot($destinationDirectory)) {
    throw "Refusing to install directly into a filesystem root: $destinationDirectory"
}
if (-not $NoStartMenuShortcut -and [System.IO.Path]::GetExtension($shortcut) -ine '.lnk') {
    throw "StartMenuShortcutPath must point to a .lnk file: $shortcut"
}

if (-not (Test-Path -LiteralPath $destinationDirectory -PathType Container)) {
    New-Item -ItemType Directory -Path $destinationDirectory -Force | Out-Null
}

Copy-Item -LiteralPath $source -Destination $destination -Force
if (-not (Test-Path -LiteralPath $destination -PathType Leaf)) {
    throw "Installation validation failed; executable was not created: $destination"
}

if (-not $NoStartMenuShortcut) {
    $shortcutDirectory = Split-Path -Parent $shortcut
    if (-not (Test-Path -LiteralPath $shortcutDirectory -PathType Container)) {
        New-Item -ItemType Directory -Path $shortcutDirectory -Force | Out-Null
    }

    $shell = $null
    $link = $null
    try {
        $shell = New-Object -ComObject WScript.Shell
        $link = $shell.CreateShortcut($shortcut)
        $link.TargetPath = $destination
        $link.WorkingDirectory = $destinationDirectory
        $link.Description = 'Master Time'
        $link.IconLocation = "$destination,0"
        $link.Save()
    }
    finally {
        if ($null -ne $link) { [Runtime.InteropServices.Marshal]::ReleaseComObject($link) | Out-Null }
        if ($null -ne $shell) { [Runtime.InteropServices.Marshal]::ReleaseComObject($shell) | Out-Null }
    }

    if (-not (Test-Path -LiteralPath $shortcut -PathType Leaf)) {
        throw "Installation validation failed; Start Menu shortcut was not created: $shortcut"
    }
}

Write-Host "Installed master-time to $destinationDirectory"
if (-not $NoStartMenuShortcut) { Write-Host "Created Start Menu shortcut at $shortcut" }
