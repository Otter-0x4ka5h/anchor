//! Regression tests for optional mutable account duplicate handling.
//!
//! The derive must reject duplicate optional mutable accounts before any
//! `unsafe load_mut` call runs, while still allowing duplicate `None`
//! sentinels (`address == program_id`) to stay silent.

use {
    anchor_lang_v2::{
        cursor::AccountCursor,
        testing::{AccountRecord, SbfInputBuffer},
        Accounts, AnchorAccount, ErrorCode, TryAccounts,
    },
    core::{mem::MaybeUninit, ops::Deref},
    pinocchio::{account::AccountView, address::Address},
    solana_program_error::ProgramError,
    std::sync::atomic::{AtomicUsize, Ordering},
};

anchor_lang_v2::declare_id!("11111111111111111111111111111111");

const PROGRAM_ID: [u8; 32] = [0x42; 32];

static LOAD_MUT_CALLS: AtomicUsize = AtomicUsize::new(0);

struct SpyAccount {
    view: AccountView,
}

impl Deref for SpyAccount {
    type Target = AccountView;

    fn deref(&self) -> &Self::Target {
        &self.view
    }
}

impl AnchorAccount for SpyAccount {
    type Data = AccountView;

    fn load(view: AccountView) -> Result<Self, ProgramError> {
        Ok(Self { view })
    }

    unsafe fn load_mut(view: AccountView) -> Result<Self, ProgramError> {
        LOAD_MUT_CALLS.fetch_add(1, Ordering::SeqCst);
        if !view.is_writable() {
            return Err(ErrorCode::ConstraintMut.into());
        }
        Ok(Self { view })
    }

    fn account(&self) -> &AccountView {
        &self.view
    }
}

#[derive(Accounts)]
struct OptionalSpyAccounts {
    #[account(mut)]
    a: Option<SpyAccount>,
    #[account(mut)]
    b: Option<SpyAccount>,
}

fn fresh_lookup() -> Vec<MaybeUninit<AccountView>> {
    let mut v: Vec<MaybeUninit<AccountView>> = Vec::with_capacity(256);
    for _ in 0..256 {
        v.push(MaybeUninit::uninit());
    }
    v
}

fn expect_err<T>(r: Result<T, ProgramError>) -> ProgramError {
    match r {
        Ok(_) => panic!("expected Err, got Ok"),
        Err(e) => e,
    }
}

fn writable_non_dup(address: [u8; 32]) -> AccountRecord {
    AccountRecord::NonDup {
        address,
        owner: [0xAA; 32],
        lamports: 100,
        is_signer: false,
        is_writable: true,
        executable: false,
        data_len: 0,
    }
}

#[test]
fn duplicate_optional_mut_rejects_before_any_load_mut() {
    LOAD_MUT_CALLS.store(0, Ordering::SeqCst);

    let records = [
        writable_non_dup([0x11; 32]),
        AccountRecord::Dup { index: 0 },
    ];
    let mut sbf = SbfInputBuffer::build(&records);
    let mut lookup = fresh_lookup();
    let mut cursor =
        unsafe { AccountCursor::new(sbf.as_mut_ptr(), lookup.as_mut_ptr() as *mut AccountView) };
    let (views, duplicates) = unsafe { cursor.walk_n(records.len()) };

    let err = expect_err(OptionalSpyAccounts::try_accounts(
        &Address::new_from_array(PROGRAM_ID),
        views,
        duplicates,
        0,
        &[],
    ));
    assert_eq!(err, ErrorCode::ConstraintDuplicateMutableAccount.into());
    assert_eq!(LOAD_MUT_CALLS.load(Ordering::SeqCst), 0);
}

#[test]
fn duplicate_optional_none_sentinels_stay_silent() {
    LOAD_MUT_CALLS.store(0, Ordering::SeqCst);

    let records = [
        writable_non_dup(PROGRAM_ID),
        AccountRecord::Dup { index: 0 },
    ];
    let mut sbf = SbfInputBuffer::build(&records);
    let mut lookup = fresh_lookup();
    let mut cursor =
        unsafe { AccountCursor::new(sbf.as_mut_ptr(), lookup.as_mut_ptr() as *mut AccountView) };
    let (views, duplicates) = unsafe { cursor.walk_n(records.len()) };

    let (accounts, _, _) = OptionalSpyAccounts::try_accounts(
        &Address::new_from_array(PROGRAM_ID),
        views,
        duplicates,
        0,
        &[],
    )
    .unwrap();
    assert!(accounts.a.is_none());
    assert!(accounts.b.is_none());
    assert_eq!(LOAD_MUT_CALLS.load(Ordering::SeqCst), 0);
}
