//! Smoke test for `init_if_needed` with an explicit `space = ...` account.
//!
//! Runtime signer enforcement for seedless `init_if_needed` (+ `close`) lives
//! in `init_if_needed_signer_check.rs`. End-to-end LiteSVM coverage is in
//! `tests-v2`.

use {
    anchor_lang_v2::{
        accounts::{Account, Program, SystemAccount},
        programs::System,
        Accounts, AnchorAccount, Discriminator, Id, InitSpace, Owner, Space,
    },
    bytemuck::{Pod, Zeroable},
    pinocchio::address::Address,
};

anchor_lang_v2::declare_id!("11111111111111111111111111111111");

const PROGRAM_ID: [u8; 32] = [0x42; 32];

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

/// Compile smoke for the seedless surface (signer enforced at runtime in
/// `init_if_needed_signer_check.rs`).
#[allow(dead_code)]
#[derive(Accounts)]
struct InitIfNeededSeedlessClose {
    #[account(mut)]
    payer: SystemAccount,
    #[account(init_if_needed, payer = payer, space = VAULT_SPACE, close = receiver)]
    vault: Account<Vault>,
    #[account(mut)]
    receiver: SystemAccount,
    system_program: Program<System>,
}

#[test]
fn init_if_needed_space_derive_smoke() {
    assert_eq!(VAULT_SPACE, 16);
}
