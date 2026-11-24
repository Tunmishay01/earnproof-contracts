//! Deterministic authorization-tree depth and rollback boundaries.

use soroban_sdk::testutils::{
    Address as _, AuthorizedInvocation, Events as _, MockAuth, MockAuthInvoke,
};
use soroban_sdk::{
    contract, contracterror, contractimpl, contracttype, symbol_short, Address, Env, IntoVal, Vec,
};

const REVIEWED_AUTH_DEPTH: u32 = 8;
const AUTH_DEPTH_CPU_MAX: u64 = 8_000_000;
const AUTH_DEPTH_MEMORY_MAX: u64 = 2_000_000;

#[contracttype]
enum DepthKey {
    Calls,
}

#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
#[repr(u32)]
enum DepthError {
    InvalidDepth = 1,
}

#[contract]
struct AuthDepthProbe;

#[contractimpl]
impl AuthDepthProbe {
    pub fn walk(
        env: Env,
        signer: Address,
        chain: Vec<Address>,
        index: u32,
        requested_depth: u32,
    ) -> Result<(), DepthError> {
        signer.require_auth();
        if requested_depth == 0
            || requested_depth > REVIEWED_AUTH_DEPTH
            || requested_depth > chain.len()
        {
            return Err(DepthError::InvalidDepth);
        }

        let calls: u32 = env.storage().instance().get(&DepthKey::Calls).unwrap_or(0);
        env.storage().instance().set(&DepthKey::Calls, &(calls + 1));
        env.events().publish((symbol_short!("authstep"),), index);

        let next_index = index + 1;
        if next_index < requested_depth {
            let next = chain
                .get(next_index)
                .expect("depth was checked against chain length");
            AuthDepthProbeClient::new(&env, &next).walk(
                &signer,
                &chain,
                &next_index,
                &requested_depth,
            );
        }
        Ok(())
    }

    pub fn calls(env: Env) -> u32 {
        env.storage().instance().get(&DepthKey::Calls).unwrap_or(0)
    }
}

fn fixture(depth: u32) -> (Env, Address, Vec<Address>, Address) {
    let env = Env::default();
    let signer = Address::generate(&env);
    let mut chain = Vec::new(&env);
    for _ in 0..depth {
        chain.push_back(env.register(AuthDepthProbe, ()));
    }
    let root = chain.get(0).unwrap();
    (env, signer, chain, root)
}

fn tree_depth(invocation: &AuthorizedInvocation) -> usize {
    1 + invocation
        .sub_invocations
        .iter()
        .map(tree_depth)
        .max()
        .unwrap_or(0)
}

fn assert_rolled_back(env: &Env, chain: &Vec<Address>, label: &str) {
    assert_eq!(env.events().all().events().len(), 0, "{label}: ghost event");
    for address in chain.iter() {
        assert_eq!(
            AuthDepthProbeClient::new(env, &address).calls(),
            0,
            "{label}: partial write at {address:?}"
        );
    }
}

#[test]
fn reviewed_maximum_depth_is_authorized_and_bounded() {
    let (env, signer, chain, root) = fixture(REVIEWED_AUTH_DEPTH);
    env.mock_all_auths();
    env.cost_estimate().budget().reset_unlimited();

    AuthDepthProbeClient::new(&env, &root).walk(&signer, &chain, &0, &REVIEWED_AUTH_DEPTH);

    let auths = env.auths();
    assert_eq!(auths.len(), 1);
    assert_eq!(tree_depth(&auths[0].1), REVIEWED_AUTH_DEPTH as usize);
    assert!(env.cost_estimate().budget().cpu_instruction_cost() <= AUTH_DEPTH_CPU_MAX);
    assert!(env.cost_estimate().budget().memory_bytes_cost() <= AUTH_DEPTH_MEMORY_MAX);
    for address in chain.iter() {
        assert_eq!(AuthDepthProbeClient::new(&env, &address).calls(), 1);
    }
}

#[test]
fn one_above_the_reviewed_depth_fails_before_state_or_events() {
    let requested = REVIEWED_AUTH_DEPTH + 1;
    let (env, signer, chain, root) = fixture(requested);
    env.mock_all_auths();

    assert!(AuthDepthProbeClient::new(&env, &root)
        .try_walk(&signer, &chain, &0, &requested)
        .is_err());
    assert_rolled_back(&env, &chain, "over-limit tree");
}

#[test]
fn authorizing_only_the_root_cannot_bypass_a_nested_auth_node() {
    let (env, signer, chain, root) = fixture(REVIEWED_AUTH_DEPTH);
    let args = (signer.clone(), chain.clone(), 0_u32, REVIEWED_AUTH_DEPTH).into_val(&env);
    env.mock_auths(&[MockAuth {
        address: &signer,
        invoke: &MockAuthInvoke {
            contract: &root,
            fn_name: "walk",
            args,
            sub_invokes: &[],
        },
    }]);

    assert!(AuthDepthProbeClient::new(&env, &root)
        .try_walk(&signer, &chain, &0, &REVIEWED_AUTH_DEPTH)
        .is_err());
    assert_rolled_back(&env, &chain, "missing nested authorization");
}
