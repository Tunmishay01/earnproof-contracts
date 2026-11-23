# Cargo dependency policy

EarnProof dependency review is enforced by `cargo-deny` 0.21.0 and the
repository-local [`deny.toml`](../deny.toml). The gate rejects known security
advisories, yanked crates, unapproved licenses, unknown registries, and unknown
Git sources. Git dependencies, when explicitly approved, must use an immutable
revision rather than a branch or tag.

The RustSec database revision is pinned in
[`advisory-db-revision`](../.cargo/advisory-db-revision). CI fetches the
database, checks out that exact commit, verifies `HEAD`, and runs the advisory
check with fetching disabled. Updating vulnerability data is therefore an
explicit reviewed change rather than a time-dependent CI input.

Advisory exceptions must be structured `deny.toml` entries with a reason and
absolute expiry date. License exceptions must appear both in `deny.toml` and
[`policy-exceptions.json`](../.cargo/policy-exceptions.json), where each entry
has an owner-readable reason and expiry. Expired, malformed, unused, or
one-sided exceptions fail closed.

Run the deterministic checks with:

```bash
python3 scripts/check-cargo-policy.py
python3 scripts/check-cargo-policy.py --self-test
cargo deny fetch
cargo deny check --disable-fetch advisories bans licenses sources
```

The self-test seeds a moving advisory pin, an expired exception, and an unknown
Git-source policy; all three must be rejected. No credentials or private source
URLs belong in these files.
