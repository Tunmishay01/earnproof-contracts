# Deployment Scripts

These scripts provide a reproducible Stellar testnet deployment path for the EarnProof Soroban contracts.

## Prerequisites

- Rust toolchain from `rust-toolchain.toml`
- `wasm32v1-none` target support from Rust `1.84.0` or newer
- Stellar CLI available as `stellar`
- A funded Stellar testnet identity configured in Stellar CLI
- No secret keys committed to the repository

## Build and Deploy

```powershell
.\scripts\deploy-testnet.ps1 -Source deployer -Admin G... -IssuerAddress G... -Output scripts\deployment-manifest.testnet.json
```

By default, Stellar CLI deploy and invoke calls are retried up to five times for transient RPC transport failures such as connection resets, send failures, timeouts, temporary unavailability, and sequence races. Override this with `-MaxRetries` when needed:

```powershell
.\scripts\deploy-testnet.ps1 -Source deployer -Admin G... -IssuerAddress G... -MaxRetries 8
```

The script:

- installs the `wasm32v1-none` target if needed;
- builds optimized release WASM artifacts with `stellar contract build`;
- deploys `protocol-config`, `issuer-registry`, and `proof-registry`;
- initializes each contract;
- approves schema version `1`;
- registers the backend issuer address before proof anchoring is enabled;
- writes a manifest with contract IDs, WASM hashes, admin address, schema versions, and CLI command evidence.

## Verify Manifest

```powershell
.\scripts\verify-manifest.ps1 -Manifest scripts\deployment-manifest.testnet.json
```

For the checked-in example manifest:

```powershell
.\scripts\verify-manifest.ps1 -Manifest scripts\deployment-manifest.example.json -AllowPlaceholders
```

The verifier checks the manifest shape and rejects placeholder contract IDs unless `-AllowPlaceholders` is explicitly supplied.

## Audit WASM Imports and Exports

After building the release artifacts, run the import/export audit to confirm no forbidden host symbols appear in the contracts:

```powershell
.\scripts\audit-wasm.ps1
```

This builds the `tools/audit-wasm` Rust binary, then checks every release WASM artifact against the allowlist in `scripts/wasm-import-policy.toml`. The script exits non-zero and prints each violation if any symbol is absent from the policy.

To audit already-built artifacts without rebuilding:

```powershell
.\scripts\audit-wasm.ps1 -SkipBuildContracts
```

To use a different policy file:

```powershell
.\scripts\audit-wasm.ps1 -Policy path/to/custom-policy.toml
```

The audit runs in CI on every pull request and push to `main`. See `.github/workflows/ci.yml`.

### Policy file

`scripts/wasm-import-policy.toml` is the reviewed allowlist. It lists:

- every permitted Soroban host function import (single-letter module name `::` function short-name);
- every permitted contract export (function names, `memory`, `__data_end`, `__heap_base`).

The `t` (test) host module is intentionally absent from the import allowlist. Any `t::*` import in a release artifact is a critical violation indicating that `soroban-sdk` testutils were compiled into production code.

To add a new permitted symbol, review it, add it to the appropriate section in `wasm-import-policy.toml`, and add a comment explaining why it is expected.
