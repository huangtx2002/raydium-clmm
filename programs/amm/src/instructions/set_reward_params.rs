use crate::error::ErrorCode;
use crate::libraries::{fixed_point_64, full_math::MulDiv, U256};
use crate::states::pool::{reward_period_limit, PoolState, REWARD_NUM};
use crate::states::*;
use crate::util::transfer_from_user_to_pool_vault;
use anchor_lang::prelude::*;
use anchor_spl::token::Token;
use anchor_spl::token_interface::{Mint, Token2022, TokenAccount};

#[derive(Accounts)]
pub struct SetRewardParams<'info> {
    /// Address to be set as protocol owner. It pays to create factory state account.
    pub authority: Signer<'info>,

    #[account(
        address = pool_state.load()?.amm_config
    )]
    pub amm_config: Account<'info, AmmConfig>,

    #[account(
        mut,
        constraint = pool_state.load()?.amm_config == amm_config.key()
    )]
    pub pool_state: AccountLoader<'info, PoolState>,

    /// load info from the account to judge reward permission
    #[account(
        seeds = [
            OPERATION_SEED.as_bytes(),
        ],
        bump,
    )]
    pub operation_state: AccountLoader<'info, OperationState>,

    /// Token program
    pub token_program: Program<'info, Token>,
    /// Token program 2022
    pub token_program_2022: Program<'info, Token2022>,
}

pub fn set_reward_params<'a, 'b, 'c: 'info, 'info>(
    ctx: Context<'a, 'b, 'c, 'info, SetRewardParams<'info>>,
    reward_index: u8,
    emissions_per_second_x64: u128,
    open_time: u64,
    end_time: u64,
) -> Result<()> {
    // Ensure the reward index is within the valid range
    assert!((reward_index as usize) < REWARD_NUM);

    // Ensure the end time is greater than the open time
    require_gt!(end_time, open_time);

    // Ensure the emissions per second is greater than zero
    require_gt!(emissions_per_second_x64, 0);

    // Load the operation state and check if the authority is an admin operator
    let operation_state = ctx.accounts.operation_state.load()?;
    let admin_keys = operation_state.operation_owners.to_vec();
    let admin_operator = admin_keys.contains(&ctx.accounts.authority.key())
        && ctx.accounts.authority.key() != Pubkey::default();

    // Get the current timestamp
    let current_timestamp = u64::try_from(Clock::get()?.unix_timestamp).unwrap();

    // Ensure the open time is greater than the current timestamp
    require_gt!(open_time, current_timestamp);

    // Load the mutable reference to the pool state
    let mut pool_state = ctx.accounts.pool_state.load_mut()?;

    // If the authority is not an admin operator, ensure they are the pool owner
    if !admin_operator {
        require_keys_eq!(ctx.accounts.authority.key(), pool_state.owner);
    }

    // Update the reward information in the pool state based on the current timestamp
    pool_state.update_reward_infos(current_timestamp)?;

    // Get the reward information for the specified reward index
    let mut reward_info = pool_state.reward_infos[reward_index as usize];

    // Ensure the reward information is initialized
    if !reward_info.initialized() {
        return err!(ErrorCode::UnInitializedRewardInfo);
    }

    // Calculate the reward amount based on whether the authority is an admin operator
    let reward_amount = if admin_operator {
        admin_update(
            &mut reward_info,
            current_timestamp,
            emissions_per_second_x64,
            open_time,
            end_time,
        )
        .unwrap()
    } else {
        if current_timestamp <= reward_info.open_time {
            return err!(ErrorCode::NotApproved);
        }
        normal_update(
            &mut reward_info,
            current_timestamp,
            emissions_per_second_x64,
            open_time,
            end_time,
        )
        .unwrap()
    };

    // Update the reward information in the pool state
    pool_state.reward_infos[reward_index as usize] = reward_info;

    // If the reward amount is greater than zero, transfer the reward tokens
    if reward_amount > 0 {
        let mut remaining_accounts = ctx.remaining_accounts.iter();

        // Get the reward token vault, authority token account, and reward vault mint
        let reward_token_vault =
            InterfaceAccount::<TokenAccount>::try_from(&remaining_accounts.next().unwrap())?;
        let authority_token_account =
            InterfaceAccount::<TokenAccount>::try_from(&remaining_accounts.next().unwrap())?;
        let reward_vault_mint =
            InterfaceAccount::<Mint>::try_from(&remaining_accounts.next().unwrap())?;

        // Ensure the mints and vault keys match
        require_keys_eq!(reward_token_vault.mint, authority_token_account.mint);
        require_keys_eq!(reward_token_vault.key(), reward_info.token_vault);

        // Transfer the reward tokens from the user to the pool vault
        // trasfer reward tokens from the pool owner, admin or the designated reward funder
        transfer_from_user_to_pool_vault(
            &ctx.accounts.authority,
            &authority_token_account,
            &reward_token_vault,
            Some(Box::new(reward_vault_mint)),
            &ctx.accounts.token_program,
            Some(ctx.accounts.token_program_2022.to_account_info()),
            reward_amount,
        )?;
    }

    Ok(())
}

