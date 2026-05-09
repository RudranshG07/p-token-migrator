use anchor_lang::prelude::*;
use anchor_spl::token::{self, Burn, Mint, MintTo, Token, TokenAccount, Transfer};

declare_id!("VaU1t111111111111111111111111111111111111111");

#[program]
pub mod vault {
    use super::*;

    pub fn deposit(ctx: Context<Deposit>, amount: u64) -> Result<()> {
        let cpi_accounts = Transfer {
            from: ctx.accounts.user_token.to_account_info(),
            to: ctx.accounts.vault_token.to_account_info(),
            authority: ctx.accounts.user.to_account_info(),
        };
        let cpi_ctx = CpiContext::new(ctx.accounts.token_program.to_account_info(), cpi_accounts);
        token::transfer(cpi_ctx, amount)?;
        Ok(())
    }

    pub fn issue_receipt(ctx: Context<IssueReceipt>, amount: u64) -> Result<()> {
        let cpi_accounts = MintTo {
            mint: ctx.accounts.receipt_mint.to_account_info(),
            to: ctx.accounts.receipt_account.to_account_info(),
            authority: ctx.accounts.vault_authority.to_account_info(),
        };
        let signer_seeds: &[&[&[u8]]] = &[&[b"vault", &[ctx.bumps.vault_authority]]];
        let cpi_ctx = CpiContext::new(ctx.accounts.token_program.to_account_info(), cpi_accounts)
            .with_signer(signer_seeds);
        token::mint_to(cpi_ctx, amount)?;
        Ok(())
    }

    pub fn redeem(ctx: Context<Redeem>, amount: u64) -> Result<()> {
        let cpi_accounts = Burn {
            mint: ctx.accounts.receipt_mint.to_account_info(),
            from: ctx.accounts.receipt_account.to_account_info(),
            authority: ctx.accounts.user.to_account_info(),
        };
        let cpi_ctx = CpiContext::new(ctx.accounts.token_program.to_account_info(), cpi_accounts);
        token::burn(cpi_ctx, amount)?;
        Ok(())
    }
}

#[derive(Accounts)]
pub struct Deposit<'info> {
    #[account(mut)]
    pub user: Signer<'info>,
    #[account(mut)]
    pub user_token: Account<'info, TokenAccount>,
    #[account(mut)]
    pub vault_token: Account<'info, TokenAccount>,
    pub token_program: Program<'info, Token>,
}

#[derive(Accounts)]
pub struct IssueReceipt<'info> {
    #[account(mut)]
    pub receipt_mint: Account<'info, Mint>,
    #[account(mut)]
    pub receipt_account: Account<'info, TokenAccount>,
    /// CHECK: PDA authority for the sample vault.
    pub vault_authority: UncheckedAccount<'info>,
    pub token_program: Program<'info, Token>,
}

#[derive(Accounts)]
pub struct Redeem<'info> {
    pub user: Signer<'info>,
    #[account(mut)]
    pub receipt_mint: Account<'info, Mint>,
    #[account(mut)]
    pub receipt_account: Account<'info, TokenAccount>,
    pub token_program: Program<'info, Token>,
}
