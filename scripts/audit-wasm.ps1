#!/usr/bin/env pwsh
#
# audit-wasm.ps1 — build the audit-wasm tool and run it against every
# release artifact, failing if any symbol is absent from the policy.
#
# Usage:
#   .\scripts\audit-wasm.ps1 [[-Policy] <path>] [-SkipBuildContracts]
#
# Parameters:
#   -Policy              Path to the policy file.
#                        Default: scripts/wasm-import-policy.toml
#   -SkipBuildContracts  Skip rebuilding the contract WASMs.  Useful when
#                        artifacts are already up to date.
#
# Prerequisites:
#   cargo and rustup must be on PATH (see rust-toolchain.toml).
#   The wasm32v1-none target must be installed (stellar contract build
#   installs it automatically; this script does not).

param(
    [string]$Policy = "scripts/wasm-import-policy.toml",
    [switch]$SkipBuildContracts
)

$ErrorActionPreference = "Stop"

function Assert-Command($Name) {
    if (-not (Get-Command $Name -ErrorAction SilentlyContinue)) {
        throw "Required command '$Name' was not found on PATH."
    }
}

Assert-Command "cargo"

$root = (Resolve-Path (Join-Path $PSScriptRoot "..")).Path
Push-Location $root

try {
    if (-not $SkipBuildContracts) {
        Write-Host "==> Building contract WASM artifacts"
        cargo run --manifest-path tools/audit-wasm/Cargo.toml --release -- --help 2>&1 | Out-Null
        # Build via stellar contract build so the correct target is used.
        Assert-Command "stellar"
        stellar contract build
        if ($LASTEXITCODE -ne 0) { throw "stellar contract build failed." }
    }

    Write-Host "==> Building audit-wasm tool"
    cargo build --manifest-path tools/audit-wasm/Cargo.toml --release --quiet
    if ($LASTEXITCODE -ne 0) { throw "cargo build for audit-wasm failed." }

    $tool = Join-Path $root "target/release/audit-wasm"
    if ($IsWindows -or $env:OS -eq "Windows_NT") {
        $tool = $tool + ".exe"
    }

    if (-not (Test-Path $tool)) {
        throw "audit-wasm binary not found at: $tool"
    }

    $wasmRoot = Join-Path $root "target/wasm32v1-none/release"
    $artifacts = @(
        (Join-Path $wasmRoot "protocol_config.wasm"),
        (Join-Path $wasmRoot "issuer_registry.wasm"),
        (Join-Path $wasmRoot "proof_registry.wasm")
    )

    foreach ($artifact in $artifacts) {
        if (-not (Test-Path $artifact)) {
            throw "WASM artifact not found: $artifact — run without -SkipBuildContracts"
        }
    }

    Write-Host "==> Running WASM import/export audit"
    & $tool --policy $Policy @artifacts
    if ($LASTEXITCODE -ne 0) {
        throw "audit-wasm reported violations.  Review output above and update the policy if the new symbols are intentional."
    }
} finally {
    Pop-Location
}
