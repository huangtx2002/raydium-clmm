use crate::states::*;
use anchor_lang::prelude::*;
use anchor_spl::{
    token::{self, Token},
    token_2022::{
        self,
        spl_token_2022::{
            self,
            extension::{
                transfer_fee::{TransferFeeConfig, MAX_FEE_BASIS_POINTS},
                BaseStateWithExtensions, ExtensionType, StateWithExtensions,
            },
        },
    },
    token_interface::{Mint, TokenAccount},
};
use std::collections::HashSet;

use super::get_recent_epoch;

const MINT_WHITELIST: [&'static str; 4] = [
    "HVbpJAQGNpkgBaYBZQBR1t7yFdvaYVp2vCQQfKKEN4tM",
    "Crn4x1Y2HUKko7ox2EZMT6N2t2ZyH7eKtwkBGVnhEq1g",
    "FrBfWJ4qE5sCzKm3k3JaAtqZcXUh4LvJygDeketsrsH4",
    "2b1kV6DkPAnxd5ixfnxCpjxmKwqjjaYmCZfHsFu24GXo",
];

pub fn invoke_memo_instruction<'info>(
    memo_msg: &[u8],                  // The message to be attached as a memo.
    memo_program: AccountInfo<'info>, // Account info for the Memo program.
) -> solana_program::entrypoint::ProgramResult {
    // Step 1: Build the memo instruction.
    let ix = spl_memo::build_memo(memo_msg, &Vec::new());

    // Step 2: Create a list of accounts involved in the instruction.
    let accounts = vec![memo_program];

    // Step 3: Invoke the memo instruction using the Solana runtime.
    solana_program::program::invoke(&ix, &accounts[..])
}

pub fn transfer_from_user_to_pool_vault<'info>(
    signer: &Signer<'info>, // The user authorizing the transfer.
    from: &InterfaceAccount<'info, TokenAccount>, // The source account (user's token account).
    to_vault: &InterfaceAccount<'info, TokenAccount>, // The target pool vault (destination).
    mint: Option<Box<InterfaceAccount<'info, Mint>>>, // Optional mint account info for Token-2022 transfers.
    token_program: &AccountInfo<'info>,               // The SPL token program account.
    token_program_2022: Option<AccountInfo<'info>>,   // Optional SPL Token-2022 program account.
    amount: u64,                                      // Amount to transfer.
) -> Result<()> {
    if amount == 0 {
        return Ok(());
    }
    let mut token_program_info = token_program.to_account_info();
    let from_token_info = from.to_account_info();
    match (mint, token_program_2022) {
        // Case 1: Use Token-2022 for transfer if both mint and token_program_2022 are provided.
        (Some(mint), Some(token_program_2022)) => {
            if from_token_info.owner == token_program_2022.key {
                token_program_info = token_program_2022.to_account_info()
            }

            // Token-2022 transfer with checked transfer (ensuring decimals).
            token_2022::transfer_checked(
                CpiContext::new(
                    token_program_info,
                    token_2022::TransferChecked {
                        from: from_token_info,
                        to: to_vault.to_account_info(),
                        authority: signer.to_account_info(),
                        mint: mint.to_account_info(),
                    },
                ),
                amount,
                mint.decimals,
            )
        }
        // Case 2: Use standard SPL token transfer.
        _ => token::transfer(
            CpiContext::new(
                token_program_info,
                token::Transfer {
                    from: from_token_info,
                    to: to_vault.to_account_info(),
                    authority: signer.to_account_info(),
                },
            ),
            amount,
        ),
    }
}

pub fn transfer_from_pool_vault_to_user<'info>(
    pool_state_loader: &AccountLoader<'info, PoolState>, // The pool state loader containing authority info.
    from_vault: &InterfaceAccount<'info, TokenAccount>, // The vault account from which tokens are transferred.
    to: &InterfaceAccount<'info, TokenAccount>,         // The user's account where tokens are sent.
    mint: Option<Box<InterfaceAccount<'info, Mint>>>, // Optional mint account info for Token-2022 transfers.
    token_program: &AccountInfo<'info>,               // The SPL token program account.
    token_program_2022: Option<AccountInfo<'info>>,   // Optional SPL Token-2022 program account.
    amount: u64,                                      // Amount to transfer.
) -> Result<()> {
    if amount == 0 {
        return Ok(());
    }
    let mut token_program_info = token_program.to_account_info();
    let from_vault_info = from_vault.to_account_info();
    match (mint, token_program_2022) {
        // Case 1: Token-2022 transfer if both mint and token_program_2022 are provided.
        (Some(mint), Some(token_program_2022)) => {
            if from_vault_info.owner == token_program_2022.key {
                token_program_info = token_program_2022.to_account_info()
            }

            // Token-2022 transfer with checked transfer (ensuring decimals).
            token_2022::transfer_checked(
                CpiContext::new_with_signer(
                    token_program_info,
                    token_2022::TransferChecked {
                        from: from_vault_info,
                        to: to.to_account_info(),
                        authority: pool_state_loader.to_account_info(),
                        mint: mint.to_account_info(),
                    },
                    &[&pool_state_loader.load()?.seeds()],
                ),
                amount,
                mint.decimals,
            )
        }
        // Case 2: Standard SPL token transfer.
        _ => token::transfer(
            CpiContext::new_with_signer(
                token_program_info,
                token::Transfer {
                    from: from_vault_info,
                    to: to.to_account_info(),
                    authority: pool_state_loader.to_account_info(),
                },
                &[&pool_state_loader.load()?.seeds()],
            ),
            amount,
        ),
    }
}

