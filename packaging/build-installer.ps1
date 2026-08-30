<#
.SYNOPSIS
    Builds Alidade and packages it into a Windows installer with Inno Setup 6.

.DESCRIPTION
    Reads the app version from Cargo.toml [workspace.package], builds the
    workspace in release mode (unless -SkipBuild), then runs ISCC against
    packaging\alidade.iss with /DAppVersion=<version>.

.PARAMETER SkipBuild
    Skip `cargo build --workspace --release`. Use this when the release
    binaries are already built (for example, a prior CI step).

.EXAMPLE
    .\packaging\build-installer.ps1

.EXAMPLE
    .\packaging\build-installer.ps1 -SkipBuild
#>
[CmdletBinding()]
param(
    [switch]$SkipBuild
)

$ErrorActionPreference = 'Stop'

$repoRoot = Split-Path -Parent $PSScriptRoot
$cargoTomlPath = Join-Path $repoRoot 'Cargo.toml'

$cargoToml = Get-Content -LiteralPath $cargoTomlPath -Raw
if ($cargoToml -notmatch '(?ms)\[workspace\.package\].*?^\s*version\s*=\s*"([^"]+)"') {
    throw "Could not find version in [workspace.package] in $cargoTomlPath"
}
$version = $Matches[1]
Write-Host "Alidade version: $version"

if (-not $SkipBuild) {
    Write-Host "Building workspace (release)..."
    Push-Location $repoRoot
    try {
        cargo build --workspace --release
        if ($LASTEXITCODE -ne 0) {
            throw "cargo build failed with exit code $LASTEXITCODE"
        }
    } finally {
        Pop-Location
    }
}

$isccCandidates = @()
$isccCommand = Get-Command ISCC -ErrorAction SilentlyContinue
if ($isccCommand) {
    $isccCandidates += $isccCommand.Source
}
$isccCandidates += Join-Path $env:LOCALAPPDATA 'Programs\Inno Setup 6\ISCC.exe'
$isccCandidates += 'C:\Program Files (x86)\Inno Setup 6\ISCC.exe'

$iscc = $isccCandidates | Where-Object { Test-Path -LiteralPath $_ } | Select-Object -First 1

if (-not $iscc) {
    throw "ISCC.exe (Inno Setup 6 command-line compiler) was not found. Install Inno Setup 6 " +
        "(winget install JRSoftware.InnoSetup) or add ISCC to PATH. Checked: $($isccCandidates -join ', ')"
}
Write-Host "Using ISCC: $iscc"

$issPath = Join-Path $repoRoot 'packaging\alidade.iss'
& $iscc "/DAppVersion=$version" $issPath
if ($LASTEXITCODE -ne 0) {
    throw "ISCC failed with exit code $LASTEXITCODE"
}

$outputExe = Join-Path $repoRoot "target\installer\alidade-setup-$version.exe"
if (-not (Test-Path -LiteralPath $outputExe)) {
    throw "Expected installer not found at $outputExe"
}
Write-Host "Installer built: $outputExe"
