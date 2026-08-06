//! Runtime regression for seedless `init_if_needed` signer enforcement.
//!
//! Seedless `init_if_needed` advertises `signer: true` in IDL/CPI metadata.
//! The exist branch never hits System Program `create_account`, so the
//! derive must also emit a runtime `ConstraintSigner` check — otherwise an
//! unsigned victim account can reach `close = …` exit and be drained.
//!
//! Run:
//! `cargo test -p anchor-lang-v2 --features testing --test init_if_needed_signer_check`

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
const VAULT_LAMPORTS: u64 = 1_000_000;
const RECEIVER_LAMPORTS: u64 = 25;

/// Seedless `init_if_needed` + `close` — the vulnerable surface before the
/// fix. Compile smoke + runtime coverage below.
#[allow(dead_code)]
#[derive(Accounts)]
struct SeedlessInitIfNeededClose {
    #[account(mut)]
    payer: SystemAccount,
    #[account(
        init_if_needed,
        payer = payer,
        space = VAULT_SPACE,
        close = receiver
    )]
    vault: Account<Vault>,
    #[account(mut)]
    receiver: SystemAccount,
    system_program: Program<System>,
}

/// Control: PDA `init_if_needed` must not require a transaction signature.
#[allow(dead_code)]
#[derive(Accounts)]
struct SeededInitIfNeeded {
    #[account(init_if_needed, payer = payer, space = VAULT_SPACE, seeds = [b"vault"], bump)]
    vault: Account<Vault>,
    #[account(mut)]
    payer: SystemAccount,
    system_program: Program<System>,
}

fn setup_existing_vault(buf: &AccountBuffer<256>, address: [u8; 32], is_signer: bool) {
    buf.init(
        address,
        PROGRAM_ID,
        VAULT_SPACE,
        is_signer,
        /*writable*/ true,
        false,
    );
    let mut data = [0u8; VAULT_SPACE];
    data[..8].copy_from_slice(Vault::DISCRIMINATOR);
    data[8..16].copy_from_slice(&7u64.to_le_bytes());
    buf.write_data(&data);
    buf.set_lamports(VAULT_LAMPORTS);
}

fn setup_system_account(buf: &AccountBuffer<128>, address: [u8; 32], lamports: u64) {
    buf.init(
        address,
        SYSTEM_PROGRAM_ID,
        0,
        /*signer*/ true,
        /*writable*/ true,
        false,
    );
    buf.set_lamports(lamports);
}

fn setup_system_program(buf: &AccountBuffer<128>) {
    buf.init(
        SYSTEM_PROGRAM_ID,
        SYSTEM_PROGRAM_ID,
        0,
        false,
        false,
        /*executable*/ true,
    );
}

fn try_seedless(
    payer: &AccountBuffer<128>,
    vault: &AccountBuffer<256>,
    receiver: &AccountBuffer<128>,
    system_program: &AccountBuffer<128>,
) -> Result<SeedlessInitIfNeededClose, ProgramError> {
    let program_id = Address::new_from_array(PROGRAM_ID);
    let views = [
        unsafe { payer.view() },
        unsafe { vault.view() },
        unsafe { receiver.view() },
        unsafe { system_program.view() },
    ];
    SeedlessInitIfNeededClose::try_accounts(&program_id, &views, None, 0, &[])
        .map(|(accounts, _bumps, _)| accounts)
}

#[test]
fn seedless_and_seeded_init_if_needed_derive_smoke() {
    assert_eq!(VAULT_SPACE, 16);
    assert_eq!(SeedlessInitIfNeededClose::HEADER_SIZE, 4);
    assert_eq!(SeededInitIfNeeded::HEADER_SIZE, 3);
}

#[test]
fn seedless_init_if_needed_exist_rejects_unsigned_account_before_close() {
    // Attack shape from finding #79: existing program-owned vault is
    // writable but not a signer; attacker signs only as payer and names an
    // attacker-controlled close recipient.
    let payer = AccountBuffer::<128>::new();
    setup_system_account(&payer, [0x11; 32], 50);

    let vault = AccountBuffer::<256>::new();
    setup_existing_vault(&vault, [0xAA; 32], /*is_signer*/ false);

    let receiver = AccountBuffer::<128>::new();
    setup_system_account(&receiver, [0xCC; 32], RECEIVER_LAMPORTS);

    let system_program = AccountBuffer::<128>::new();
    setup_system_program(&system_program);

    let err = match try_seedless(&payer, &vault, &receiver, &system_program) {
        Ok(_) => panic!("unsigned existing vault must be rejected before close"),
        Err(err) => err,
    };
    assert_eq!(err, ErrorCode::ConstraintSigner.into());

    // No close happened — balances unchanged.
    assert_eq!(unsafe { vault.view() }.lamports(), VAULT_LAMPORTS);
    assert_eq!(unsafe { receiver.view() }.lamports(), RECEIVER_LAMPORTS);
}

#[test]
fn seedless_init_if_needed_exist_allows_signed_account_and_close_transfers() {
    let payer = AccountBuffer::<128>::new();
    setup_system_account(&payer, [0x11; 32], 50);

    let vault = AccountBuffer::<256>::new();
    setup_existing_vault(&vault, [0xAA; 32], /*is_signer*/ true);

    let receiver = AccountBuffer::<128>::new();
    setup_system_account(&receiver, [0xCC; 32], RECEIVER_LAMPORTS);

    let system_program = AccountBuffer::<128>::new();
    setup_system_program(&system_program);

    let mut accounts = match try_seedless(&payer, &vault, &receiver, &system_program) {
        Ok(accounts) => accounts,
        Err(err) => panic!("signed vault must load, got: {err:?}"),
    };

    if let Err(err) = accounts.exit_accounts(&[]) {
        panic!("close exit must succeed, got: {err:?}");
    }

    assert_eq!(
        unsafe { vault.view() }.lamports(),
        0,
        "closed vault must have zero lamports"
    );
    assert_eq!(
        unsafe { receiver.view() }.lamports(),
        RECEIVER_LAMPORTS + VAULT_LAMPORTS,
        "receiver must receive the closed vault's lamports"
    );
}
