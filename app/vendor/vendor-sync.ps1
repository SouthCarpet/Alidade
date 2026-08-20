<#
Refreshes the vendored copy under app/vendor/ from the private design-system
tree. See README.md in this directory for what is vendored, why, and when to
run this.

Usage (from anywhere, run with PowerShell):
  pwsh app/vendor/vendor-sync.ps1          # refresh the vendored copy
  pwsh app/vendor/vendor-sync.ps1 -Check   # report drift only, change nothing

This script only works on a machine that has the private design-system tree
checked out next to this repo's ecosystem root. A clone of this public repo
does not have that tree and cannot run this script — that is by design: the
private tree is not published, and does not need to be for the app to build
(see README.md). Running this script from a clone fails loudly, on purpose,
rather than doing nothing and looking like it worked.
#>
param(
    [switch]$Check
)

$ErrorActionPreference = "Stop"

$SourceRoot = "A:\projects-vault\design"
$VendorRoot = $PSScriptRoot

if (-not (Test-Path -LiteralPath $SourceRoot)) {
    Write-Error @"
vendor-sync: source tree not found at $SourceRoot

This is expected on a clone of this repo — the private design-system tree
lives outside it and is never published. The vendored copy under
$VendorRoot is what the build actually uses; it does not need
refreshing to build or run. This script only works on the machine (or a
machine) that has $SourceRoot checked out.
"@
    exit 1
}

# Explicit allowlist, not a recursive directory copy: adding a new file under
# $SourceRoot does NOT get vendored automatically. That is deliberate — see
# "Only vendor what Alidade actually needs" in README.md. Extend this list by
# hand if Alidade starts needing something it does not need today.
#
# CargoToml = the one file that is not a byte-for-byte copy of its source;
# see $CargoTomlMarker below for what and why.
$Files = @(
    @{ Source = "crates\design-tokens\Cargo.toml";   Vendor = "crates\design-tokens\Cargo.toml";   CargoToml = $true }
    @{ Source = "crates\design-tokens\build.rs";      Vendor = "crates\design-tokens\build.rs";      CargoToml = $false }
    @{ Source = "crates\design-tokens\src\lib.rs";    Vendor = "crates\design-tokens\src\lib.rs";    CargoToml = $false }
    @{ Source = "crates\design-tokens\src\color.rs";  Vendor = "crates\design-tokens\src\color.rs";  CargoToml = $false }
    @{ Source = "crates\design-tokens\src\schema.rs"; Vendor = "crates\design-tokens\src\schema.rs"; CargoToml = $false }
    @{ Source = "tokens\base.json";                   Vendor = "tokens\base.json";                   CargoToml = $false }
    @{ Source = "tokens\apps\alidade.json";            Vendor = "tokens\apps\alidade.json";            CargoToml = $false }
    @{ Source = "assets\fonts\Inter-Regular.ttf";      Vendor = "fonts\Inter-Regular.ttf";             CargoToml = $false }
    @{ Source = "assets\fonts\Inter-Medium.ttf";       Vendor = "fonts\Inter-Medium.ttf";              CargoToml = $false }
    @{ Source = "assets\fonts\Inter-SemiBold.ttf";     Vendor = "fonts\Inter-SemiBold.ttf";            CargoToml = $false }
    @{ Source = "assets\fonts\OFL.txt";                Vendor = "fonts\OFL.txt";                       CargoToml = $false }
)

