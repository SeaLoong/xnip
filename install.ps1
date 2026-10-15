# Install a published release. Overrides: XNIP_VERSION, XNIP_INSTALL_DIR, XNIP_REPO.
$ErrorActionPreference = 'Stop'
$Repo = if ($env:XNIP_REPO) { $env:XNIP_REPO } else { 'SeaLoong/xnip' }
$Version = if ($env:XNIP_VERSION) { $env:XNIP_VERSION } else { 'latest' }
$InstallDir = if ($env:XNIP_INSTALL_DIR) { $env:XNIP_INSTALL_DIR } else { Join-Path $env:USERPROFILE 'bin' }
if ($Version -eq 'latest') {
    $Version = (Invoke-RestMethod -Uri "https://api.github.com/repos/$Repo/releases/latest").tag_name
}
if ($Version -match '^\d') { $Version = "v$Version" }
if ($Version -notmatch '^v[0-9][a-zA-Z0-9._-]*$') { throw "Invalid release tag: $Version" }
# Detect native OS architecture even in an emulated PowerShell process.
$Arch = if ($env:PROCESSOR_ARCHITEW6432) { $env:PROCESSOR_ARCHITEW6432 } else { $env:PROCESSOR_ARCHITECTURE }
switch ($Arch) {
    'AMD64' { $Target = 'x86_64-pc-windows-msvc' }
    'ARM64' { $Target = 'aarch64-pc-windows-msvc' }
    default { throw "Unsupported architecture: $Arch" }
}
$Asset = "xnip-$Version-$Target.zip"
$Url = "https://github.com/$Repo/releases/download/$Version/$Asset"
$TaskTempRoot = [IO.Path]::GetFullPath([IO.Path]::GetTempPath())
$Tmp = Join-Path $TaskTempRoot "xnip-install-$([guid]::NewGuid())"
New-Item -ItemType Directory -Path $Tmp | Out-Null
try {
    $ZipPath = Join-Path $Tmp $Asset
    Write-Host "Downloading $Url"
    Invoke-WebRequest -UseBasicParsing -Uri $Url -OutFile $ZipPath
    Invoke-WebRequest -UseBasicParsing -Uri "$Url.sha256" -OutFile "$ZipPath.sha256"
    $Expected = ((Get-Content -LiteralPath "$ZipPath.sha256" -Raw).Trim() -split '\s+')[0]
    $Actual = (Get-FileHash -LiteralPath $ZipPath -Algorithm SHA256).Hash
    if ($Expected -notmatch '^[a-fA-F0-9]{64}$' -or $Actual -ne $Expected) { throw 'SHA256 mismatch' }
    Expand-Archive -LiteralPath $ZipPath -DestinationPath $Tmp
    New-Item -ItemType Directory -Path $InstallDir -Force | Out-Null
    Copy-Item -LiteralPath (Join-Path $Tmp 'xnip.exe') -Destination (Join-Path $InstallDir 'xnip.exe') -Force
    Write-Host "Installed to $InstallDir\xnip.exe"
    Write-Host "Add $InstallDir to your user PATH through Windows Environment Variables if needed."
    & (Join-Path $InstallDir 'xnip.exe') --version
    if ($LASTEXITCODE -ne 0) { throw 'Installed binary failed its version check' }
} finally {
    $ResolvedTemp = [IO.Path]::GetFullPath($Tmp)
    if ($ResolvedTemp.StartsWith($TaskTempRoot, [StringComparison]::OrdinalIgnoreCase) -and
        [IO.Path]::GetFileName($ResolvedTemp).StartsWith('xnip-install-')) {
        Remove-Item -LiteralPath $ResolvedTemp -Recurse -Force
    }
}
