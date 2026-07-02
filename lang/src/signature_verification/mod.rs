use crate::prelude::*;
use crate::solana_program::instruction::Instruction;
use solana_instructions_sysvar::load_instruction_at_checked;

mod ed25519;
mod secp256k1;

pub use ed25519::{verify_ed25519_ix, verify_ed25519_ix_with_instruction_index};
pub use secp256k1::{verify_secp256k1_ix, verify_secp256k1_ix_with_instruction_index};

/// Loads an instruction from the instructions sysvar and normalizes failures
/// into Anchor errors.
pub fn load_instruction(index: usize, ix_sysvar: &AccountInfo<'_>) -> Result<Instruction> {
    load_instruction_at_checked(index, ix_sysvar)
        .map_err(|_| error!(error::ErrorCode::ConstraintRaw))
}
