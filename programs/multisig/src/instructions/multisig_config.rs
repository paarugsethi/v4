use anchor_lang::prelude::*;

use crate::errors::*;
use crate::state::*;

#[derive(AnchorSerialize, AnchorDeserialize)]
pub struct MultisigAddMemberArgs {
    new_member: Member,
    /// Memo isn't used for anything, but is included in `AddMemberEvent` that can later be parsed and indexed.
    pub memo: Option<String>,
}

#[derive(AnchorSerialize, AnchorDeserialize)]
pub struct MultisigRemoveMemberArgs {
    old_member: Pubkey,
    /// Memo isn't used for anything, but is included in `RemoveMemberEvent` that can later be parsed and indexed.
    pub memo: Option<String>,
}

#[derive(AnchorSerialize, AnchorDeserialize)]
pub struct MultisigChangeThresholdArgs {
    new_threshold: u16,
    /// Memo isn't used for anything, but is included in `ChangeThreshold` that can later be parsed and indexed.
    pub memo: Option<String>,
}

#[derive(AnchorSerialize, AnchorDeserialize)]
pub struct MultisigSetTimeLockArgs {
    time_lock: u32,
    /// Memo isn't used for anything, but is included in `ChangeThreshold` that can later be parsed and indexed.
    pub memo: Option<String>,
}

#[derive(AnchorSerialize, AnchorDeserialize)]
pub struct MultisigSetConfigAuthorityArgs {
    config_authority: Pubkey,
    /// Memo isn't used for anything, but is included in `ChangeThreshold` that can later be parsed and indexed.
    pub memo: Option<String>,
}

#[derive(AnchorSerialize, AnchorDeserialize)]
pub struct MultisigAddVaultArgs {
    /// The next vault index to set as the latest used.
    /// Must be the current `vault_index + 1`.
    /// We pass it explicitly to make this instruction idempotent.
    vault_index: u8,
    /// Memo isn't used for anything, but is included in `ChangeThreshold` that can later be parsed and indexed.
    pub memo: Option<String>,
}

#[derive(Accounts)]
pub struct MultisigConfig<'info> {
    #[account(
        mut,
        seeds = [SEED_PREFIX, SEED_MULTISIG, multisig.create_key.as_ref()],
        bump = multisig.bump,
    )]
    multisig: Account<'info, Multisig>,

    /// Multisig `config_authority` that must authorize the configuration change.
    pub config_authority: Signer<'info>,

    /// The account that will be charged in case the multisig account needs to reallocate space,
    /// for example when adding a new member.
    /// This is usually the same as `config_authority`, but can be a different account if needed.
    #[account(mut)]
    pub rent_payer: Option<Signer<'info>>,

    /// We might need it in case reallocation is needed.
    pub system_program: Option<Program<'info, System>>,
}