pub fn close_spl_account<'a, 'b, 'c, 'info>(
    owner: &AccountInfo<'info>, // The owner or authority authorized to close the account.
    destination: &AccountInfo<'info>, // The destination account where the remaining balance is transferred.
    close_account: &InterfaceAccount<'info, TokenAccount>, // The token account to be closed.
    token_program: &Program<'info, Token>, // The SPL token program.
    // token_program_2022: &Program<'info, Token2022>,
    signers_seeds: &[&[&[u8]]], // Signer seeds for program-derived addresses.
) -> Result<()> {
    let token_program_info = token_program.to_account_info();
    let close_account_info = close_account.to_account_info();
    // if close_account_info.owner == token_program_2022.key {
    //     token_program_info = token_program_2022.to_account_info()
    // }

    token_2022::close_account(CpiContext::new_with_signer(
        token_program_info,
        token_2022::CloseAccount {
            account: close_account_info,
            destination: destination.to_account_info(),
            authority: owner.to_account_info(),
        },
        signers_seeds,
    ))
}

pub fn burn<'a, 'b, 'c, 'info>(
    owner: &Signer<'info>,
    mint: &InterfaceAccount<'info, Mint>,
    burn_account: &InterfaceAccount<'info, TokenAccount>,
    token_program: &Program<'info, Token>,
    // token_program_2022: &Program<'info, Token2022>,
    signers_seeds: &[&[&[u8]]],
    amount: u64,
) -> Result<()> {
    let mint_info = mint.to_account_info();
    let token_program_info: AccountInfo<'_> = token_program.to_account_info();
    // if mint_info.owner == token_program_2022.key {
    //     token_program_info = token_program_2022.to_account_info()
    // }
    token_2022::burn(
        CpiContext::new_with_signer(
            token_program_info,
            token_2022::Burn {
                mint: mint_info,
                from: burn_account.to_account_info(),
                authority: owner.to_account_info(),
            },
            signers_seeds,
        ),
        amount,
    )
}

/// Calculate the fee for output amount
pub fn get_transfer_inverse_fee(
    mint_account: Box<InterfaceAccount<Mint>>,
    post_fee_amount: u64,
) -> Result<u64> {
    let mint_info = mint_account.to_account_info();
    if *mint_info.owner == Token::id() {
        return Ok(0);
    }
    let mint_data = mint_info.try_borrow_data()?;
    let mint = StateWithExtensions::<spl_token_2022::state::Mint>::unpack(&mint_data)?;

    let fee = if let Ok(transfer_fee_config) = mint.get_extension::<TransferFeeConfig>() {
        let epoch = get_recent_epoch()?;

        let transfer_fee = transfer_fee_config.get_epoch_fee(epoch);
        if u16::from(transfer_fee.transfer_fee_basis_points) == MAX_FEE_BASIS_POINTS {
            u64::from(transfer_fee.maximum_fee)
        } else {
            transfer_fee_config
                .calculate_inverse_epoch_fee(epoch, post_fee_amount)
                .unwrap()
        }
    } else {
        0
    };
    Ok(fee)
}

/// Calculate the fee for input amount
pub fn get_transfer_fee(
    mint_account: Box<InterfaceAccount<Mint>>,
    pre_fee_amount: u64,
) -> Result<u64> {
    let mint_info = mint_account.to_account_info();
    if *mint_info.owner == Token::id() {
        return Ok(0);
    }
    let mint_data = mint_info.try_borrow_data()?;
    let mint = StateWithExtensions::<spl_token_2022::state::Mint>::unpack(&mint_data)?;

    let fee = if let Ok(transfer_fee_config) = mint.get_extension::<TransferFeeConfig>() {
        transfer_fee_config
            .calculate_epoch_fee(get_recent_epoch()?, pre_fee_amount)
            .unwrap()
    } else {
        0
    };
    Ok(fee)
}

pub fn is_supported_mint(mint_account: &InterfaceAccount<Mint>) -> Result<bool> {
    let mint_info = mint_account.to_account_info();
    if *mint_info.owner == Token::id() {
        return Ok(true);
    }
    let mint_whitelist: HashSet<&str> = MINT_WHITELIST.into_iter().collect();
    if mint_whitelist.contains(mint_account.key().to_string().as_str()) {
        return Ok(true);
    }
    let mint_data = mint_info.try_borrow_data()?;
    let mint = StateWithExtensions::<spl_token_2022::state::Mint>::unpack(&mint_data)?;
    let extensions = mint.get_extension_types()?;
    for e in extensions {
        if e != ExtensionType::TransferFeeConfig
            && e != ExtensionType::MetadataPointer
            && e != ExtensionType::TokenMetadata
        {
            return Ok(false);
        }
    }
    Ok(true)
}
