use anchor_lang::{
    error::{Error, ErrorCode},
    signature_verification::{
        load_instruction, verify_ed25519_ix, verify_ed25519_ix_with_instruction_index,
        verify_secp256k1_ix, verify_secp256k1_ix_with_instruction_index,
    },
    solana_program::{
        account_info::AccountInfo,
        instruction::{BorrowedAccountMeta, BorrowedInstruction, Instruction},
        pubkey::Pubkey,
    },
};
use solana_instructions_sysvar::{construct_instructions_data, store_current_index_checked};
use solana_sdk_ids::{ed25519_program, secp256k1_program, sysvar};

fn make_ed25519_instruction(
    pubkey: &[u8; 32],
    msg: &[u8],
    sig: &[u8; 64],
    instruction_index: u16,
) -> Instruction {
    const DATA_START: usize = 16;
    let pubkey_offset = DATA_START as u16;
    let sig_offset = pubkey_offset + pubkey.len() as u16;
    let msg_offset = sig_offset + sig.len() as u16;

    let mut data = Vec::with_capacity(DATA_START + sig.len() + pubkey.len() + msg.len());
    data.push(1u8);
    data.push(0u8);
    data.extend_from_slice(&sig_offset.to_le_bytes());
    data.extend_from_slice(&instruction_index.to_le_bytes());
    data.extend_from_slice(&pubkey_offset.to_le_bytes());
    data.extend_from_slice(&instruction_index.to_le_bytes());
    data.extend_from_slice(&msg_offset.to_le_bytes());
    data.extend_from_slice(&(msg.len() as u16).to_le_bytes());
    data.extend_from_slice(&instruction_index.to_le_bytes());
    data.extend_from_slice(pubkey);
    data.extend_from_slice(sig);
    data.extend_from_slice(msg);

    Instruction {
        program_id: ed25519_program::id(),
        accounts: vec![],
        data,
    }
}

fn make_secp256k1_instruction(
    eth_address: &[u8; 20],
    msg: &[u8],
    sig: &[u8; 64],
    recovery_id: u8,
    instruction_index: u8,
) -> Instruction {
    const DATA_START: usize = 12;
    let eth_offset = DATA_START as u16;
    let sig_offset = eth_offset + eth_address.len() as u16;
    let msg_offset = sig_offset + sig.len() as u16 + 1;

    let mut data = Vec::with_capacity(DATA_START + eth_address.len() + sig.len() + 1 + msg.len());
    data.push(1u8);
    data.extend_from_slice(&sig_offset.to_le_bytes());
    data.push(instruction_index);
    data.extend_from_slice(&eth_offset.to_le_bytes());
    data.push(instruction_index);
    data.extend_from_slice(&msg_offset.to_le_bytes());
    data.extend_from_slice(&(msg.len() as u16).to_le_bytes());
    data.push(instruction_index);
    data.extend_from_slice(eth_address);
    data.extend_from_slice(sig);
    data.push(recovery_id);
    data.extend_from_slice(msg);

    Instruction {
        program_id: secp256k1_program::id(),
        accounts: vec![],
        data,
    }
}

fn borrowed_instruction(ix: &Instruction) -> BorrowedInstruction<'_> {
    BorrowedInstruction {
        program_id: &ix.program_id,
        accounts: ix
            .accounts
            .iter()
            .map(|account| BorrowedAccountMeta {
                pubkey: &account.pubkey,
                is_signer: account.is_signer,
                is_writable: account.is_writable,
            })
            .collect(),
        data: &ix.data,
    }
}

fn make_instructions_sysvar_data(instructions: &[Instruction], current_index: u16) -> Vec<u8> {
    let borrowed = instructions
        .iter()
        .map(borrowed_instruction)
        .collect::<Vec<_>>();
    let mut data = construct_instructions_data(&borrowed);
    store_current_index_checked(&mut data, current_index).unwrap();
    data
}

