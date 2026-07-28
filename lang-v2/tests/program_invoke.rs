//! Run: `cargo test -p anchor-lang-v2 --features testing --test program_invoke`

use {
    anchor_lang_v2::{
        accounts::Account,
        prelude::BorshAccount,
        solana_program::{
            instruction::{AccountMeta, Instruction},
            program,
        },
        testing::{AccountBuffer, MIN_ACCOUNT_BUF},
        wincode::{SchemaRead, SchemaWrite},
        Address, AnchorAccount, CpiContext, CpiHandle, CpiHandleMut, Discriminator, Owner,
        ToCpiAccounts, ToCpiHandle, ToCpiHandleMut,
    },
    bytemuck::{Pod, Zeroable},
    solana_program_error::ProgramError,
};

const ID: Address = Address::new_from_array([7; 32]);
const PROGRAM_ID: [u8; 32] = [0x42; 32];

#[derive(ToCpiAccounts)]
struct ReadonlyCpi<'a> {
    account: CpiHandle<'a>,
}

#[derive(ToCpiAccounts)]
struct WritableCpi<'a> {
    account: CpiHandleMut<'a>,
}

#[derive(ToCpiAccounts)]
struct OptionalCpi<'a> {
    account: CpiHandle<'a>,
    optional: Option<CpiHandle<'a>>,
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct PodCounter {
    value: u64,
}

impl Owner for PodCounter {
    const OWNER: Address = Address::new_from_array(PROGRAM_ID);
}

impl Discriminator for PodCounter {
    const DISCRIMINATOR: &'static [u8] = &[0x4c, 0xde, 0x7f, 0x28, 0x61, 0x2f, 0x07, 0x73];
}

#[derive(SchemaRead, SchemaWrite, Default, Clone, PartialEq, Debug)]
struct BorshCounter {
    value: u64,
}

impl Owner for BorshCounter {
    const OWNER: Address = Address::new_from_array(PROGRAM_ID);
}

impl Discriminator for BorshCounter {
    const DISCRIMINATOR: &'static [u8] = &[0xff, 0xb0, 0x04, 0xf5, 0xbc, 0xfd, 0x7c, 0x19];
}

fn account_view(address: [u8; 32], writable: bool) -> AccountBuffer<{ MIN_ACCOUNT_BUF + 8 }> {
    let buffer = AccountBuffer::new();
    buffer.init(address, [9; 32], 8, false, writable, false);
    buffer
}

fn slab_account_view(
    address: [u8; 32],
    writable: bool,
    value: u64,
) -> AccountBuffer<{ MIN_ACCOUNT_BUF + 8 }> {
    let buffer = AccountBuffer::new();
    buffer.init(
        address,
        PROGRAM_ID,
        8 + core::mem::size_of::<PodCounter>(),
        false,
        writable,
        false,
    );
    let mut data = [0u8; 16];
    data[..8].copy_from_slice(PodCounter::DISCRIMINATOR);
    data[8..16].copy_from_slice(&value.to_le_bytes());
    buffer.write_data(&data);
    buffer
}

fn borsh_account_view(address: [u8; 32], writable: bool, value: u64) -> AccountBuffer<256> {
    let buffer = AccountBuffer::new();
    buffer.init(address, PROGRAM_ID, 16, false, writable, false);
    let mut data = [0u8; 16];
    data[..8].copy_from_slice(BorshCounter::DISCRIMINATOR);
    data[8..16].copy_from_slice(&value.to_le_bytes());
    buffer.write_data(&data);
    buffer.set_lamports(1_000_000_000);
    buffer
}

fn instruction(account: Address, writable: bool) -> Instruction {
    let meta = if writable {
        AccountMeta::new(account, false)
    } else {
        AccountMeta::new_readonly(account, false)
    };

    Instruction {
        program_id: ID,
        accounts: vec![meta],
        data: vec![1, 2, 3],
    }
}

#[test]
fn checked_invoke_accepts_matching_handles() {
    let buffer = account_view([1; 32], true);
    let mut view = unsafe { buffer.view() };
    let ix = instruction(*view.address(), true);
    let handles = [CpiHandle::writable(&mut view)];

    program::invoke(&ix, &handles).unwrap();
}

#[test]
fn account_view_converts_to_cpi_handles() {
    let buffer = account_view([1; 32], true);
    let mut view = unsafe { buffer.view() };
    let address = *view.address();

    let readonly = view.to_cpi_handle();
    assert_eq!(*readonly.address(), address);
    assert!(!readonly.is_writable());

    let writable = view.to_cpi_handle_mut();
    assert_eq!(*writable.address(), address);
    assert!(writable.is_writable());
}

