use {
    anchor_lang_v2::{
        accounts::{BorshAccount, Slab},
        bytemuck::{Pod, Zeroable},
        testing::AccountBuffer,
        wincode::{SchemaRead, SchemaWrite},
        AnchorAccount, Discriminator, Owner,
    },
    pinocchio::{account::RuntimeAccount, address::Address},
    solana_program_error::ProgramError,
};

const PROGRAM_ID: [u8; 32] = [0x42; 32];

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct Counter {
    value: u64,
    _pad: [u8; 8],
}

impl Owner for Counter {
    const OWNER: Address = Address::new_from_array(PROGRAM_ID);
}

impl Discriminator for Counter {
    const DISCRIMINATOR: &'static [u8] = &[0xff, 0xb0, 0x04, 0xf5, 0xbc, 0xfd, 0x7c, 0x19];
}

type CounterAccount = Slab<Counter>;

#[derive(Clone, Default, SchemaRead, SchemaWrite)]
struct Vault {
    authority: [u8; 32],
    balance: u64,
}

impl Owner for Vault {
    const OWNER: Address = Address::new_from_array(PROGRAM_ID);
}

impl Discriminator for Vault {
    const DISCRIMINATOR: &'static [u8] = &[0xd3, 0x08, 0xe8, 0x2b, 0x02, 0x98, 0x75, 0x77];
}

fn setup_counter(buf: &mut AccountBuffer<256>, writable: bool) {
    let data_len = 8 + core::mem::size_of::<Counter>();
    buf.init([0xAA; 32], PROGRAM_ID, data_len, false, writable, false);
    let mut data = [0u8; 24];
    data[..8].copy_from_slice(Counter::DISCRIMINATOR);
    data[8..16].copy_from_slice(&42u64.to_le_bytes());
    buf.write_data(&data);
}

fn setup_vault(buf: &mut AccountBuffer<256>) {
    let data_len = 8 + 32 + 8;
    buf.init([0xAB; 32], PROGRAM_ID, data_len, false, true, false);
    let mut data = [0u8; 48];
    data[..8].copy_from_slice(Vault::DISCRIMINATOR);
    data[8..40].copy_from_slice(&[0xCC; 32]);
    data[40..48].copy_from_slice(&999u64.to_le_bytes());
    buf.write_data(&data);
    buf.set_lamports(1_000_000_000);
}

fn runtime_account(buf: &AccountBuffer<256>) -> &RuntimeAccount {
    unsafe { &*(buf.raw() as *const RuntimeAccount) }
}

#[test]
fn slab_read_only_load_allows_shared_borrow_on_copy() {
    let mut buf = AccountBuffer::<256>::new();
    setup_counter(&mut buf, false);
    let view = unsafe { buf.view() };

    let account = CounterAccount::load(view).unwrap();
    let view_copy = view;
    assert!(view_copy.try_borrow().is_ok());
    assert_eq!(account.value, 42);
}

#[test]
fn slab_read_only_load_blocks_unsafe_mut_reload_on_copy() {
    let mut buf = AccountBuffer::<256>::new();
    setup_counter(&mut buf, true);
    let view = unsafe { buf.view() };

    let account = CounterAccount::load(view).unwrap();
    let err = unsafe { CounterAccount::load_mut(view) }.err();
    assert_eq!(err, Some(ProgramError::AccountBorrowFailed));
    assert_eq!(account.value, 42);
}

#[test]
fn serialized_account_close_loaded_mutably_succeeds() {
    let mut buf = AccountBuffer::<256>::new();
    setup_vault(&mut buf);

    let dest_buf = AccountBuffer::<256>::new();
    dest_buf.init([0xDD; 32], PROGRAM_ID, 0, false, true, false);
    dest_buf.set_lamports(100);

    {
        let view = unsafe { buf.view() };
        let dest_view = unsafe { dest_buf.view() };
        let mut vault = unsafe { BorshAccount::<Vault>::load_mut(view) }.unwrap();
        vault.close(dest_view).unwrap();
    }

    assert_eq!(runtime_account(&buf).lamports, 0);
    assert_eq!(runtime_account(&buf).data_len, 0);
    assert_eq!(runtime_account(&dest_buf).lamports, 1_000_000_100);
}

#[test]
#[should_panic(
    expected = "SerializedAccount mutated through a read-only load. Add #[account(mut)] to your accounts struct."
)]
fn serialized_account_close_panics_when_loaded_read_only() {
    let mut buf = AccountBuffer::<256>::new();
    setup_vault(&mut buf);

    let dest_buf = AccountBuffer::<256>::new();
    dest_buf.init([0xDD; 32], PROGRAM_ID, 0, false, true, false);

    let view = unsafe { buf.view() };
    let dest_view = unsafe { dest_buf.view() };
    let mut vault = BorshAccount::<Vault>::load(view).unwrap();
    vault.close(dest_view).unwrap();
}