fn assert_anchor_error(err: Error, expected: ErrorCode) {
    match err {
        Error::AnchorError(anchor_err) => {
            let expected_code: u32 = expected.into();
            assert_eq!(anchor_err.error_code_number, expected_code);
        }
        other => panic!("expected anchor error, got {other:?}"),
    }
}

#[test]
fn load_instruction_reads_from_sysvar() {
    let first = Instruction {
        program_id: Pubkey::new_unique(),
        accounts: vec![],
        data: vec![1, 2, 3],
    };
    let second = Instruction {
        program_id: Pubkey::new_unique(),
        accounts: vec![],
        data: vec![4, 5, 6],
    };
    let mut data = make_instructions_sysvar_data(&[first.clone(), second.clone()], 1);
    let key = sysvar::instructions::id();
    let owner = sysvar::id();
    let mut lamports = 0;
    let ix_sysvar = AccountInfo::new(&key, false, false, &mut lamports, &mut data, &owner, false);

    let loaded = load_instruction(1, &ix_sysvar).unwrap();
    assert_eq!(loaded, second);
}

#[test]
fn verify_ed25519_ix_accepts_inline_layout() {
    let pubkey = [7u8; 32];
    let msg = b"anchor-ed25519-inline";
    let sig = [3u8; 64];
    let ix = make_ed25519_instruction(&pubkey, msg, &sig, u16::MAX);

    verify_ed25519_ix(&ix, &pubkey, msg, &sig).unwrap();
}

#[test]
fn verify_ed25519_ix_accepts_custom_instruction_index() {
    let pubkey = [8u8; 32];
    let msg = b"anchor-ed25519-custom-index";
    let sig = [4u8; 64];
    let ix = make_ed25519_instruction(&pubkey, msg, &sig, 7);

    verify_ed25519_ix_with_instruction_index(&ix, 7, &pubkey, msg, &sig).unwrap();
}

#[test]
fn verify_ed25519_ix_rejects_mismatched_bytes() {
    let pubkey = [9u8; 32];
    let msg = b"anchor-ed25519-bad";
    let sig = [5u8; 64];
    let mut ix = make_ed25519_instruction(&pubkey, msg, &sig, u16::MAX);
    ix.data[20] ^= 1;

    let err = verify_ed25519_ix(&ix, &pubkey, msg, &sig).unwrap_err();
    assert_anchor_error(err, ErrorCode::SignatureVerificationFailed);
}

#[test]
fn verify_secp256k1_ix_accepts_first_instruction_layout() {
    let eth_address = [1u8; 20];
    let msg = b"anchor-secp-inline";
    let sig = [6u8; 64];
    let recovery_id = 1;
    let ix = make_secp256k1_instruction(&eth_address, msg, &sig, recovery_id, 0);

    verify_secp256k1_ix(&ix, &eth_address, msg, &sig, recovery_id).unwrap();
}

#[test]
fn verify_secp256k1_ix_accepts_custom_instruction_index() {
    let eth_address = [2u8; 20];
    let msg = b"anchor-secp-custom-index";
    let sig = [7u8; 64];
    let recovery_id = 0;
    let ix = make_secp256k1_instruction(&eth_address, msg, &sig, recovery_id, 3);

    verify_secp256k1_ix_with_instruction_index(&ix, 3, &eth_address, msg, &sig, recovery_id)
        .unwrap();
}

#[test]
fn verify_secp256k1_ix_rejects_invalid_recovery_id() {
    let eth_address = [3u8; 20];
    let msg = b"anchor-secp-invalid-recovery";
    let sig = [8u8; 64];
    let recovery_id = 2;
    let ix = make_secp256k1_instruction(&eth_address, msg, &sig, recovery_id, 0);

    let err = verify_secp256k1_ix(&ix, &eth_address, msg, &sig, recovery_id).unwrap_err();
    assert_anchor_error(err, ErrorCode::InvalidRecoveryId);
}
