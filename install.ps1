# xnip — installer (Windows PowerShell)
#
# Usage (latest):
#   iwr -useb https://github.com/SeaLoong/xnip/releases/download/v0.1.0/install.ps1 | iex
#
# Override version / location:
#   $env:XNIP_VERSION = 'v0.1.0'
#   $env:XNIP_INSTALL_DIR = "$env:USERPROFILE\bin"
#   iwr -useb ... | iex

$ErrorActionPreference = 'Stop'

$Repo       = if ($env:XNIP_REPO)        { $env:XNIP_REPO }        else { 'SeaLoong/xnip' }
$Version    = if ($env:XNIP_VERSION)     { $env:XNIP_VERSION }     else { '' }
$InstallDir = if ($env:XNIP_INSTALL_DIR) { $env:XNIP_INSTALL_DIR } else { Join-Path $env:USERPROFILE 'bin' }

if (-not $Version) {
    Write-Error "xnip install: set XNIP_VERSION=vX.Y.Z (latest channel not yet supported on Windows)."
}

$Arch = (Get-CimInstance Win32_Processor).Architecture
switch ($Arch) {
    9  { $Target = 'x86_64-pc-windows-msvc' }
    12 { $Target = 'aarch64-pc-windows-msvc' }  # ARM64
    default {
        Write-Error "xnip install: unsupported architecture: $Arch"
    }
}

$VersionStripped = $Version.TrimStart('v')
$Asset = "xnip-$VersionStripped-$Target.zip"
$Url   = "https://github.com/$Repo/releases/download/$Version/$Asset"

Write-Host "Downloading $Url"
$Tmp = New-Item -ItemType Directory -Path (Join-Path $env:TEMP "xnip-install-$([guid]::NewGuid())") -Force
$ZipPath = Join-Path $Tmp.FullName $Asset
Invoke-WebRequest -UseBasicParsing -Uri $Url -OutFile $ZipPath

Write-Host "Extracting"
Expand-Archive -Path $ZipPath -DestinationPath $Tmp.FullName -Force

if (-not (Test-Path $InstallDir)) {
    New-Item -ItemType Directory -Path $InstallDir -Force | Out-Null
}

$Bin = Join-Path $Tmp.FullName 'xnip.exe'
Move-Item -Force $Bin (Join-Path $InstallDir 'xnip.exe')

Write-Host ""
Write-Host "Installed to: $InstallDir\xnip.exe"
Write-Host "If $InstallDir is not on your PATH, add it:"
Write-Host "  setx PATH `"%PATH%;$InstallDir`""
Write-Host ""
& (Join-Path $InstallDir 'xnip.exe') --version
