# Offline installer contracts. Run after cargo build --locked on Windows.
$ErrorActionPreference = 'Stop'
$TaskRepoRoot = Split-Path $PSScriptRoot -Parent
$TaskBinary = Join-Path $TaskRepoRoot 'target/debug/xnip.exe'
if (-not (Test-Path -LiteralPath $TaskBinary)) { throw 'Run cargo build --locked first' }
$TaskTempRoot = [IO.Path]::GetFullPath([IO.Path]::GetTempPath())
$TaskFixtureRoot = Join-Path $TaskTempRoot "xnip-installer-contract-$([guid]::NewGuid())"
New-Item -ItemType Directory -Path $TaskFixtureRoot | Out-Null
$TaskSavedEnvironment = @{}
foreach ($Name in @('XNIP_VERSION', 'XNIP_INSTALL_DIR', 'XNIP_REPO', 'PROCESSOR_ARCHITECTURE', 'PROCESSOR_ARCHITEW6432')) {
    $TaskSavedEnvironment[$Name] = [Environment]::GetEnvironmentVariable($Name)
}
$global:TaskRequests = @()
$global:TaskExpectedTarget = ''
$global:TaskChecksum = ''
$global:TaskDownloadFails = $false
$script:TaskCaseCount = 0

# The real installer executes against deterministic HTTP fixtures.
function Invoke-RestMethod {
    param($Uri)
    if ($Uri -ne 'https://api.github.com/repos/fixture/project/releases/latest') {
        throw "Unexpected latest URL: $Uri"
    }
    $global:TaskRequests += $Uri
    return @{ tag_name = 'v0.2.0' }
}
function Invoke-WebRequest {
    param([switch]$UseBasicParsing, $Uri, $OutFile)
    $Expected = "https://github.com/fixture/project/releases/download/v0.2.0/xnip-v0.2.0-$global:TaskExpectedTarget.zip"
    if ($Uri -ne $Expected -and $Uri -ne "$Expected.sha256") { throw "Unexpected asset URL: <$Uri>; expected <$Expected>" }
    $global:TaskRequests += $Uri
    if ($global:TaskDownloadFails) { throw 'fixture download failure' }
    if ($Uri.EndsWith('.sha256')) {
        Set-Content -LiteralPath $OutFile -Value "$global:TaskChecksum  release.zip"
    } else {
        Copy-Item -LiteralPath (Join-Path $TaskFixtureRoot 'release.zip') -Destination $OutFile
    }
}

function Invoke-InstallerCase {
    param(
        [string]$Version = 'latest',
        [string]$Architecture = 'AMD64',
        [string]$NativeArchitecture = '',
        [string]$Target = 'x86_64-pc-windows-msvc',
        [string]$ErrorPattern = '',
        [switch]$BadChecksum,
        [switch]$DownloadFailure
    )
    $global:TaskRequests = @()
    $global:TaskExpectedTarget = $Target
    $global:TaskChecksum = if ($BadChecksum) { '0' * 64 } else { $TaskArchiveHash }
    $global:TaskDownloadFails = [bool]$DownloadFailure
    $env:XNIP_VERSION = $Version
    $env:XNIP_REPO = 'fixture/project'
    $env:PROCESSOR_ARCHITECTURE = $Architecture
    $env:PROCESSOR_ARCHITEW6432 = $NativeArchitecture
    $env:XNIP_INSTALL_DIR = Join-Path $TaskFixtureRoot "installation with spaces $([guid]::NewGuid())"
    New-Item -ItemType Directory -Path $env:XNIP_INSTALL_DIR | Out-Null
    $Installed = Join-Path $env:XNIP_INSTALL_DIR 'xnip.exe'
    [IO.File]::WriteAllText($Installed, 'original installation')
    $Failure = $null
    try { & (Join-Path $TaskRepoRoot 'install.ps1') } catch { $Failure = $_ }
    if ($ErrorPattern) {
        if (-not $Failure -or $Failure.Exception.Message -notmatch $ErrorPattern) {
            throw "Expected '$ErrorPattern', got '$Failure'"
        }
        if ([IO.File]::ReadAllText($Installed) -ne 'original installation') {
            throw 'Failed install overwrote the existing installation'
        }
    } else {
        if ($Failure) { throw $Failure }
        if ((Get-FileHash -LiteralPath $Installed).Hash -ne $TaskBinaryHash) { throw 'Installed bytes differ' }
        if ($global:TaskRequests.Count -lt 2) { throw 'Archive and checksum were not both requested' }
    }
    if (@(Get-ChildItem -LiteralPath $env:XNIP_INSTALL_DIR).Count -ne 1) {
        throw 'Installer left staging files in the destination'
    }
    $script:TaskCaseCount++
}

try {
    $TaskArchivePath = Join-Path $TaskFixtureRoot 'release.zip'
    Compress-Archive -LiteralPath $TaskBinary -DestinationPath $TaskArchivePath
    $TaskArchiveHash = (Get-FileHash -LiteralPath $TaskArchivePath -Algorithm SHA256).Hash
    $TaskBinaryHash = (Get-FileHash -LiteralPath $TaskBinary -Algorithm SHA256).Hash
    foreach ($Version in @('latest', 'v0.2.0', '0.2.0')) {
        Invoke-InstallerCase -Version $Version
    }
    Invoke-InstallerCase -Architecture 'ARM64' -Target 'aarch64-pc-windows-msvc'
    Invoke-InstallerCase -Architecture 'x86' -NativeArchitecture 'ARM64' -Target 'aarch64-pc-windows-msvc'
    Invoke-InstallerCase -BadChecksum -ErrorPattern 'SHA256 mismatch'
    Invoke-InstallerCase -DownloadFailure -ErrorPattern 'fixture download failure'
    Invoke-InstallerCase -Version 'invalid' -ErrorPattern 'Invalid release tag'
    Invoke-InstallerCase -Version 'v0.2.0/escape' -ErrorPattern 'Invalid release tag'
    Invoke-InstallerCase -Version 'v0.2.0' -Architecture 'Unsupported' -ErrorPattern 'Unsupported architecture'
    if ($script:TaskCaseCount -ne 10) { throw "Expected 10 installer cases, ran $script:TaskCaseCount" }
    Write-Host "PASS: $script:TaskCaseCount offline installer contract cases"
} finally {
    foreach ($Name in $TaskSavedEnvironment.Keys) {
        [Environment]::SetEnvironmentVariable($Name, $TaskSavedEnvironment[$Name])
    }
    $TaskResolvedFixture = [IO.Path]::GetFullPath($TaskFixtureRoot)
    if ($TaskResolvedFixture.StartsWith($TaskTempRoot, [StringComparison]::OrdinalIgnoreCase) -and
        [IO.Path]::GetFileName($TaskResolvedFixture).StartsWith('xnip-installer-contract-')) {
        Remove-Item -LiteralPath $TaskResolvedFixture -Recurse -Force
    }
}
