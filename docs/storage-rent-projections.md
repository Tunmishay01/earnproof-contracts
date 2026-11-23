# Storage Rent Projections

This report is generated from synthetic fixtures. It estimates storage exposure in
encoded bytes and byte-ledgers; it does not replace transaction resource limits or
claim a network fee, because ledger rent pricing is network-configurable.

## Reproducibility

- Model version: `1`
- Rust: `1.98.0`
- Soroban SDK: `27.0.0`
- Renewal horizon: `500,000` ledgers
- Scenario SHA-256: `6292ff4df1a523f79ac0ca18a77ce7e06c1483da7811083ef5cb8e45dc81da07`
- Generate: `python scripts/generate-storage-rent-report.py`
- Verify: `python scripts/generate-storage-rent-report.py --check`

Encoded sizes are deterministic version-1 synthetic fixture lengths. Update them only
with a reviewed encoding measurement when a stored record shape changes.

## Entry Assumptions

| Entry type | Durability | Encoded bytes | TTL assumption |
|---|---:|---:|---:|
| `contract_instance` | instance | 2,048 | 500,000 ledgers |
| `issuer_record` | persistent | 512 | 500,000 ledgers |
| `issuer_address_index` | persistent | 160 | 500,000 ledgers |
| `proof_record` | persistent | 640 | 500,000 ledgers |
| `schema_policy` | persistent | 384 | 500,000 ledgers |

## Scenario Totals

| Scenario | Entries | Encoded bytes | Byte-ledgers | Delta vs baseline |
|---|---:|---:|---:|---:|
| `baseline` | 10,228 | 6,482,944 | 3,241,472,000,000 | +0 |
| `next_release` | 50,553 | 32,193,344 | 16,096,672,000,000 | +12,855,200,000,000 |
| `stress_projection` | 252,103 | 160,716,544 | 80,358,272,000,000 | +77,116,800,000,000 |

## Scenario Detail

### baseline

Current small production cohort.

| Entry type | Count | Encoded bytes | Total bytes |
|---|---:|---:|---:|
| `contract_instance` | 3 | 2,048 | 6,144 |
| `issuer_record` | 100 | 512 | 51,200 |
| `issuer_address_index` | 100 | 160 | 16,000 |
| `proof_record` | 10,000 | 640 | 6,400,000 |
| `schema_policy` | 25 | 384 | 9,600 |

Total: 10,228 entries, 6,482,944 encoded bytes, and 3,241,472,000,000 byte-ledgers.

### next_release

Expected state after one release cycle.

| Entry type | Count | Encoded bytes | Total bytes |
|---|---:|---:|---:|
| `contract_instance` | 3 | 2,048 | 6,144 |
| `issuer_record` | 250 | 512 | 128,000 |
| `issuer_address_index` | 250 | 160 | 40,000 |
| `proof_record` | 50,000 | 640 | 32,000,000 |
| `schema_policy` | 50 | 384 | 19,200 |

Total: 50,553 entries, 32,193,344 encoded bytes, and 16,096,672,000,000 byte-ledgers.

### stress_projection

Capacity-planning bound, not an enforced limit.

| Entry type | Count | Encoded bytes | Total bytes |
|---|---:|---:|---:|
| `contract_instance` | 3 | 2,048 | 6,144 |
| `issuer_record` | 1,000 | 512 | 512,000 |
| `issuer_address_index` | 1,000 | 160 | 160,000 |
| `proof_record` | 250,000 | 640 | 160,000,000 |
| `schema_policy` | 100 | 384 | 38,400 |

Total: 252,103 entries, 160,716,544 encoded bytes, and 80,358,272,000,000 byte-ledgers.

## Interpretation

Convert byte-ledgers to a network fee only with the target network's current rent
configuration. These projections intentionally exclude archived cold state, code
entries, transaction envelopes, and temporary execution memory. The stress scenario
is a planning comparison and does not raise or replace on-chain capacity limits.