fn normal_update(
    reward_info: &mut RewardInfo,
    current_timestamp: u64,
    emissions_per_second_x64: u128,
    open_time: u64,
    end_time: u64,
) -> Result<u64> {
    // Variable to store the calculated reward amount
    let mut reward_amount: u64;

    // Check if the reward emission has finished
    if reward_info.last_update_time == reward_info.end_time {
        // reward emission has finished
        let time_delta = end_time.checked_sub(open_time).unwrap();
        if time_delta < reward_period_limit::MIN_REWARD_PERIOD
            || time_delta > reward_period_limit::MAX_REWARD_PERIOD
        {
            return Err(ErrorCode::InvalidRewardPeriod.into());
        }

        // Calculate the reward amount for the new period
        reward_amount = U256::from(time_delta)
            .mul_div_ceil(
                U256::from(emissions_per_second_x64),
                U256::from(fixed_point_64::Q64),
            )
            .unwrap()
            .as_u64();

        // Update the reward information with the new parameters
        reward_info.open_time = open_time;
        reward_info.last_update_time = open_time;
        reward_info.end_time = end_time;
        reward_info.emissions_per_second_x64 = emissions_per_second_x64;
    } else {
        // reward emission does not finish
        // Reward emission is still ongoing
        let left_reward_time = reward_info.end_time.checked_sub(current_timestamp).unwrap();
        let extend_period = end_time.checked_sub(reward_info.end_time).unwrap();

        if extend_period < reward_period_limit::MIN_REWARD_PERIOD
            || extend_period > reward_period_limit::MAX_REWARD_PERIOD
        {
            return err!(ErrorCode::NotApproveUpdateRewardEmissiones);
        }

        // emissions_per_second_x64 must not smaller than before with in 72hrs
        if emissions_per_second_x64 < reward_info.emissions_per_second_x64 {
            require_gt!(
                reward_period_limit::INCREASE_EMISSIONES_PERIOD,
                left_reward_time
            );
        }

        // Calculate the difference in emissions per second
        let emission_diff_x64 =
            emissions_per_second_x64.saturating_sub(reward_info.emissions_per_second_x64);

        // Calculate the reward amount for the remaining time with the updated emissions rate
        reward_amount = U256::from(left_reward_time)
            .mul_div_floor(
                U256::from(emission_diff_x64),
                U256::from(fixed_point_64::Q64),
            )
            .unwrap()
            .as_u64();

        // Update the emissions rate
        reward_info.emissions_per_second_x64 = emissions_per_second_x64;

        // If the extend period is greater than zero, calculate the additional reward amount
        if extend_period > 0 {
            let reward_amount_diff = U256::from(extend_period)
                .mul_div_floor(
                    U256::from(reward_info.emissions_per_second_x64),
                    U256::from(fixed_point_64::Q64),
                )
                .unwrap()
                .as_u64();

            // Add the additional reward amount to the total reward amount
            reward_amount = reward_amount.checked_add(reward_amount_diff).unwrap();

            // Update the end time
            reward_info.end_time = end_time;
        }
    }

    // Return the calculated reward amount
    Ok(reward_amount)
}

fn admin_update(
    reward_info: &mut RewardInfo,
    current_timestamp: u64,
    emissions_per_second_x64: u128,
    open_time: u64,
    end_time: u64,
) -> Result<u64> {
    // Variable to store the calculated reward amount
    let mut reward_amount: u64;

    // Check if the reward emission has finished or if the reward has not yet started
    if reward_info.last_update_time == reward_info.end_time
        || reward_info.open_time > current_timestamp
    {
        // reward emission has finished
        let time_delta = end_time.checked_sub(open_time).unwrap();
        if time_delta == 0 {
            return Err(ErrorCode::InvalidRewardPeriod.into());
        }

        // Calculate the reward amount for the new period
        reward_amount = U256::from(time_delta)
            .mul_div_ceil(
                U256::from(emissions_per_second_x64),
                U256::from(fixed_point_64::Q64),
            )
            .unwrap()
            .as_u64();

        // Update the reward information with the new parameters
        reward_info.open_time = open_time;
        reward_info.last_update_time = open_time;
        reward_info.end_time = end_time;
        reward_info.emissions_per_second_x64 = emissions_per_second_x64;
    } else {
        // reward emission does not finish
        // Reward emission is still ongoing
        let left_reward_time = reward_info.end_time.checked_sub(current_timestamp).unwrap();
        let extend_period = end_time.saturating_sub(reward_info.end_time);

        // emissions_per_second_x64 can be update for admin during anytime
        // Calculate the difference in emissions per second
        let emission_diff_x64 =
            emissions_per_second_x64.saturating_sub(reward_info.emissions_per_second_x64);

        // Calculate the reward amount for the remaining time with the updated emissions rate
        reward_amount = U256::from(left_reward_time)
            .mul_div_floor(
                U256::from(emission_diff_x64),
                U256::from(fixed_point_64::Q64),
            )
            .unwrap()
            .as_u64();

        // Update the emissions rate
        reward_info.emissions_per_second_x64 = emissions_per_second_x64;

        // Calculate the additional reward amount for the extended period
        let reward_amount_diff = U256::from(extend_period)
            .mul_div_floor(
                U256::from(reward_info.emissions_per_second_x64),
                U256::from(fixed_point_64::Q64),
            )
            .unwrap()
            .as_u64();

        // Add the additional reward amount to the total reward amount
        reward_amount = reward_amount.checked_add(reward_amount_diff).unwrap();

        // Update the end time
        reward_info.end_time = end_time;
    }

    // Return the calculated reward amount
    Ok(reward_amount)
}
