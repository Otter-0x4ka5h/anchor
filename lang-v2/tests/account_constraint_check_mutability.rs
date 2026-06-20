use {
    anchor_lang_v2::{
        prelude::BorshAccount,
        testing::AccountBuffer,
        AccountConstraint, AnchorAccount, Discriminator, Owner,
        wincode::{SchemaRead, SchemaWrite},
    },
    pinocchio::address::Address,
    solana_program_error::ProgramError,
};

const PROGRAM_ID: [u8; 32] = [0x42; 32];

#[derive(SchemaRead, SchemaWrite, Default)]
struct Counter {
    value: u64,
}

impl Owner for Counter {
    const OWNER: Address = Address::new_from_array(PROGRAM_ID);
}

impl Discriminator for Counter {
    const DISCRIMINATOR: &'static [u8] = &[0xff, 0xb0, 0x04, 0xf5, 0xbc, 0xfd, 0x7c, 0x19];
}

struct MutateLamportsInCheck;

impl AccountConstraint<BorshAccount<Counter>> for MutateLamportsInCheck {
    type Value = u64;

    fn check(account: &BorshAccount<Counter>, lamports: &u64) -> Result<(), ProgramError> {
        let mut view = *account.account();
        view.set_lamports(*lamports);
        Ok(())
    }
}

fn setup_counter_buf(buf: &AccountBuffer<128>, value: u64) {
    buf.init([0x44; 32], PROGRAM_ID, 16, false, true, false);
    let mut data = [0u8; 16];
    data[..8].copy_from_slice(Counter::DISCRIMINATOR);
    data[8..16].copy_from_slice(&value.to_le_bytes());
    buf.write_data(&data);
    buf.set_lamports(100);
}

#[test]
fn check_hook_can_mutate_lamports_through_shared_borsh_account() {
    let buf = AccountBuffer::<128>::new();
    setup_counter_buf(&buf, 7);

    let view = unsafe { buf.view() };
    let account = BorshAccount::<Counter>::load(view).unwrap();

    <MutateLamportsInCheck as AccountConstraint<_>>::check(&account, &777).unwrap();

    assert_eq!(unsafe { buf.view() }.lamports(), 777);
    assert_eq!(account.value, 7);
}
