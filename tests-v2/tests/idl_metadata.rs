use {
    anchor_lang_idl_spec::{IdlInstructionAccount, IdlInstructionAccountItem, IdlSeed},
    anchor_lang_v2::{programs::AssociatedToken, Id},
    declare_program_surface::surface,
};

fn parse_accounts(json: &str) -> Vec<IdlInstructionAccountItem> {
    serde_json::from_str(json)
        .unwrap_or_else(|err| panic!("failed to parse accounts JSON: {err}\njson: {json}"))
}

fn single_account(items: &[IdlInstructionAccountItem], index: usize) -> &IdlInstructionAccount {
    match &items[index] {
        IdlInstructionAccountItem::Single(account) => account,
        IdlInstructionAccountItem::Composite(_) => {
            panic!("expected single account at index {index}")
        }
    }
}

#[test]
fn marker_id_program_seed_emits_marker_address_bytes() {
    let items = parse_accounts(&accounts_test::CheckAssociatedTokenProgramSeed::__idl_accounts());
    let account = single_account(&items, 0);
    let pda = account.pda.as_ref().expect("account should include pda");
    let program = pda.program.as_ref().expect("pda should include program");

    match program {
        IdlSeed::Const(seed) => {
            assert_eq!(seed.value, AssociatedToken::id().to_bytes());
        }
        other => panic!("expected const program seed, got {other:?}"),
    }
}

#[test]
fn nested_accounts_register_transitive_idl_deps() {
    let mut accounts = Vec::new();
    let mut types = Vec::new();
    accounts_test::NestedIdlDepsOuter::__idl_register_deps(&mut accounts, &mut types);

    assert!(
        accounts
            .iter()
            .any(|entry| entry.contains("\"name\":\"NestedVault\"")),
        "nested account data should register its account entry: {accounts:?}"
    );
    assert!(
        types
            .iter()
            .any(|entry| entry.contains("\"name\":\"NestedVault\"")),
        "nested account data should register its type entry: {types:?}"
    );
}

#[test]
fn nested_accounts_without_data_do_not_invent_idl_deps() {
    let mut accounts = Vec::new();
    let mut types = Vec::new();
    accounts_test::NestedNoDepsOuter::__idl_register_deps(&mut accounts, &mut types);

    assert!(
        accounts.is_empty(),
        "expected no account deps, got {accounts:?}"
    );
    assert!(types.is_empty(), "expected no type deps, got {types:?}");
}

#[test]
fn declared_program_markers_expose_known_idl_address() {
    assert_eq!(
        surface::program::Surface::IDL_ADDRESS,
        "D9t6cEFPTDWmTZfcikokLbnuuyeJT6oXnpEbyXB45LU2"
    );
}
