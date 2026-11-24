//! Renewal-amplification resistance for low-cost and failing calls.

use super::fixture::{bytes, deployment, proof_key, FAR_FUTURE};
use earnproof_shared::{TTL_EXTEND_TO_LEDGERS, TTL_THRESHOLD_LEDGERS};
use soroban_sdk::testutils::storage::Persistent as _;
use soroban_sdk::testutils::Events as _;

fn ttl(deployment: &super::fixture::Deployment, proof_id: &soroban_sdk::BytesN<32>) -> u32 {
    deployment.env.as_contract(&deployment.proofs_id, || {
        deployment
            .env
            .storage()
            .persistent()
            .get_ttl(&proof_key(&deployment.env, proof_id))
    })
}

fn age_to_threshold(deployment: &super::fixture::Deployment) {
    deployment.idle(TTL_EXTEND_TO_LEDGERS - TTL_THRESHOLD_LEDGERS);
}

#[test]
fn targeted_read_does_not_renew_an_unrelated_proof() {
    let deployment = deployment();
    let first = deployment.register_proof(FAR_FUTURE);
    let second = bytes(&deployment.env, 0x71);
    deployment.proofs.register_proof_typed(
        &second,
        &bytes(&deployment.env, 0x72),
        &deployment.issuer,
        &1,
        &FAR_FUTURE,
        &bytes(&deployment.env, 1),
    );
    age_to_threshold(&deployment);

    assert_eq!(ttl(&deployment, &first), TTL_THRESHOLD_LEDGERS);
    assert_eq!(ttl(&deployment, &second), TTL_THRESHOLD_LEDGERS);

    deployment.proofs.get_proof(&first);

    assert_eq!(ttl(&deployment, &first), TTL_EXTEND_TO_LEDGERS);
    assert_eq!(ttl(&deployment, &second), TTL_THRESHOLD_LEDGERS);
}

#[test]
fn repeated_reads_in_one_ledger_do_not_amplify_renewal() {
    let deployment = deployment();
    let proof_id = deployment.register_proof(FAR_FUTURE);
    age_to_threshold(&deployment);

    deployment.proofs.get_proof(&proof_id);
    let renewed_ttl = ttl(&deployment, &proof_id);
    for _ in 0..32 {
        deployment.proofs.get_proof(&proof_id);
        assert_eq!(ttl(&deployment, &proof_id), renewed_ttl);
    }

    assert_eq!(renewed_ttl, TTL_EXTEND_TO_LEDGERS);
}

#[test]
fn failed_unknown_read_does_not_renew_existing_state() {
    let deployment = deployment();
    let proof_id = deployment.register_proof(FAR_FUTURE);
    age_to_threshold(&deployment);
    let events_before = deployment.env.events().all().len();

    assert!(deployment
        .proofs
        .try_get_proof(&bytes(&deployment.env, 0xfe))
        .is_err());

    assert_eq!(ttl(&deployment, &proof_id), TTL_THRESHOLD_LEDGERS);
    assert_eq!(deployment.env.events().all().len(), events_before);
}

#[test]
fn unauthorized_mutation_changes_neither_ttl_nor_events() {
    let deployment = deployment();
    let proof_id = deployment.register_proof(FAR_FUTURE);
    age_to_threshold(&deployment);
    let events_before = deployment.env.events().all().len();
    deployment.env.mock_auths(&[]);

    assert!(deployment.proofs.try_revoke_proof(&proof_id).is_err());

    assert_eq!(ttl(&deployment, &proof_id), TTL_THRESHOLD_LEDGERS);
    assert_eq!(deployment.env.events().all().len(), events_before);
}
