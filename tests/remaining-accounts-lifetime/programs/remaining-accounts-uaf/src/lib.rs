use anchor_lang::prelude::*;

declare_id!("Fg6PaFpoGXkYsidMpWxTWqkZP8eM5uZuN4fKwvVZZzCk");

#[program]
pub mod remaining_accounts_uaf {
    use super::*;

    pub fn exploit(ctx: Context<Exploit>) -> Result<()> {
        *ctx.remaining_accounts[0].lamports.borrow_mut() = &mut ctx.accounts.data.value;
        Ok(())
    }
}

#[derive(Accounts)]
pub struct Exploit<'info> {
    #[account(mut)]
    pub data: Account<'info, Data>,
}

#[account]
pub struct Data {
    pub value: u64,
}
