# Batch Validation Semantics

Proof registration and issuer/admin revocation batches are bounded by
`MAX_PROOF_BATCH_SIZE`. Mutating endpoints reject an empty or oversized batch
and reject repeated proof identifiers before authorization, storage writes,
TTL renewal, counters, or events. Duplicate detection compares fixed-size
identifiers and is applied to every input ordering.

Clients that need a stable location and reason can call
`preflight_register_proofs_batch` or `preflight_revoke_proofs_batch`. Version 1
returns either `Valid` or an `Invalid` record containing:

- `index`: the failing element;
- `related_index`: the first occurrence for a duplicate, otherwise `index`;
- `reason`: a bounded enum such as `DuplicateIdentifier`, `Expired`, or
  `ProofNotFound`.

The response never echoes proof identifiers, commitments, issuer addresses, or
other private input. Preflight performs no authorization and makes no storage
change. It is advisory: the mutating call revalidates current state and remains
atomic if state changes between preflight and execution.

Read-only issuer and schema status batches retain positional semantics:
duplicates produce duplicate result positions. Their TTL renewal is idempotent
and they change no counters, authorization, or events.
