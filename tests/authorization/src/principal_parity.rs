//! Account and custom-account contract authorization parity.

use protocol_config::{ProtocolConfigContract, ProtocolConfigContractClient};
use soroban_sdk::auth::{Context, CustomAccountInterface};
use soroban_sdk::crypto::Hash;
use soroban_sdk::testutils::storage::Instance as _;
use soroban_sdk::testutils::{Address as _, Events as _, MockAuth, MockAuthInvoke};
use soroban_sdk::xdr::{
    InvokeContractArgs, ScVal, SorobanAddressCredentials, SorobanAuthorizationEntry,
    SorobanAuthorizedFunction, SorobanAuthorizedInvocation, SorobanCredentials, VecM,
};
use soroban_sdk::{contract, contracterror, contractimpl, Address, Env, Map, Val, Vec};

#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
#[repr(u32)]
enum AccountError {
    #[allow(dead_code)]
    Rejected = 1,
}

#[contract]
struct ContractPrincipal;

#[contractimpl]
impl CustomAccountInterface for ContractPrincipal {
    type Signature = ();
    type Error = AccountError;

    fn __check_auth(
        _env: Env,
        _signature_payload: Hash<32>,
        _signatures: (),
        _auth_contexts: Vec<Context>,
    ) -> Result<(), AccountError> {
        Ok(())
    }
}

struct Fixture<'a> {
    env: Env,
    config: ProtocolConfigContractClient<'a>,
    config_address: Address,
    admin: Address,
}

fn fixture<'a>(env: &'a Env, admin: Address) -> Fixture<'a> {
    env.mock_all_auths();
    let config_address = env.register(ProtocolConfigContract, ());
    let config = ProtocolConfigContractClient::new(&env, &config_address);
    config.initialize(&admin);
    Fixture {
        env: env.clone(),
        config,
        config_address,
        admin,
    }
}

fn authorize_account(fixture: &Fixture) {
    fixture.env.mock_auths(&[MockAuth {
        address: &fixture.admin,
        invoke: &MockAuthInvoke {
            contract: &fixture.config_address,
            fn_name: "pause",
            args: soroban_sdk::Vec::new(&fixture.env),
            sub_invokes: &[],
        },
    }]);
}

fn authorize_contract(fixture: &Fixture, function: &str) {
    let address = fixture.admin.clone().try_into().unwrap();
    fixture.env.set_auths(&[SorobanAuthorizationEntry {
        credentials: SorobanCredentials::Address(SorobanAddressCredentials {
            address,
            nonce: 1,
            signature_expiration_ledger: 100_000,
            signature: ScVal::Void,
        }),
        root_invocation: SorobanAuthorizedInvocation {
            function: SorobanAuthorizedFunction::ContractFn(InvokeContractArgs {
                contract_address: fixture.config_address.clone().try_into().unwrap(),
                function_name: function.try_into().unwrap(),
                args: VecM::default(),
            }),
            sub_invocations: VecM::default(),
        },
    }]);
}

fn instance(fixture: &Fixture) -> Map<Val, Val> {
    fixture.env.as_contract(&fixture.config_address, || {
        fixture.env.storage().instance().all()
    })
}

#[test]
fn account_and_contract_principals_have_equivalent_success_invariants() {
    let account_env = Env::default();
    let account = Address::generate(&account_env);
    let account_fixture = fixture(&account_env, account);
    authorize_account(&account_fixture);
    account_fixture.config.pause();
    let account_event_count = account_fixture.env.events().all().events().len();

    let contract_env = Env::default();
    let contract = contract_env.register(ContractPrincipal, ());
    let contract_fixture = fixture(&contract_env, contract);
    authorize_contract(&contract_fixture, "pause");
    contract_fixture.config.pause();
    let contract_event_count = contract_fixture.env.events().all().events().len();

    assert!(account_fixture.config.is_paused());
    assert!(contract_fixture.config.is_paused());
    assert_eq!(account_event_count, contract_event_count);
    assert_eq!(account_event_count, 1);
}

#[test]
fn both_principal_types_reject_missing_or_mismatched_authorization_atomically() {
    let account_env = Env::default();
    let account_fixture = fixture(&account_env, Address::generate(&account_env));
    account_fixture.env.mock_auths(&[]);
    let account_before = instance(&account_fixture);
    assert!(account_fixture.config.try_pause().is_err());
    assert_eq!(instance(&account_fixture), account_before);
    assert_eq!(account_fixture.env.events().all().events().len(), 0);

    let contract_env = Env::default();
    let contract = contract_env.register(ContractPrincipal, ());
    let contract_fixture = fixture(&contract_env, contract);
    authorize_contract(&contract_fixture, "unpause");
    let contract_before = instance(&contract_fixture);
    assert!(contract_fixture.config.try_pause().is_err());
    assert_eq!(instance(&contract_fixture), contract_before);
    assert_eq!(contract_fixture.env.events().all().events().len(), 0);
}

#[test]
fn contract_credentials_do_not_authorize_a_different_account_principal() {
    let env = Env::default();
    let account = Address::generate(&env);
    let contract = env.register(ContractPrincipal, ());
    let fixture = fixture(&env, account);

    let contract_address = contract.try_into().unwrap();
    fixture.env.set_auths(&[SorobanAuthorizationEntry {
        credentials: SorobanCredentials::Address(SorobanAddressCredentials {
            address: contract_address,
            nonce: 1,
            signature_expiration_ledger: 100_000,
            signature: ScVal::Void,
        }),
        root_invocation: SorobanAuthorizedInvocation {
            function: SorobanAuthorizedFunction::ContractFn(InvokeContractArgs {
                contract_address: fixture.config_address.clone().try_into().unwrap(),
                function_name: "pause".try_into().unwrap(),
                args: VecM::default(),
            }),
            sub_invocations: VecM::default(),
        },
    }]);

    let before = instance(&fixture);
    assert!(fixture.config.try_pause().is_err());
    assert_eq!(instance(&fixture), before);
    assert_eq!(fixture.env.events().all().events().len(), 0);
}