#[test]
fn checked_invoke_rejects_missing_handle() {
    let ix = instruction(Address::new_from_array([1; 32]), false);

    let err = program::invoke(&ix, &[]).unwrap_err();

    assert_eq!(err, ProgramError::NotEnoughAccountKeys);
}

#[test]
fn checked_invoke_accepts_optional_none_program_id_sentinel() {
    let buffer = account_view([1; 32], false);
    let view = unsafe { buffer.view() };
    let ix = Instruction {
        program_id: ID,
        accounts: vec![
            AccountMeta::new_readonly(*view.address(), false),
            AccountMeta::new_readonly(ID, false),
        ],
        data: vec![],
    };
    let handles = [view.to_cpi_handle()];

    program::invoke(&ix, &handles).unwrap();
}

#[test]
fn checked_invoke_rejects_address_mismatch() {
    let buffer = account_view([1; 32], false);
    let view = unsafe { buffer.view() };
    let ix = instruction(Address::new_from_array([2; 32]), false);
    let handles = [CpiHandle::readonly(&view)];

    let err = program::invoke(&ix, &handles).unwrap_err();

    assert_eq!(err, ProgramError::InvalidArgument);
}

#[test]
fn checked_invoke_rejects_readonly_handle_for_writable_meta() {
    let buffer = account_view([1; 32], true);
    let view = unsafe { buffer.view() };
    let ix = instruction(*view.address(), true);
    let handles = [CpiHandle::readonly(&view)];

    let err = program::invoke(&ix, &handles).unwrap_err();

    assert_eq!(err, ProgramError::InvalidArgument);
}

#[test]
fn invoke_ix_rejects_readonly_handle_for_writable_meta() {
    let program = ID;
    let buffer = account_view([1; 32], true);
    let view = unsafe { buffer.view() };
    let accounts = ReadonlyCpi {
        account: view.to_cpi_handle(),
    };
    let ix = Instruction {
        program_id: program,
        accounts: vec![AccountMeta::new(*view.address(), false)],
        data: vec![],
    };

    let err = CpiContext::new(&program, accounts)
        .invoke_ix(ix)
        .unwrap_err();

    assert_eq!(err, ProgramError::InvalidArgument);
}

#[test]
fn invoke_ix_accepts_optional_none_program_id_sentinel() {
    let program = ID;
    let buffer = account_view([1; 32], false);
    let view = unsafe { buffer.view() };
    let accounts = OptionalCpi {
        account: view.to_cpi_handle(),
        optional: None,
    };
    let ix = Instruction {
        program_id: program,
        accounts: vec![
            AccountMeta::new_readonly(*view.address(), false),
            AccountMeta::new_readonly(program, false),
        ],
        data: vec![],
    };

    CpiContext::new(&program, accounts).invoke_ix(ix).unwrap();
}

#[test]
fn invoke_ix_rejects_writable_program_id_meta_without_handle() {
    let program = ID;
    let buffer = account_view([1; 32], false);
    let view = unsafe { buffer.view() };
    let accounts = OptionalCpi {
        account: view.to_cpi_handle(),
        optional: None,
    };
    let ix = Instruction {
        program_id: program,
        accounts: vec![
            AccountMeta::new_readonly(*view.address(), false),
            AccountMeta::new(program, false),
        ],
        data: vec![],
    };

    let err = CpiContext::new(&program, accounts)
        .invoke_ix(ix)
        .unwrap_err();

    assert_eq!(err, ProgramError::NotEnoughAccountKeys);
}

#[test]
fn checked_invoke_rejects_live_borrow_for_writable_meta() {
    let buffer = account_view([1; 32], true);
    let mut view = unsafe { buffer.view() };
    let borrow_view = view;
    let _borrow = borrow_view.try_borrow().unwrap();
    let ix = instruction(*view.address(), true);
    let handles = [CpiHandle::writable(&mut view)];

    let err = program::invoke(&ix, &handles).unwrap_err();

    assert_eq!(err, ProgramError::AccountBorrowFailed);
}

#[test]
fn checked_invoke_rejects_live_mut_borrow_for_writable_meta() {
    let buffer = account_view([1; 32], true);
    let mut view = unsafe { buffer.view() };
    let mut borrow_view = view;
    let _borrow = borrow_view.try_borrow_mut().unwrap();
    let ix = instruction(*view.address(), true);
    let handles = [view.to_cpi_handle_mut().into()];

    let err = program::invoke(&ix, &handles).unwrap_err();

    assert_eq!(err, ProgramError::AccountBorrowFailed);
}

