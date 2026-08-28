[CmdletBinding()]
param(
    [Parameter(Position = 0)]
    [ValidateNotNullOrEmpty()]
    [string]$InstallDirectory = (Join-Path $env:LOCALAPPDATA 'Programs\Master Time'),

    [Parameter()]
    [ValidateNotNullOrEmpty()]
    [string]$StartMenuShortcutPath = (Join-Path ([Environment]::GetFolderPath('Programs')) 'Master Time.lnk'),

    [switch]$KeepStartMenuShortcut,
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
        '-StartMenuShortcutPath', $StartMenuShortcutPath
    )
    if ($KeepStartMenuShortcut) { $arguments += '-KeepStartMenuShortcut' }
    $process = Start-Process -FilePath $powershell -Verb RunAs -ArgumentList $arguments -Wait -PassThru
    exit $process.ExitCode
}

$destinationDirectory = [System.IO.Path]::GetFullPath($InstallDirectory)
$executable = Join-Path $destinationDirectory 'master-time.exe'
$shortcut = [System.IO.Path]::GetFullPath($StartMenuShortcutPath)

if ($destinationDirectory -eq [System.IO.Path]::GetPathRoot($destinationDirectory)) {
    throw "Refusing to uninstall from a filesystem root: $destinationDirectory"
}
if (-not $KeepStartMenuShortcut -and [System.IO.Path]::GetExtension($shortcut) -ine '.lnk') {
    throw "StartMenuShortcutPath must point to a .lnk file: $shortcut"
}

if (-not $KeepStartMenuShortcut -and (Test-Path -LiteralPath $shortcut -PathType Leaf)) {
    $removeShortcut = $true
    $shell = $null
    $link = $null
    try {
        $shell = New-Object -ComObject WScript.Shell
        try {
            $link = $shell.CreateShortcut($shortcut)
            $target = [System.IO.Path]::GetFullPath($link.TargetPath)
            $removeShortcut = ($target -ieq $executable)
        }
        finally {
            if ($null -ne $link) { [Runtime.InteropServices.Marshal]::ReleaseComObject($link) | Out-Null }
            if ($null -ne $shell) { [Runtime.InteropServices.Marshal]::ReleaseComObject($shell) | Out-Null }
        }
    }
    catch {
        throw "Could not validate the Start Menu shortcut before removal: $shortcut. $($_.Exception.Message)"
    }

    if ($removeShortcut) {
        Remove-Item -LiteralPath $shortcut -Force
        Write-Host "Removed Start Menu shortcut: $shortcut"
    }
    else {
        Write-Warning "Leaving unrelated Start Menu shortcut in place: $shortcut"
    }
}

if (Test-Path -LiteralPath $destinationDirectory -PathType Container) {
    $entries = @(Get-ChildItem -LiteralPath $destinationDirectory -Force)
    $onlyOwnedExecutable = $entries.Count -eq 1 -and
        $entries[0].Name -ieq 'master-time.exe' -and
        -not $entries[0].PSIsContainer

    if ($entries.Count -eq 0 -or $onlyOwnedExecutable) {
        Remove-Item -LiteralPath $destinationDirectory -Recurse -Force
        Write-Host "Removed installation directory: $destinationDirectory"
    }
    else {
        Write-Warning "Leaving installation directory because it contains files not owned by this script: $destinationDirectory"
        if (Test-Path -LiteralPath $executable -PathType Leaf) {
            Remove-Item -LiteralPath $executable -Force
        }
    }
}
else {
    Write-Host "Installation directory was already absent: $destinationDirectory"
}

if (Test-Path -LiteralPath $destinationDirectory) {
    if (Test-Path -LiteralPath $executable) {
        throw "Uninstallation validation failed; executable remains: $executable"
    }
}

Write-Host 'Uninstallation complete.'
