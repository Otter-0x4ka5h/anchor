//! Regression tests for `init_if_needed` exact-space validation.

use {
    anchor_lang_v2::{
        accounts::{Account, Program, SystemAccount},
        programs::System,
        testing::AccountBuffer,
        Accounts, AnchorAccount, Discriminator, ErrorCode, Id, InitSpace, Owner, Space,
        TryAccounts,
    },
    bytemuck::{Pod, Zeroable},
    pinocchio::address::Address,
    solana_program_error::ProgramError,
};

anchor_lang_v2::declare_id!("11111111111111111111111111111111");

const PROGRAM_ID: [u8; 32] = [0x42; 32];
const SYSTEM_PROGRAM_ID: [u8; 32] = [0u8; 32];

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable, InitSpace)]
struct Vault {
    value: u64,
}

impl Owner for Vault {
    const OWNER: Address = Address::new_from_array(PROGRAM_ID);
}

impl Discriminator for Vault {
    // sha256("account:Vault")[..8]
    const DISCRIMINATOR: &'static [u8] = &[0xd3, 0x08, 0xe8, 0x2b, 0x02, 0x98, 0x75, 0x77];
}

const VAULT_SPACE: usize = 8 + Vault::INIT_SPACE;

#[allow(dead_code)]
#[derive(Accounts)]
struct InitIfNeededVault {
    #[account(init_if_needed, payer = payer, space = VAULT_SPACE, seeds = [b"vault"], bump)]
    vault: Account<Vault>,
    #[account(mut)]
    payer: SystemAccount,
    system_program: Program<System>,
}

fn expect_err<T>(r: Result<T, ProgramError>) -> ProgramError {
    match r {
        Ok(_) => panic!("expected Err, got Ok"),
        Err(e) => e,
    }
}

fn setup_existing_vault(buf: &AccountBuffer<256>, address: [u8; 32], data_len: usize, value: u64) {
    buf.init(address, PROGRAM_ID, data_len, false, true, false);
    let mut data = vec![0u8; data_len];
    data[..8].copy_from_slice(Vault::DISCRIMINATOR);
    data[8..16].copy_from_slice(&value.to_le_bytes());
    buf.write_data(&data);
}

fn setup_payer(buf: &AccountBuffer<128>, address: [u8; 32]) {
    buf.init(address, SYSTEM_PROGRAM_ID, 0, false, true, false);
}

fn setup_system_program(buf: &AccountBuffer<128>) {
    buf.init(SYSTEM_PROGRAM_ID, SYSTEM_PROGRAM_ID, 0, false, false, true);
}

#[test]
fn existing_init_if_needed_account_with_exact_space_is_accepted() {
    let program_id = Address::new_from_array(PROGRAM_ID);
    let (vault_address, _) = anchor_lang_v2::find_program_address(&[b"vault"], &program_id);

    let vault_buf = AccountBuffer::<256>::new();
    setup_existing_vault(&vault_buf, vault_address.to_bytes(), VAULT_SPACE, 7);

    let payer_buf = AccountBuffer::<128>::new();
    setup_payer(&payer_buf, [0x33; 32]);

    let system_program_buf = AccountBuffer::<128>::new();
    setup_system_program(&system_program_buf);

    let views = [
        unsafe { vault_buf.view() },
        unsafe { payer_buf.view() },
        unsafe { system_program_buf.view() },
    ];

    let (accounts, _, _) =
        InitIfNeededVault::try_accounts(&program_id, &views, None, 0, &[]).unwrap();
    assert_eq!(accounts.vault.value, 7);
}

#[test]
fn existing_init_if_needed_account_with_mismatched_space_is_rejected() {
    let program_id = Address::new_from_array(PROGRAM_ID);
    let (vault_address, _) = anchor_lang_v2::find_program_address(&[b"vault"], &program_id);

    let vault_buf = AccountBuffer::<256>::new();
    setup_existing_vault(&vault_buf, vault_address.to_bytes(), VAULT_SPACE + 8, 11);

    let payer_buf = AccountBuffer::<128>::new();
    setup_payer(&payer_buf, [0x44; 32]);

    let system_program_buf = AccountBuffer::<128>::new();
    setup_system_program(&system_program_buf);

    let views = [
        unsafe { vault_buf.view() },
        unsafe { payer_buf.view() },
        unsafe { system_program_buf.view() },
    ];

    let err = expect_err(InitIfNeededVault::try_accounts(
        &program_id,
        &views,
        None,
        0,
        &[],
    ));
    assert_eq!(err, ErrorCode::ConstraintSpace.into());
}
