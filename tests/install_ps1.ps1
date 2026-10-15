# Offline contract tests using a locally built executable and mocked HTTP calls.
$ErrorActionPreference = 'Stop'
$RepoRoot = Split-Path $PSScriptRoot -Parent
$Binary = Join-Path $RepoRoot 'target/debug/xnip.exe'
if (-not (Test-Path -LiteralPath $Binary)) { throw 'Run cargo build --locked first' }
$TestRoot = Join-Path ([IO.Path]::GetTempPath()) "xnip-installer-test-$([guid]::NewGuid())"
New-Item -ItemType Directory -Path $TestRoot | Out-Null
$SavedEnv = @{}
foreach ($Name in @('XNIP_VERSION', 'XNIP_INSTALL_DIR', 'PROCESSOR_ARCHITECTURE', 'PROCESSOR_ARCHITEW6432')) {
    $SavedEnv[$Name] = [Environment]::GetEnvironmentVariable($Name)
}
try {
    $FixtureZip = Join-Path $TestRoot 'fixture.zip'
    Compress-Archive -LiteralPath $Binary -DestinationPath $FixtureZip
    $FixtureHash = (Get-FileHash -LiteralPath $FixtureZip -Algorithm SHA256).Hash
    $env:PROCESSOR_ARCHITECTURE = 'AMD64'
    $env:PROCESSOR_ARCHITEW6432 = ''
    $env:XNIP_INSTALL_DIR = Join-Path $TestRoot 'install space'
    function Invoke-RestMethod { param($Uri) return @{ tag_name = 'v0.1.0' } }
    function Invoke-WebRequest {
        param([switch]$UseBasicParsing, $Uri, $OutFile)
        if ($Uri -notmatch '/v0.1.0/xnip-v0.1.0-x86_64-pc-windows-msvc.zip') { throw "Unexpected URL: $Uri" }
        if ($Uri.EndsWith('.sha256')) {
            Set-Content -LiteralPath $OutFile -Value "$FixtureHash  fixture.zip"
        } else {
            Copy-Item -LiteralPath $FixtureZip -Destination $OutFile
        }
    }
    foreach ($Version in @('latest', 'v0.1.0', '0.1.0')) {
        $env:XNIP_VERSION = $Version
        & (Join-Path $RepoRoot 'install.ps1')
        $Installed = Join-Path $env:XNIP_INSTALL_DIR 'xnip.exe'
        if ((Get-FileHash $Installed).Hash -ne (Get-FileHash $Binary).Hash) { throw 'Installed bytes differ' }
    }
    $FixtureHash = '0' * 64
    $Rejected = $false
    try { & (Join-Path $RepoRoot 'install.ps1') } catch {
        if ($_.Exception.Message -notmatch 'SHA256 mismatch') { throw }
        $Rejected = $true
    }
    if (-not $Rejected) { throw 'Corrupt download was accepted' }
    if ((Get-FileHash $Installed).Hash -ne (Get-FileHash $Binary).Hash) { throw 'Existing installation changed' }
    Write-Host 'PASS: latest, explicit tags, normalized tags, checksum failure'
} finally {
    foreach ($Name in $SavedEnv.Keys) {
        [Environment]::SetEnvironmentVariable($Name, $SavedEnv[$Name])
    }
    $Resolved = [IO.Path]::GetFullPath($TestRoot)
    $TempRoot = [IO.Path]::GetFullPath([IO.Path]::GetTempPath())
    if ($Resolved.StartsWith($TempRoot, [StringComparison]::OrdinalIgnoreCase) -and
        [IO.Path]::GetFileName($Resolved).StartsWith('xnip-installer-test-')) {
        Remove-Item -LiteralPath $Resolved -Recurse -Force
    }
}