impl MultisigConfig<'_> {
    fn validate(&self) -> Result<()> {
        require_keys_eq!(
            self.config_authority.key(),
            self.multisig.config_authority,
            MultisigError::Unauthorized
        );

        Ok(())
    }

    /// Add a member/key to the multisig and reallocate space if necessary.
    ///
    /// NOTE: This instruction must be called only by the `config_authority` if one is set (Controlled Multisig).
    ///       Uncontrolled Mustisigs should use `config_transaction_create` instead.
    #[access_control(ctx.accounts.validate())]
    pub fn multisig_add_member(ctx: Context<Self>, args: MultisigAddMemberArgs) -> Result<()> {
        let MultisigAddMemberArgs { new_member, .. } = args;

        let system_program = &ctx
            .accounts
            .system_program
            .as_ref()
            .ok_or(MultisigError::MissingAccount)?;
        let rent_payer = &ctx
            .accounts
            .rent_payer
            .as_ref()
            .ok_or(MultisigError::MissingAccount)?;
        let multisig = &mut ctx.accounts.multisig;

        // Check if we need to reallocate space.
        let reallocated = Multisig::realloc_if_needed(
            multisig.to_account_info(),
            multisig.members.len() + 1,
            rent_payer.to_account_info(),
            system_program.to_account_info(),
        )?;

        if reallocated {
            multisig.reload()?;
        }

        multisig.add_member(new_member);

        multisig.invariant()?;

        multisig.config_updated();

        Ok(())
    }

    /// Remove a member/key from the multisig.
    ///
    /// NOTE: This instruction must be called only by the `config_authority` if one is set (Controlled Multisig).
    ///       Uncontrolled Mustisigs should use `config_transaction_create` instead.
    #[access_control(ctx.accounts.validate())]
    pub fn multisig_remove_member(
        ctx: Context<Self>,
        args: MultisigRemoveMemberArgs,
    ) -> Result<()> {
        let multisig = &mut ctx.accounts.multisig;

        require!(multisig.members.len() > 1, MultisigError::RemoveLastMember);

        multisig.remove_member(args.old_member)?;

        // Update the threshold if necessary.
        if usize::from(multisig.threshold) > multisig.members.len() {
            multisig.threshold = multisig
                .members
                .len()
                .try_into()
                .expect("didn't expect more that `u16::MAX` members");
        };

        multisig.invariant()?;

        multisig.config_updated();

        Ok(())
    }

    /// NOTE: This instruction must be called only by the `config_authority` if one is set (Controlled Multisig).
    ///       Uncontrolled Mustisigs should use `config_transaction_create` instead.
    #[access_control(ctx.accounts.validate())]
    pub fn multisig_change_threshold(
        ctx: Context<Self>,
        args: MultisigChangeThresholdArgs,
    ) -> Result<()> {
        let MultisigChangeThresholdArgs { new_threshold, .. } = args;

        let multisig = &mut ctx.accounts.multisig;

        multisig.threshold = new_threshold;

        multisig.invariant()?;

        multisig.config_updated();

        Ok(())
    }

    /// Set the `time_lock` config parameter for the multisig.
    ///
    /// NOTE: This instruction must be called only by the `config_authority` if one is set (Controlled Multisig).
    ///       Uncontrolled Mustisigs should use `config_transaction_create` instead.
    #[access_control(ctx.accounts.validate())]
    pub fn multisig_set_time_lock(ctx: Context<Self>, args: MultisigSetTimeLockArgs) -> Result<()> {
        let multisig = &mut ctx.accounts.multisig;

        multisig.time_lock = args.time_lock;

        multisig.invariant()?;

        multisig.config_updated();

        Ok(())
    }

    /// Set the multisig `config_authority`.
    ///
    /// NOTE: This instruction must be called only by the `config_authority` if one is set (Controlled Multisig).
    ///       Uncontrolled Mustisigs should use `config_transaction_create` instead.
    #[access_control(ctx.accounts.validate())]
    pub fn multisig_set_config_authority(
        ctx: Context<Self>,
        args: MultisigSetConfigAuthorityArgs,
    ) -> Result<()> {
        let multisig = &mut ctx.accounts.multisig;

        multisig.config_authority = args.config_authority;

        multisig.invariant()?;

        multisig.config_updated();

        Ok(())
    }

    /// Increment the multisig `vault_index`.
    /// This doesn't actually "add" a new vault, because vaults are derived from the multisig address and index, so technically
    /// they always exist. This just increments the index so that UIs can show the "used" vaults.
    ///
    /// NOTE: This instruction must be called only by the `config_authority` if one is set (Controlled Multisig).
    ///       Uncontrolled Mustisigs should use `config_transaction_create` instead.
    #[access_control(ctx.accounts.validate())]
    pub fn multisig_add_vault(ctx: Context<Self>, args: MultisigAddVaultArgs) -> Result<()> {
        let multisig = &mut ctx.accounts.multisig;

        require!(
            args.vault_index == multisig.vault_index + 1,
            MultisigError::InvalidVaultIndex
        );

        multisig.vault_index = args.vault_index;

        multisig.invariant()?;

        // NOTE: we don't call `multisig.config_updated` here, because this doesn't
        // affect the transactions by any means, and really just a UI feature.

        Ok(())
    }
}
