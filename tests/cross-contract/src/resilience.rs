//! Runtime boundary tests for recursive and reentrant cross-contract calls.

use soroban_sdk::testutils::{Events as _, Ledger as _};
use soroban_sdk::{
    contract, contracterror, contractimpl, contracttype, symbol_short, Address, Env, Vec,
};

#[contracttype]
enum ProbeKey {
    Calls,
    Entered,
}

#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
#[repr(u32)]
enum ProbeError {
    ReentrantCall = 1,
}

#[contract]
struct DepthProbe;

#[contractimpl]
impl DepthProbe {
    pub fn descend(env: Env, chain: Vec<Address>, index: u32) {
        let calls: u32 = env.storage().instance().get(&ProbeKey::Calls).unwrap_or(0);
        env.storage().instance().set(&ProbeKey::Calls, &(calls + 1));
        env.events().publish((symbol_short!("depth"),), index);

        let next_index = index + 1;
        if next_index < chain.len() {
            let next = chain.get(next_index).expect("the index was bounds checked");
            DepthProbeClient::new(&env, &next).descend(&chain, &next_index);
        }
    }

    pub fn calls(env: Env) -> u32 {
        env.storage().instance().get(&ProbeKey::Calls).unwrap_or(0)
    }
}

#[contract]
struct GuardedTarget;

#[contractimpl]
impl GuardedTarget {
    pub fn enter(env: Env, callback: Address) -> Result<(), ProbeError> {
        if env
            .storage()
            .instance()
            .get(&ProbeKey::Entered)
            .unwrap_or(false)
        {
            return Err(ProbeError::ReentrantCall);
        }

        env.storage().instance().set(&ProbeKey::Entered, &true);
        let calls: u32 = env.storage().instance().get(&ProbeKey::Calls).unwrap_or(0);
        env.storage().instance().set(&ProbeKey::Calls, &(calls + 1));
        env.events().publish((symbol_short!("entered"),), calls + 1);

        CallbackProbeClient::new(&env, &callback).call_back(&env.current_contract_address());
        env.storage().instance().remove(&ProbeKey::Entered);
        Ok(())
    }

    pub fn calls(env: Env) -> u32 {
        env.storage().instance().get(&ProbeKey::Calls).unwrap_or(0)
    }
}

#[contract]
struct CallbackProbe;

#[contractimpl]
impl CallbackProbe {
    pub fn call_back(env: Env, target: Address) {
        GuardedTargetClient::new(&env, &target).enter(&env.current_contract_address());
    }
}

fn rejects(call: impl FnOnce()) -> bool {
    let previous = std::panic::take_hook();
    std::panic::set_hook(std::boxed::Box::new(|_| {}));
    let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(call));
    std::panic::set_hook(previous);
    outcome.is_err()
}

#[test]
fn recursive_unique_contract_chain_hits_the_host_limit_atomically() {
    let env = Env::default();
    env.ledger().set_sequence_number(1_000);

    let mut chain = Vec::new(&env);
    for _ in 0..128 {
        chain.push_back(env.register(DepthProbe, ()));
    }

    let root = chain.get(0).unwrap();
    assert!(rejects(
        || DepthProbeClient::new(&env, &root).descend(&chain, &0)
    ));
    assert_eq!(
        env.events().all().events().len(),
        0,
        "aborted recursion leaked events"
    );

    for address in chain.iter() {
        assert_eq!(
            DepthProbeClient::new(&env, &address).calls(),
            0,
            "aborted recursion left a partial state write"
        );
    }
}

#[test]
fn reentrant_callback_is_rejected_and_rolls_back_caller_and_callee() {
    let env = Env::default();
    let target = env.register(GuardedTarget, ());
    let callback = env.register(CallbackProbe, ());

    assert!(rejects(
        || GuardedTargetClient::new(&env, &target).enter(&callback)
    ));
    assert_eq!(
        env.events().all().events().len(),
        0,
        "rejected callback leaked events"
    );
    assert_eq!(GuardedTargetClient::new(&env, &target).calls(), 0);
}
