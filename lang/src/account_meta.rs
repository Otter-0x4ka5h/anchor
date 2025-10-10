use crate::ToAccountMetas;
use solana_instruction::AccountMeta;

#[cfg(not(feature = "std"))]
use alloc::{vec::Vec, vec};

#[cfg(feature = "std")]
use std::{vec::Vec, vec};

impl ToAccountMetas for AccountMeta {
    fn to_account_metas(&self, _is_signer: Option<bool>) -> Vec<AccountMeta> {
        vec![self.clone()]
    }
}
