use {
    anchor_lang_idl_spec::{IdlInstructionAccount, IdlInstructionAccountItem, IdlSeed},
    anchor_lang_v2::{programs::AssociatedToken, Id},
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
fn const_seed_expression_emits_const_seed_bytes() {
    let items = parse_accounts(&seeds::CheckConstSeeds::__idl_accounts());
    let account = single_account(&items, 1);
    let pda = account.pda.as_ref().expect("const seed account should include pda");

    assert_eq!(pda.seeds.len(), 1);
    match &pda.seeds[0] {
        IdlSeed::Const(seed) => assert_eq!(seed.value, b"data"),
        other => panic!("expected const seed metadata, got {other:?}"),
    }
}

#[test]
fn fn_seed_expression_emits_const_seed_bytes() {
    let items = parse_accounts(&seeds::CheckFnSeeds::__idl_accounts());
    let account = single_account(&items, 1);
    let pda = account.pda.as_ref().expect("function seed account should include pda");

    assert_eq!(pda.seeds.len(), 1);
    match &pda.seeds[0] {
        IdlSeed::Const(seed) => assert_eq!(seed.value, b"data"),
        other => panic!("expected const seed metadata, got {other:?}"),
    }
}

#[test]
fn unsupported_runtime_seed_omits_pda_metadata() {
    let items = parse_accounts(&seeds::InitDirectAccountFieldSeed::__idl_accounts());
    let account = single_account(&items, 2);
    assert!(
        account.pda.is_none(),
        "runtime-only account-data seed should omit unsupported pda metadata"
    );
}
