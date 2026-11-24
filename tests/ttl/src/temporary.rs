//! Exact temporary-storage expiry semantics for future coordination state.

use soroban_sdk::testutils::storage::Temporary as _;
use soroban_sdk::testutils::{Address as _, Ledger as _};
use soroban_sdk::{contract, contracterror, contractimpl, contracttype, Address, BytesN, Env};

const START_LEDGER: u32 = 1_000;
const LIVE_FOR: u32 = 32;

#[contracttype]
#[derive(Clone)]
enum TemporaryKey {
    Proposal(BytesN<32>),
    Nonce(BytesN<32>),
    Cursor(Address),
    Coordination(BytesN<32>),
}

#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
#[repr(u32)]
enum TemporaryError {
    MissingOrExpired = 1,
}

#[contract]
struct TemporaryStateProbe;

#[contractimpl]
impl TemporaryStateProbe {
    pub fn put(env: Env, key: TemporaryKey, value: u32) {
        env.storage().temporary().set(&key, &value);
        env.storage()
            .temporary()
            .extend_ttl(&key, LIVE_FOR, LIVE_FOR);
    }

    pub fn get(env: Env, key: TemporaryKey) -> Option<u32> {
        env.storage().temporary().get(&key)
    }

    pub fn consume(env: Env, key: TemporaryKey) -> Result<u32, TemporaryError> {
        let value = env
            .storage()
            .temporary()
            .get(&key)
            .ok_or(TemporaryError::MissingOrExpired)?;
        env.storage().temporary().remove(&key);
        Ok(value)
    }
}

fn fixture() -> (Env, Address, TemporaryStateProbeClient<'static>) {
    let env = Env::default();
    env.ledger().set_sequence_number(START_LEDGER);
    let address = env.register(TemporaryStateProbe, ());
    let client = TemporaryStateProbeClient::new(&env, &address);
    (env, address, client)
}

fn ttl(env: &Env, contract: &Address, key: &TemporaryKey) -> u32 {
    env.as_contract(contract, || env.storage().temporary().get_ttl(key))
}

fn id(env: &Env, byte: u8) -> BytesN<32> {
    BytesN::from_array(env, &[byte; 32])
}

#[test]
fn temporary_entries_are_live_before_and_at_the_boundary_then_disappear() {
    let (env, address, client) = fixture();
    let keys = [
        TemporaryKey::Proposal(id(&env, 1)),
        TemporaryKey::Nonce(id(&env, 2)),
        TemporaryKey::Cursor(Address::generate(&env)),
        TemporaryKey::Coordination(id(&env, 3)),
    ];

    for (index, key) in keys.iter().enumerate() {
        client.put(key, &(index as u32 + 1));
    }

    env.ledger()
        .set_sequence_number(START_LEDGER + LIVE_FOR - 1);
    for key in &keys {
        assert_eq!(ttl(&env, &address, key), 1);
        assert!(client.get(key).is_some());
    }

    env.ledger().set_sequence_number(START_LEDGER + LIVE_FOR);
    for key in &keys {
        assert_eq!(ttl(&env, &address, key), 0);
        assert!(client.get(key).is_some());
    }

    env.ledger()
        .set_sequence_number(START_LEDGER + LIVE_FOR + 1);
    for key in &keys {
        assert_eq!(client.get(key), None);
    }
    assert_eq!(
        env.as_contract(&address, || env.storage().temporary().all().len()),
        0
    );
}

#[test]
fn expired_or_consumed_nonce_cannot_be_replayed() {
    let (env, _address, client) = fixture();
    let consumed = TemporaryKey::Nonce(id(&env, 4));
    client.put(&consumed, &7);
    assert_eq!(client.consume(&consumed), 7);
    assert!(client.try_consume(&consumed).is_err());

    let expired = TemporaryKey::Nonce(id(&env, 5));
    client.put(&expired, &8);
    env.ledger()
        .set_sequence_number(START_LEDGER + LIVE_FOR + 1);
    assert!(client.try_consume(&expired).is_err());
    assert_eq!(client.get(&expired), None);
}
