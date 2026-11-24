//! Persistent entry-count and encoded-byte regression budgets.

use earnproof_shared::{ProofRegistrationInput, MAX_PROOF_BATCH_SIZE};
use issuer_registry::{IssuerRegistryContract, IssuerRegistryContractClient};
use proof_registry::{ProofRegistryContract, ProofRegistryContractClient};
use protocol_config::{ProtocolConfigContract, ProtocolConfigContractClient};
use soroban_sdk::testutils::storage::Persistent as _;
use soroban_sdk::testutils::{Address as _, Events as _, Ledger as _};
use soroban_sdk::xdr::ToXdr;
use soroban_sdk::{Address, BytesN, Env, Map, Val};

const SINGLE_PROOF_ENTRY_MAX: u32 = 16;
const SINGLE_PROOF_BYTES_MAX: u32 = 32_768;
const MAX_BATCH_ENTRY_MAX: u32 = 192;
const MAX_BATCH_BYTES_MAX: u32 = 524_288;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct Footprint {
    entries: u32,
    encoded_bytes: u32,
}

impl Footprint {
    fn delta(self, before: Self) -> Self {
        Self {
            entries: self
                .entries
                .checked_sub(before.entries)
                .expect("entry count regressed"),
            encoded_bytes: self
                .encoded_bytes
                .checked_sub(before.encoded_bytes)
                .expect("encoded footprint regressed"),
        }
    }
}

fn footprint(env: &Env, contract: &Address) -> Footprint {
    let all: Map<Val, Val> = env.as_contract(contract, || env.storage().persistent().all());
    let mut entries = 0_u32;
    let mut encoded_bytes = 0_u32;
    for key in all.keys().iter() {
        if !env.as_contract(contract, || env.storage().persistent().has(&key)) {
            continue;
        }
        let value: Val = env.as_contract(contract, || {
            env.storage()
                .persistent()
                .get(&key)
                .expect("key was present during the same snapshot")
        });
        entries += 1;
        encoded_bytes += key.clone().to_xdr(env).len() + value.to_xdr(env).len();
    }
    Footprint {
        entries,
        encoded_bytes,
    }
}

fn assert_delta(
    operation: &str,
    key_family: &str,
    delta: Footprint,
    entry_max: u32,
    byte_max: u32,
) {
    std::println!(
        "{operation}: family={key_family}, entries={}, encoded_bytes={}",
        delta.entries,
        delta.encoded_bytes
    );
    assert!(
        delta.entries <= entry_max,
        "{operation} grew {key_family} by {} entries, budget {entry_max}",
        delta.entries
    );
    assert!(
        delta.encoded_bytes <= byte_max,
        "{operation} grew {key_family} by {} encoded bytes, budget {byte_max}",
        delta.encoded_bytes
    );
}

fn bytes(env: &Env, value: u8) -> BytesN<32> {
    BytesN::from_array(env, &[value; 32])
}

struct Deployment<'a> {
    env: Env,
    proofs: ProofRegistryContractClient<'a>,
    proofs_address: Address,
    issuer: Address,
}

fn deployment() -> Deployment<'static> {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().set_timestamp(1_000);
    let admin = Address::generate(&env);
    let issuer = Address::generate(&env);

    let config_address = env.register(ProtocolConfigContract, ());
    let config = ProtocolConfigContractClient::new(&env, &config_address);
    config.initialize(&admin);
    config.approve_schema_version(&1);
    config.approve_proof_type(&bytes(&env, 1));

    let issuers_address = env.register(IssuerRegistryContract, ());
    let issuers = IssuerRegistryContractClient::new(&env, &issuers_address);
    issuers.initialize(&admin);
    issuers.register_issuer(&bytes(&env, 2), &issuer, &bytes(&env, 3), &bytes(&env, 4));

    let proofs_address = env.register(ProofRegistryContract, ());
    let proofs = ProofRegistryContractClient::new(&env, &proofs_address);
    proofs.initialize(&admin, &issuers_address, &config_address);
    Deployment {
        env,
        proofs,
        proofs_address,
        issuer,
    }
}

#[test]
fn minimum_and_maximum_registration_footprints_stay_bounded() {
    let minimum = deployment();
    let before = footprint(&minimum.env, &minimum.proofs_address);
    minimum.proofs.register_proof_typed(
        &bytes(&minimum.env, 10),
        &bytes(&minimum.env, 11),
        &minimum.issuer,
        &1,
        &10_000,
        &bytes(&minimum.env, 1),
    );
    let single = footprint(&minimum.env, &minimum.proofs_address).delta(before);
    assert_delta(
        "register_proof_typed(single)",
        "Proof/ProofConfiguration/ProofPolicy/accounting",
        single,
        SINGLE_PROOF_ENTRY_MAX,
        SINGLE_PROOF_BYTES_MAX,
    );

    let maximum = deployment();
    let mut batch = soroban_sdk::Vec::new(&maximum.env);
    for index in 0..MAX_PROOF_BATCH_SIZE {
        batch.push_back(ProofRegistrationInput {
            proof_id_hash: bytes(&maximum.env, 30 + index as u8),
            commitment_hash: bytes(&maximum.env, 100 + index as u8),
            schema_version: 1,
            expires_at: 10_000,
            proof_type: bytes(&maximum.env, 1),
        });
    }
    let before = footprint(&maximum.env, &maximum.proofs_address);
    maximum
        .proofs
        .register_proofs_batch(&batch, &maximum.issuer);
    let max = footprint(&maximum.env, &maximum.proofs_address).delta(before);
    assert_delta(
        "register_proofs_batch(max)",
        "Proof/ProofConfiguration/ProofPolicy/accounting/rate-usage",
        max,
        MAX_BATCH_ENTRY_MAX,
        MAX_BATCH_BYTES_MAX,
    );
    assert!(max.entries >= single.entries);
    assert!(max.encoded_bytes >= single.encoded_bytes);
}

#[test]
fn duplicate_and_unauthorized_failures_add_no_persistent_entries_or_bytes() {
    let deployment = deployment();
    let proof_id = bytes(&deployment.env, 20);
    let commitment = bytes(&deployment.env, 21);
    deployment.proofs.register_proof_typed(
        &proof_id,
        &commitment,
        &deployment.issuer,
        &1,
        &10_000,
        &bytes(&deployment.env, 1),
    );

    let before_duplicate = footprint(&deployment.env, &deployment.proofs_address);
    assert!(deployment
        .proofs
        .try_register_proof_typed(
            &proof_id,
            &commitment,
            &deployment.issuer,
            &1,
            &10_000,
            &bytes(&deployment.env, 1),
        )
        .is_err());
    assert_eq!(
        footprint(&deployment.env, &deployment.proofs_address),
        before_duplicate
    );
    assert_eq!(deployment.env.events().all().events().len(), 0);

    deployment.env.mock_auths(&[]);
    let before_unauthorized = footprint(&deployment.env, &deployment.proofs_address);
    assert!(deployment
        .proofs
        .try_register_proof_typed(
            &bytes(&deployment.env, 22),
            &bytes(&deployment.env, 23),
            &deployment.issuer,
            &1,
            &10_000,
            &bytes(&deployment.env, 1),
        )
        .is_err());
    assert_eq!(
        footprint(&deployment.env, &deployment.proofs_address),
        before_unauthorized
    );
    assert_eq!(deployment.env.events().all().events().len(), 0);
}
