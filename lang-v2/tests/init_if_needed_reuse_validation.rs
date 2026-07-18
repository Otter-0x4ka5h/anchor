use {
    anchor_lang_v2::{
        accounts::{Signer, UncheckedAccount},
        cpi::rent_exempt_lamports,
        testing::AccountBuffer,
        Accounts, AnchorAccount, ErrorCode, Id, TryAccounts,
    },
    pinocchio::address::Address,
    solana_program_error::ProgramError,
};

const PROGRAM_ID: [u8; 32] = [0x42; 32];
const FOREIGN_OWNER: [u8; 32] = [0x24; 32];

#[derive(Accounts)]
struct SeedlessReuseUnchecked {
    #[account(init_if_needed, payer = payer, space = 8)]
    target: UncheckedAccount,
    #[account(mut)]
    payer: Signer,
}

fn expect_err<T>(result: Result<T, ProgramError>) -> ProgramError {
    match result {
        Ok(_) => panic!("expected Err, got Ok"),
        Err(err) => err,
    }
}

fn target_account(
    owner: [u8; 32],
    data_len: usize,
    signer: bool,
    lamports: u64,
) -> AccountBuffer<128> {
    let buf = AccountBuffer::<128>::new();
    buf.init([0xAA; 32], owner, data_len, signer, true, false);
    buf.set_lamports(lamports);
    buf
}

fn payer_account() -> AccountBuffer<128> {
    let buf = AccountBuffer::<128>::new();
    buf.init([0xBB; 32], PROGRAM_ID, 0, true, true, false);
    buf
}

fn try_reuse(target: &AccountBuffer<128>, payer: &AccountBuffer<128>) -> Result<(), ProgramError> {
    let views = [unsafe { target.view() }, unsafe { payer.view() }];
    SeedlessReuseUnchecked::try_accounts(&Address::new_from_array(PROGRAM_ID), &views, None, 0, &[])
        .map(|_| ())
}

#[test]
fn seedless_reuse_requires_target_signature() {
    let target = target_account(PROGRAM_ID, 8, false, 1_000_000);
    let payer = payer_account();

    let err = expect_err(try_reuse(&target, &payer));
    assert_eq!(err, ErrorCode::ConstraintSigner.into());
}

#[test]
fn seedless_reuse_revalidates_exact_space() {
    let target = target_account(PROGRAM_ID, 16, true, 1_000_000);
    let payer = payer_account();

    let err = expect_err(try_reuse(&target, &payer));
    assert_eq!(err, ErrorCode::ConstraintSpace.into());
}

#[test]
fn seedless_reuse_revalidates_owner_for_unchecked_accounts() {
    let target = target_account(FOREIGN_OWNER, 8, true, 1_000_000);
    let payer = payer_account();

    let err = expect_err(try_reuse(&target, &payer));
    assert_eq!(err, ErrorCode::ConstraintOwner.into());
}

#[test]
fn seedless_reuse_revalidates_rent_exemption() {
    let target = target_account(PROGRAM_ID, 8, true, 1);
    let payer = payer_account();

    let err = expect_err(try_reuse(&target, &payer));
    assert_eq!(err, ErrorCode::ConstraintRentExempt.into());
}

#[test]
fn seedless_reuse_accepts_fully_initialized_accounts() {
    let required = rent_exempt_lamports(8).unwrap();
    let target = target_account(PROGRAM_ID, 8, true, required);
    let payer = payer_account();

    try_reuse(&target, &payer).expect("fully initialized reuse path should succeed");
}
