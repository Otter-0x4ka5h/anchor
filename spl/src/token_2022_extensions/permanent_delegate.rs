// Avoiding AccountInfo deprecated msg in anchor context
#![allow(deprecated)]
use {
    anchor_lang::{
        context::CpiContext,
        solana_program::{account_info::AccountInfo, pubkey::Pubkey},
        Accounts, Result,
    },
    spl_token_2022_interface as spl_token_2022,
};

pub fn permanent_delegate_initialize<'info>(
    ctx: CpiContext<'_, '_, '_, 'info, PermanentDelegateInitialize<'info>>,
    permanent_delegate: &Pubkey,
) -> Result<()> {
    let ix = spl_token_2022::instruction::initialize_permanent_delegate(
        &ctx.program_id,
        ctx.accounts.mint.key,
        permanent_delegate,
    )?;
    anchor_lang::solana_program::program::invoke_signed(&ix, &[ctx.accounts.mint], ctx.signer_seeds)
        .map_err(Into::into)
}

#[derive(Accounts)]
pub struct PermanentDelegateInitialize<'info> {
    pub mint: AccountInfo<'info>,
}