# Appended to the vendored design-tokens Cargo.toml only. This crate was
# never linted by this repo's CI before it was vendored (it lived outside the
# Cargo workspace); vendoring it inside app/ makes `cargo clippy --workspace
# --all-targets -- -D warnings` see it for the first time, surfacing two
# lints its own (separately linted) source repo does not gate on. Rewriting
# either would diverge this copy from its source line-for-line, the opposite
# of what vendoring the crate whole is for, so this appends a package-scoped
# allow for both instead.
$CargoTomlMarkerLines = @(
    ""
    "# The lines below are this vendored copy's one addition on top of the source"
    "# crate (``A:\projects-vault\design\crates\design-tokens\Cargo.toml``, not part"
    "# of this repo — see ../../README.md). This crate was never linted by this"
    "# repo's CI before it was vendored (it lived outside the Cargo workspace);"
    "# vendoring it inside app/ makes ``cargo clippy --workspace --all-targets --"
    "# -D warnings`` see it for the first time, surfacing two lints its own"
    "# (separately linted) source repo does not gate on: build.rs's generated-code"
    "# writer uses ``write!(..., ""...\n"", ...)`` throughout rather than ``writeln!``"
    "# (literal trailing newlines, matching the exact rendered Rust/CSS output),"
    "# and the OKLCH->sRGB conversion in build.rs bakes 9-decimal-digit f32"
    "# literals into the generated Theme consts (deliberate — see color.rs's"
    "# ``rust_lit``). Rewriting either would diverge this copy from its source"
    "# line-for-line, which is the opposite of what vendoring the crate whole is"
    "# for, so this allows exactly those two lints in this one package instead."
    "# Re-add this block after every ``vendor-sync.ps1`` refresh; the sync script's"
    "# plain file diff will flag its absence as drift."
    "[lints.clippy]"
    "write_with_newline = ""allow"""
    "excessive_precision = ""allow"""
)
$CargoTomlMarker = ($CargoTomlMarkerLines -join "`n") + "`n"
$Utf8NoBom = New-Object System.Text.UTF8Encoding($false)

function Get-ExpectedBytes {
    param($SourcePath, [bool]$IsCargoToml)
    $bytes = [System.IO.File]::ReadAllBytes($SourcePath)
    if (-not $IsCargoToml) {
        return $bytes
    }
    $sourceText = $Utf8NoBom.GetString($bytes)
    return $Utf8NoBom.GetBytes($sourceText + $CargoTomlMarker)
}

$drift = @()
$updated = @()
$unchanged = @()

foreach ($f in $Files) {
    $sourcePath = Join-Path $SourceRoot $f.Source
    $vendorPath = Join-Path $VendorRoot $f.Vendor

    if (-not (Test-Path -LiteralPath $sourcePath)) {
        Write-Error "vendor-sync: expected source file missing: $sourcePath"
        exit 1
    }

    $expected = Get-ExpectedBytes -SourcePath $sourcePath -IsCargoToml $f.CargoToml
    $currentExists = Test-Path -LiteralPath $vendorPath
    $current = if ($currentExists) { [System.IO.File]::ReadAllBytes($vendorPath) } else { $null }
    $matches = $currentExists -and ($current.Length -eq $expected.Length) -and (-not (Compare-Object $current $expected -SyncWindow 0))

    if ($matches) {
        $unchanged += $f.Vendor
        continue
    }

    if ($Check) {
        $drift += $f.Vendor
        continue
    }

    $vendorDir = Split-Path -Parent $vendorPath
    if (-not (Test-Path -LiteralPath $vendorDir)) {
        New-Item -ItemType Directory -Force -Path $vendorDir | Out-Null
    }
    [System.IO.File]::WriteAllBytes($vendorPath, $expected)
    $updated += $f.Vendor
}

if ($Check) {
    Write-Output "vendor-sync -Check: $($unchanged.Count) unchanged, $($drift.Count) drifted"
    foreach ($d in $drift) { Write-Output "  DRIFT: $d" }
    if ($drift.Count -gt 0) {
        Write-Output ""
        Write-Output "Run 'pwsh app/vendor/vendor-sync.ps1' (no -Check) to refresh, then review the diff before committing."
        exit 1
    }
    exit 0
}

Write-Output "vendor-sync: $($updated.Count) updated, $($unchanged.Count) already current"
foreach ($u in $updated) { Write-Output "  updated: $u" }
if ($updated.Count -gt 0) {
    Write-Output ""
    Write-Output "Review the diff, then run cargo build --workspace and cargo test --workspace before committing."
}
