use anchor_lang::prelude::*;
use anchor_lang::solana_program::program::invoke;

declare_id!("145kzfkNvXKJx97e3beWGaB4azDNqv6KcjGrvZym6fQb");

#[program]
pub mod good_one {
    use super::*;

    pub fn initialize(ctx: Context<Initialize>) -> Result<()> {
        let user_account = &mut ctx.accounts.user_account;
        user_account.balance = 100;
        user_account.authority = ctx.accounts.authority.key();
        Ok(())
    }

    pub fn transfer_with_cpi_bad(ctx: Context<Transfer>, amount: u64) -> Result<()> {
        let instruction = anchor_lang::solana_program::instruction::Instruction {
            program_id: anchor_lang::solana_program::system_program::ID,
            accounts: vec![],
            data: vec![],
        };

        ctx.accounts.user_account.reload()?;

        let i = &mut ctx.accounts.user_account;
        let i = &mut ctx.accounts.beneficiary;

        invoke(&instruction, &[
            i.to_account_info(),
        ])?;

        let i = &mut ctx.accounts.beneficiary;
        i.reload()?;

        let balance = ctx.accounts.user_account.balance;
        i.balance = 0u64 - amount;

        Ok(())
    }
}

#[derive(Accounts)]
pub struct Initialize<'info> {
    #[account(init, payer = authority, space = 8 + 8 + 32)]
    pub user_account: Account<'info, UserAccount>,
    #[account(mut)]
    pub authority: Signer<'info>,
    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
pub struct Transfer<'info> {
    #[account(mut)]
    pub user_account: Account<'info, UserAccount>,
    #[account(mut)]
    pub beneficiary: Account<'info, UserAccount>,
    #[account(mut)]
    pub authority: Signer<'info>,
}

#[account]
pub struct UserAccount {
    pub balance: u64,
    pub authority: Pubkey,
}
