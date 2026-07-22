use {
    anchor_lang_v2::{
        accounts::{BorshAccount, Signer, UncheckedAccount},
        testing::AccountBuffer,
        wincode::{SchemaRead, SchemaWrite},
        AccountConstraint, Accounts, AnchorAccount, Discriminator, ErrorCode, Owner, TryAccounts,
    },
    pinocchio::address::Address,
    solana_program_error::ProgramError,
};

const PROGRAM_ID: [u8; 32] = [0x42; 32];
const OLD_AUTHORITY: [u8; 32] = [0x10; 32];
const NEW_AUTHORITY: [u8; 32] = [0x20; 32];

#[derive(SchemaRead, SchemaWrite, Clone, Copy)]
struct Vault {
    current_authority: Address,
}

impl Owner for Vault {
    const OWNER: Address = Address::new_from_array(PROGRAM_ID);
}

impl Discriminator for Vault {
    const DISCRIMINATOR: &'static [u8] = &[0x41, 0x75, 0x74, 0x68, 0x56, 0x61, 0x75, 0x6c];
}

mod role {
    use super::*;

    pub struct SetAuthorityConstraint;

    impl AccountConstraint<BorshAccount<Vault>> for SetAuthorityConstraint {
        type Value = pinocchio::account::AccountView;

        fn update(
            account: &mut BorshAccount<Vault>,
            new_authority: &Self::Value,
        ) -> Result<(), ProgramError> {
            account.current_authority = *new_authority.address();
            Ok(())
        }
    }
}

#[derive(Accounts)]
struct RotateAuthority {
    #[account(mut, has_one = current_authority, update(role::set_authority = new_authority))]
    vault: BorshAccount<Vault>,
    current_authority: Signer,
    new_authority: UncheckedAccount,
}

fn expect_err<T>(result: Result<T, ProgramError>) -> ProgramError {
    match result {
        Ok(_) => panic!("expected Err, got Ok"),
        Err(err) => err,
    }
}

fn vault_account(authority: [u8; 32]) -> AccountBuffer<128> {
    let buf = AccountBuffer::<128>::new();
    let mut data = [0u8; 40];
    data[..8].copy_from_slice(Vault::DISCRIMINATOR);
    data[8..40].copy_from_slice(&authority);
    buf.init([0xAA; 32], PROGRAM_ID, data.len(), false, true, false);
    buf.write_data(&data);
    buf
}

fn signer_account(address: [u8; 32], signer: bool) -> AccountBuffer<128> {
    let buf = AccountBuffer::<128>::new();
    buf.init(address, PROGRAM_ID, 0, signer, false, false);
    buf
}

fn unchecked_account(address: [u8; 32]) -> AccountBuffer<128> {
    let buf = AccountBuffer::<128>::new();
    buf.init(address, PROGRAM_ID, 0, false, false, false);
    buf
}

fn read_vault_authority(buf: &AccountBuffer<128>) -> [u8; 32] {
    let data = buf.read_data();
    data[8..40].try_into().unwrap()
}

#[test]
fn try_accounts_checks_authority_before_running_updates() {
    let vault = vault_account(OLD_AUTHORITY);
    let current = signer_account(NEW_AUTHORITY, true);
    let replacement = unchecked_account(NEW_AUTHORITY);
    let views = [unsafe { vault.view() }, unsafe { current.view() }, unsafe {
        replacement.view()
    }];

    let err = expect_err(RotateAuthority::try_accounts(
        &Address::new_from_array(PROGRAM_ID),
        &views,
        None,
        0,
        &[],
    ));

    assert_eq!(err, ErrorCode::ConstraintHasOne.into());
    assert_eq!(read_vault_authority(&vault), OLD_AUTHORITY);
}

#[test]
fn update_accounts_runs_after_validation_and_persists_on_exit() {
    let vault = vault_account(OLD_AUTHORITY);
    let current = signer_account(OLD_AUTHORITY, true);
    let replacement = unchecked_account(NEW_AUTHORITY);
    let views = [unsafe { vault.view() }, unsafe { current.view() }, unsafe {
        replacement.view()
    }];

    let (mut accounts, _, _) = <RotateAuthority as TryAccounts>::validate_accounts(
        &Address::new_from_array(PROGRAM_ID),
        &views,
        None,
        0,
        &[],
    )
    .expect("authority check should pass before updates");

    assert_eq!(accounts.vault.current_authority.to_bytes(), OLD_AUTHORITY);

    accounts.update_accounts().unwrap();
    assert_eq!(accounts.vault.current_authority.to_bytes(), NEW_AUTHORITY);

    accounts.exit_accounts().unwrap();
    assert_eq!(read_vault_authority(&vault), NEW_AUTHORITY);
}

#[test]
fn try_accounts_still_runs_updates_for_direct_callers() {
    let vault = vault_account(OLD_AUTHORITY);
    let current = signer_account(OLD_AUTHORITY, true);
    let replacement = unchecked_account(NEW_AUTHORITY);
    let views = [unsafe { vault.view() }, unsafe { current.view() }, unsafe {
        replacement.view()
    }];

    let (accounts, _, _) =
        RotateAuthority::try_accounts(&Address::new_from_array(PROGRAM_ID), &views, None, 0, &[])
            .expect("direct callers should still receive updated accounts");

    assert_eq!(accounts.vault.current_authority.to_bytes(), NEW_AUTHORITY);
}