#[test]
fn checked_invoke_accepts_mutable_slab_handle() {
    let buffer = slab_account_view([1; 32], true, 9);
    let view = unsafe { buffer.view() };
    let mut acct = unsafe { Account::<PodCounter>::load_mut(view) }.unwrap();
    let address = *acct.address();
    let ix = instruction(address, true);
    let handles = [acct.cpi_handle_mut().into()];

    program::invoke(&ix, &handles).unwrap();
}

#[test]
fn cpi_context_invoke_rejects_live_borrow_for_writable_meta() {
    let program = ID;
    let buffer = account_view([1; 32], true);
    let mut view = unsafe { buffer.view() };
    let borrow_view = view;
    let _borrow = borrow_view.try_borrow().unwrap();
    let accounts = WritableCpi {
        account: view.to_cpi_handle_mut(),
    };

    let err = CpiContext::new(&program, accounts)
        .invoke(&[1, 2, 3])
        .unwrap_err();

    assert_eq!(err, ProgramError::AccountBorrowFailed);
}

#[test]
fn cpi_context_invoke_accepts_mutable_slab_handle() {
    let program = ID;
    let buffer = slab_account_view([1; 32], true, 9);
    let view = unsafe { buffer.view() };
    let mut acct = unsafe { Account::<PodCounter>::load_mut(view) }.unwrap();
    let accounts = WritableCpi {
        account: acct.cpi_handle_mut(),
    };

    CpiContext::new(&program, accounts)
        .invoke(&[1, 2, 3])
        .unwrap();
}

#[test]
fn invoke_ix_rejects_live_borrow_for_writable_meta() {
    let program = ID;
    let buffer = account_view([1; 32], true);
    let mut view = unsafe { buffer.view() };
    let address = *view.address();
    let borrow_view = view;
    let _borrow = borrow_view.try_borrow().unwrap();
    let accounts = WritableCpi {
        account: view.to_cpi_handle_mut(),
    };
    let ix = Instruction {
        program_id: program,
        accounts: vec![AccountMeta::new(address, false)],
        data: vec![],
    };

    let err = CpiContext::new(&program, accounts)
        .invoke_ix(ix)
        .unwrap_err();

    assert_eq!(err, ProgramError::AccountBorrowFailed);
}

#[test]
fn invoke_ix_accepts_mutable_slab_handle() {
    let program = ID;
    let buffer = slab_account_view([1; 32], true, 9);
    let view = unsafe { buffer.view() };
    let mut acct = unsafe { Account::<PodCounter>::load_mut(view) }.unwrap();
    let address = *acct.address();
    let accounts = WritableCpi {
        account: acct.cpi_handle_mut(),
    };
    let ix = Instruction {
        program_id: program,
        accounts: vec![AccountMeta::new(address, false)],
        data: vec![],
    };

    CpiContext::new(&program, accounts).invoke_ix(ix).unwrap();
}

#[test]
fn cpi_context_invoke_accepts_mutable_borsh_handle() {
    let program = ID;
    let buffer = borsh_account_view([1; 32], true, 9);
    let view = unsafe { buffer.view() };
    let mut acct = unsafe { BorshAccount::<BorshCounter>::load_mut(view) }.unwrap();

    {
        let accounts = WritableCpi {
            account: acct.cpi_handle_mut(),
        };
        CpiContext::new(&program, accounts)
            .invoke(&[1, 2, 3])
            .unwrap();
    }

    acct.value = 11;
    assert_eq!(acct.value, 11);
}

#[test]
fn invoke_ix_accepts_mutable_borsh_handle() {
    let program = ID;
    let buffer = borsh_account_view([1; 32], true, 9);
    let view = unsafe { buffer.view() };
    let mut acct = unsafe { BorshAccount::<BorshCounter>::load_mut(view) }.unwrap();
    let address = *acct.address();

    {
        let accounts = WritableCpi {
            account: acct.cpi_handle_mut(),
        };
        let ix = Instruction {
            program_id: program,
            accounts: vec![AccountMeta::new(address, false)],
            data: vec![],
        };
        CpiContext::new(&program, accounts).invoke_ix(ix).unwrap();
    }

    acct.value = 11;
    assert_eq!(acct.value, 11);
}

#[test]
fn unchecked_handle_api_is_available() {
    let ix = Instruction {
        program_id: Address::new_from_array([7; 32]),
        accounts: vec![],
        data: vec![],
    };

    unsafe { program::invoke_unchecked(&ix, &[]) }.unwrap();
}
