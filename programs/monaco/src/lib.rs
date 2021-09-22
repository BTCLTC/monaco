use anchor_lang::prelude::*;
use anchor_lang::solana_program::program_pack::Pack;
use anchor_lending::cpi::{
    deposit_reserve_liquidity, redeem_reserve_collateral, DepositReserveLiquidity,
    RedeemReserveCollateral,
};
use anchor_spl::dex;
use anchor_spl::dex::serum_dex::instruction::SelfTradeBehavior;
use anchor_spl::dex::serum_dex::matching::{OrderType, Side as SerumSide};
use anchor_spl::dex::serum_dex::state::MarketState;
use anchor_spl::token::{self, Mint, TokenAccount};
use spl_token_lending::state::Reserve;
use std::num::NonZeroU64;

declare_id!("BuYep31Y9ahB7qYPnTXY8zPVr4m341WPknmKj7RjGnaD");

#[program]
pub mod monaco {
    use super::*;

    /// Deposits funds into solend reserve first, then makes corresponding DepositState account
    pub fn deposit(
        ctx: Context<Deposit>,
        nonce: u8,
        liquidity_amount: u64,
        schedule: DcaSchedule,
        dca_recipient: Pubkey,
    ) -> ProgramResult {
        // Make deposit into lending program
        let cpi_accounts = DepositReserveLiquidity {
            lending_program: ctx.accounts.lending_program.clone(),
            source_liquidity: ctx.accounts.source_liquidity.to_account_info().clone(),
            destination_collateral_account: ctx
                .accounts
                .destination_collateral
                .to_account_info()
                .clone(),
            reserve_account: ctx.accounts.reserve.clone(),
            reserve_collateral_mint: ctx.accounts.reserve_collateral_mint.clone(),
            reserve_liquidity_supply: ctx.accounts.reserve_liquidity_supply.clone(),
            lending_market_account: ctx.accounts.lending_market.clone(),
            lending_market_authority: ctx.accounts.lending_market_authority.clone(),
            transfer_authority: ctx.accounts.transfer_authority.clone(),
            clock: ctx.accounts.clock.to_account_info().clone(),
            token_program_id: ctx.accounts.token_program.clone(),
        };

        let user_authority = ctx.accounts.user_authority.clone();
        let reserve = ctx.accounts.reserve.clone();

        let pda_seeds = &[
            &user_authority.key.to_bytes()[..32],
            &reserve.key.to_bytes()[..32],
            &[nonce],
        ];
        let pda_signer = &[&pda_seeds[..]];
        let cpi_ctx = CpiContext::new_with_signer(
            ctx.accounts.lending_program.clone(),
            cpi_accounts,
            pda_signer,
        );
        deposit_reserve_liquidity(cpi_ctx, liquidity_amount)?;

        // Build deposit state account
        let deposit_state_account = &mut ctx.accounts.deposit;

        // Query collateral token account for new balance
        let collateral_amount =
            token::accessor::amount(&ctx.accounts.destination_collateral.to_account_info())?;
        // maybe run an error check for zero collateral token account balance

        deposit_state_account.user_authority = *ctx.accounts.user_authority.key;
        deposit_state_account.collateral_account_key =
            *ctx.accounts.destination_collateral.to_account_info().key;
        deposit_state_account.liquidity_amount = liquidity_amount;
        deposit_state_account.collateral_amount = collateral_amount;
        deposit_state_account.schedule = schedule;
        deposit_state_account.reserve_account = *ctx.accounts.reserve.key;
        deposit_state_account.dca_mint = *ctx.accounts.dca_mint.to_account_info().key;
        // dca_recipient should be the caller's ATA of the token they want to DCA into
        deposit_state_account.dca_recipient = dca_recipient;
        deposit_state_account.created_at = ctx.accounts.clock.unix_timestamp;
        deposit_state_account.counter = 0;
        deposit_state_account.nonce = nonce;
        deposit_state_account.ooa = None;

        Ok(())
    }

    /// Adds funds to an existing DepositState account. Requires user to supply the same
    /// destination_collateral_account (controlled by PDA)
    pub fn add_to_deposit(
        ctx: Context<AddToDeposit>,
        nonce: u8,
        liquidity_amount: u64,
    ) -> ProgramResult {
        let cpi_accounts = DepositReserveLiquidity {
            lending_program: ctx.accounts.lending_program.clone(),
            source_liquidity: ctx.accounts.source_liquidity.to_account_info().clone(),
            destination_collateral_account: ctx
                .accounts
                .destination_collateral
                .to_account_info()
                .clone(),
            reserve_account: ctx.accounts.reserve.clone(),
            reserve_collateral_mint: ctx.accounts.reserve_collateral_mint.clone(),
            reserve_liquidity_supply: ctx.accounts.reserve_liquidity_supply.clone(),
            lending_market_account: ctx.accounts.lending_market.clone(),
            lending_market_authority: ctx.accounts.lending_market_authority.clone(),
            transfer_authority: ctx.accounts.transfer_authority.clone(),
            clock: ctx.accounts.clock.to_account_info().clone(),
            token_program_id: ctx.accounts.token_program.clone(),
        };

        let user_authority = ctx.accounts.user_authority.clone();
        let reserve = ctx.accounts.reserve.clone();

        let pda_seeds = &[
            &user_authority.key.to_bytes()[..32],
            &reserve.key.to_bytes()[..32],
            &[nonce],
        ];
        let pda_signer = &[&pda_seeds[..]];

        let cpi_ctx = CpiContext::new_with_signer(
            ctx.accounts.lending_program.clone(),
            cpi_accounts,
            pda_signer,
        );

        // CPI to lending program instruction
        deposit_reserve_liquidity(cpi_ctx, liquidity_amount)?;

        let deposit_state = &mut ctx.accounts.deposit_state;
        deposit_state.liquidity_amount += liquidity_amount;
        // Query collateral token account for new balance
        let collateral_amount =
            token::accessor::amount(&ctx.accounts.destination_collateral.to_account_info())?;

        deposit_state.collateral_amount = collateral_amount;

        Ok(())
    }

    /// Privileged instruction for running DCA strat on a deposit account
    /// Admin is currently set to fee_receiver::ID
    #[access_control(validate_admin(&ctx))]
    pub fn run_dca_strategy<'info>(
        ctx: Context<'_, '_, '_, 'info, RunDcaStrategy<'info>>,
        nonce: u8,
        side: Side,
        min_expected_swap_amount: u64,
        // ooa is only supplied to set the ooa on deposit_state during first
        // DCA purchase
        ooa: Option<Pubkey>,
    ) -> ProgramResult {
        // Refresh reserve account
        // let refresh_cpi_accounts = RefreshReserve {
        //     reserve: ctx.accounts.reserve.clone(),
        //     pyth_reserve_liquidity_oracle: ctx.accounts.pyth_reserve_liquidity_oracle.clone(),
        //     switchboard_reserve_liquidity_oracle: ctx
        //         .accounts
        //         .switchboard_reserve_liquidity_oracle
        //         .clone(),
        //     clock: ctx.accounts.clock.clone(),
        // };
        // let refresh_cpi_ctx = CpiContext::new(ctx.accounts.solend.clone(), refresh_cpi_accounts);
        // refresh_reserve(refresh_cpi_ctx)?;

        // Calculating how much collateral to redeem from reserve
        let reserve_acct = &mut ctx.accounts.refreshed_reserve;
        let deposit_state = &mut ctx.accounts.deposit_state;
        let reserve: Reserve = Reserve::unpack(&reserve_acct.data.borrow())?;

        let liquidity_in_collateral = reserve
            .collateral_exchange_rate()?
            .collateral_to_liquidity(deposit_state.collateral_amount)?;

        let amount_to_redeem = reserve
            .collateral_exchange_rate()?
            .liquidity_to_collateral(liquidity_in_collateral - deposit_state.liquidity_amount)?;

        // Redeem reserve collateral
        let redeem_cpi_accounts = RedeemReserveCollateral {
            lending_program: ctx.accounts.lending_program.clone(),
            source_collateral: ctx.accounts.source_collateral.to_account_info().clone(),
            // This is the account that receives the liquidity, should be controlled by PDA authority
            destination_liquidity: ctx
                .accounts
                .market
                .destination_liquidity
                .to_account_info()
                .clone(),
            refreshed_reserve_account: ctx.accounts.refreshed_reserve.clone(),
            reserve_collateral_mint: ctx.accounts.reserve_collateral_mint.clone(),
            reserve_liquidity: ctx.accounts.reserve_liquidity.clone(),
            lending_market: ctx.accounts.lending_market.clone(),
            lending_market_authority: ctx.accounts.lending_market_authority.clone(),
            user_transfer_authority: ctx.accounts.transfer_authority.clone(),
            clock: ctx.accounts.clock.clone(),
            token_program_id: ctx.accounts.token_program_id.clone(),
        };

        let user_authority = ctx.accounts.user_authority.clone();
        let reserve_account = ctx.accounts.refreshed_reserve.clone();

        let pda_seeds = &[
            &user_authority.key.to_bytes()[..32],
            &reserve_account.key.to_bytes()[..32],
            &[nonce],
        ];
        let pda_signer = &[&pda_seeds[..]];

        let redeem_cpi_ctx = CpiContext::new_with_signer(
            ctx.accounts.lending_program.clone(),
            redeem_cpi_accounts,
            pda_signer,
        );
        redeem_reserve_collateral(redeem_cpi_ctx, amount_to_redeem)?;

        let (from_token, to_token) = match side {
            Side::Bid => (
                ctx.accounts.dca_recipient.to_account_info(),
                ctx.accounts.market.destination_liquidity.to_account_info(),
            ),
            Side::Ask => (
                ctx.accounts.market.destination_liquidity.to_account_info(),
                ctx.accounts.dca_recipient.to_account_info(),
            ),
        };

        // Token balances before the trade.
        let from_amount_before = token::accessor::amount(&from_token)?;
        let to_amount_before = token::accessor::amount(&to_token)?;

        // Initiate and settle Serum swap
        let orderbook: OrderbookClient<'info> = (&*ctx.accounts).into();
        match side {
            Side::Bid => orderbook.buy(amount_to_redeem, None)?,
            Side::Ask => orderbook.sell(amount_to_redeem, None)?,
        }
        orderbook.settle(None, &ctx.accounts.dca_recipient)?;

        // Token balances after the trade.
        let from_amount_after = token::accessor::amount(&from_token)?;
        let to_amount_after = token::accessor::amount(&to_token)?;

        //  Calculate the delta, i.e. the amount swapped.
        let from_amount = from_amount_before.checked_sub(from_amount_after).unwrap();
        let to_amount = to_amount_after.checked_sub(to_amount_before).unwrap();

        // Run safety checks on serum swap
        apply_risk_checks(DidSwap {
            authority: *ctx.accounts.transfer_authority.key,
            given_amount: amount_to_redeem,
            min_expected_swap_amount,
            from_amount,
            to_amount,
            spill_amount: 0,
            from_mint: token::accessor::mint(&from_token)?,
            to_mint: token::accessor::mint(&to_token)?,
            quote_mint: match side {
                Side::Bid => token::accessor::mint(&from_token)?,
                Side::Ask => token::accessor::mint(&to_token)?,
            },
        })?;

        let deposit_account = &mut ctx.accounts.deposit_state;
        deposit_account.counter += 1;
        // This should only be not None on the first DCA, which can be checked client side by
        // decoding the deposit_state account
        if ooa != None {
            deposit_account.ooa = ooa;
        }

        Ok(())
    }

    pub fn close_account(ctx: Context<CloseAccount>, nonce: u8) -> ProgramResult {
        let reserve_collateral = &mut ctx.accounts.source_collateral;
        let collateral_amount = token::accessor::amount(&reserve_collateral.to_account_info())?;

        let redeem_cpi_accounts = RedeemReserveCollateral {
            lending_program: ctx.accounts.lending_program.clone(),
            source_collateral: ctx.accounts.source_collateral.to_account_info().clone(),
            // This is the account that receives the liquidity, should be controlled by PDA authority
            destination_liquidity: ctx.accounts.liquidity_recipient.to_account_info().clone(),
            refreshed_reserve_account: ctx.accounts.refreshed_reserve.clone(),
            reserve_collateral_mint: ctx.accounts.reserve_collateral_mint.clone(),
            reserve_liquidity: ctx.accounts.reserve_liquidity.clone(),
            lending_market: ctx.accounts.lending_market.clone(),
            lending_market_authority: ctx.accounts.lending_market_authority.clone(),
            user_transfer_authority: ctx.accounts.transfer_authority.clone(),
            clock: ctx.accounts.clock.clone(),
            token_program_id: ctx.accounts.token_program_id.clone(),
        };

        let user_authority = ctx.accounts.user_authority.clone();
        let reserve_account = ctx.accounts.refreshed_reserve.clone();

        let pda_seeds = &[
            &user_authority.key.to_bytes()[..32],
            &reserve_account.key.to_bytes()[..32],
            &[nonce],
        ];
        let pda_signer = &[&pda_seeds[..]];

        let redeem_cpi_ctx = CpiContext::new_with_signer(
            ctx.accounts.lending_program.clone(),
            redeem_cpi_accounts,
            pda_signer,
        );
        redeem_reserve_collateral(redeem_cpi_ctx, collateral_amount)?;

        Ok(())
    }
}

#[derive(Accounts)]
#[instruction(nonce: u8, liquidity_amount: u64, _bump: u8)]
pub struct Deposit<'info> {
    // Deposit state account
    #[account(init, payer = user_authority)]
    pub deposit: Account<'info, DepositState>,

    // AccountInfo of the account that calls the ix
    #[account(signer)]
    pub user_authority: AccountInfo<'info>,

    // Solend, Jet, or Port program
    pub lending_program: AccountInfo<'info>,

    // Token mint of DCA receiving asset
    pub dca_mint: Account<'info, Mint>,

    // Solend CPI accounts
    // Token account for asset to deposit into reserve and make sure account owner is transfer authority PDA
    #[account(
        constraint = source_liquidity.amount >= liquidity_amount,
        constraint = source_liquidity.owner == *transfer_authority.key
    )]
    pub source_liquidity: Account<'info, TokenAccount>,
    // Token account for reserve collateral token
    // Make sure it has a 0 balance to ensure empty account and make sure account owner is transfer authority PDA
    #[account(
        constraint = destination_collateral.amount == 0,
        constraint = destination_collateral.owner == *transfer_authority.key,
    )]
    pub destination_collateral: Account<'info, TokenAccount>,
    // Reserve state account
    pub reserve: AccountInfo<'info>,
    // Token mint for reserve collateral token
    pub reserve_collateral_mint: AccountInfo<'info>,
    // Reserve liquidity supply SPL token account
    pub reserve_liquidity_supply: AccountInfo<'info>,
    // Lending market account
    pub lending_market: AccountInfo<'info>,
    // Lending market authority (PDA)
    pub lending_market_authority: AccountInfo<'info>,
    // Transfer authority for source_liquidity and desitnation_collateral accounts
    #[account(seeds = [&user_authority.key.to_bytes()[..32], &reserve.key.to_bytes()[..32], &[nonce]], bump = _bump)]
    pub transfer_authority: AccountInfo<'info>,

    pub system_program: Program<'info, System>,
    // Clock
    pub clock: Sysvar<'info, Clock>,
    // Token program
    #[account(constraint = token_program.key == &token::ID)]
    pub token_program: AccountInfo<'info>,
}

#[derive(Accounts)]
#[instruction(nonce: u8, liquidity_amount: u64, _bump: u8)]
pub struct AddToDeposit<'info> {
    // Deposit state being modified
    // has_one ensures only the creator of the deposit_state account can add to it
    #[account(mut, has_one=user_authority)]
    pub deposit_state: ProgramAccount<'info, DepositState>,

    // Account calling the instruction
    #[account(signer)]
    pub user_authority: AccountInfo<'info>,

    // Solend, Jet, or Port program
    pub lending_program: AccountInfo<'info>,

    // Solend CPI accounts
    // Token account for asset to deposit into reserve and make sure account owner is transfer authority PDA
    #[account(
        constraint = source_liquidity.amount >= liquidity_amount,
        constraint = source_liquidity.owner == *transfer_authority.key
    )]
    pub source_liquidity: Account<'info, TokenAccount>,
    // Token account for reserve collateral token
    // Make sure account owner is transfer authority PDA
    #[account(
        constraint = destination_collateral.owner == *transfer_authority.key,
        // Destination collateral account should be deterministically derived for consistency - needs to be the same
        // across all deposits to a deposit state account
        constraint = *destination_collateral.to_account_info().key == deposit_state.collateral_account_key
    )]
    pub destination_collateral: Account<'info, TokenAccount>,
    // Reserve state account
    pub reserve: AccountInfo<'info>,
    // Token mint for reserve collateral token
    pub reserve_collateral_mint: AccountInfo<'info>,
    // Reserve liquidity supply SPL token account
    pub reserve_liquidity_supply: AccountInfo<'info>,
    // Lending market account
    pub lending_market: AccountInfo<'info>,
    // Lending market authority (PDA)
    pub lending_market_authority: AccountInfo<'info>,
    // Transfer authority for accounts 1 and 2
    #[account(seeds = [&user_authority.key.to_bytes()[..32], &reserve.key.to_bytes()[..32], &[nonce]], bump = _bump)]
    pub transfer_authority: AccountInfo<'info>,
    // Clock
    pub clock: Sysvar<'info, Clock>,
    // Token program
    #[account(constraint = token_program.key == &token::ID)]
    pub token_program: AccountInfo<'info>,
}

#[derive(Accounts)]
#[instruction(nonce: u8, _bump: u8)]
pub struct RunDcaStrategy<'info> {
    // Deposite state account being modified
    #[account(mut)]
    pub deposit_state: ProgramAccount<'info, DepositState>,

    // Account calling the instruction
    #[account(signer)]
    pub user_authority: AccountInfo<'info>,

    // Solend, Jet, or Port program
    pub lending_program: AccountInfo<'info>,

    // Solana CPI accounts for RefreshReserve and RedeemReserveCollateral

    // Refresh reserve accounts
    // Reserve account
    // pub reserve: AccountInfo<'info>,
    // // Pyth reserve liquidity oracle
    // // Must be the pyth price account specified in InitReserve
    // pub pyth_reserve_liquidity_oracle: AccountInfo<'info>,
    // // Switchboard Reserve liquidity oracle account
    // // Must be the switchboard price account specified in InitReserve
    // pub switchboard_reserve_liquidity_oracle: AccountInfo<'info>,

    // RedeeemReserveCollateral accounts
    // Source token account for reserve collateral token
    #[account(constraint = source_collateral.to_account_info().key == transfer_authority.key)]
    pub source_collateral: Account<'info, TokenAccount>,
    #[account(
        mut,
        constraint = *serum_recipient.to_account_info().key == deposit_state.dca_recipient
    )]
    pub serum_recipient: Account<'info, TokenAccount>,
    // Refreshed reserve account
    // #[account(constraint = refreshed_reserve.key == reserve.key)]
    pub refreshed_reserve: AccountInfo<'info>,
    // Reserve collateral mint account
    pub reserve_collateral_mint: AccountInfo<'info>,
    // Reserve liquidity supply SPL Token account.
    pub reserve_liquidity: AccountInfo<'info>,
    // Lending market account
    pub lending_market: AccountInfo<'info>,
    // Lending market authority - PDA
    pub lending_market_authority: AccountInfo<'info>,
    // User transfer authority
    #[account(seeds = [&user_authority.key.to_bytes()[..32], &refreshed_reserve.key.to_bytes()[..32], &[nonce]], bump = _bump)]
    pub transfer_authority: AccountInfo<'info>,

    // Serum swap accounts
    market: MarketAccounts<'info>,
    // DELET DIS
    #[account(mut)]
    dca_recipient: Account<'info, TokenAccount>,
    // Programs.
    dex_program: AccountInfo<'info>,

    // Misc accounts - Leave at AccountInfo
    pub clock: AccountInfo<'info>,
    pub rent: AccountInfo<'info>,
    pub token_program_id: AccountInfo<'info>,
}

impl<'info> From<&RunDcaStrategy<'info>> for OrderbookClient<'info> {
    fn from(accounts: &RunDcaStrategy<'info>) -> OrderbookClient<'info> {
        OrderbookClient {
            market: accounts.market.clone(),
            authority: accounts.transfer_authority.clone(),
            // pc_wallet: accounts.dca_recipient.to_account_info().clone(),
            dex_program: accounts.dex_program.clone(),
            token_program: accounts.token_program_id.clone(),
            rent: accounts.rent.clone(),
        }
    }
}

#[derive(Accounts)]
#[instruction(nonce: u8, _bump: u8)]
pub struct CloseAccount<'info> {
    #[account(
        mut,
        close = user_authority,
        constraint = deposit_state.user_authority == *user_authority.key
    )]
    pub deposit_state: Account<'info, DepositState>,

    #[account(signer)]
    pub user_authority: AccountInfo<'info>,

    pub liquidity_recipient: Account<'info, TokenAccount>,

    // Solend, Jet, or Port program
    pub lending_program: AccountInfo<'info>,

    // RedeeemReserveCollateral accounts
    // Source token account for reserve collateral token
    #[account(constraint = source_collateral.to_account_info().key == transfer_authority.key)]
    pub source_collateral: Account<'info, TokenAccount>,
    #[account(
        mut,
        constraint = *serum_recipient.to_account_info().key == deposit_state.dca_recipient
    )]
    pub serum_recipient: Account<'info, TokenAccount>,
    // Refreshed reserve account
    // #[account(constraint = refreshed_reserve.key == reserve.key)]
    pub refreshed_reserve: AccountInfo<'info>,
    // Reserve collateral mint account
    pub reserve_collateral_mint: AccountInfo<'info>,
    // Reserve liquidity supply SPL Token account.
    pub reserve_liquidity: AccountInfo<'info>,
    // Lending market account
    pub lending_market: AccountInfo<'info>,
    // Lending market authority - PDA
    pub lending_market_authority: AccountInfo<'info>,
    // User transfer authority
    #[account(seeds = [&user_authority.key.to_bytes()[..32], &refreshed_reserve.key.to_bytes()[..32], &[nonce]], bump = _bump)]
    pub transfer_authority: AccountInfo<'info>,

    pub clock: AccountInfo<'info>,
    pub rent: AccountInfo<'info>,
    pub token_program_id: AccountInfo<'info>,
}

#[account]
#[derive(Default)]
pub struct DepositState {
    // Pubkey of depositor that called ix
    pub user_authority: Pubkey,
    // Pubkey of account holding the reserve collateral token
    // Used for AddToDeposit context struct constraints
    pub collateral_account_key: Pubkey,
    // Current amount of liquidity tokens deposited
    // Update on withdraw or modification
    pub liquidity_amount: u64,
    // Current amount of reserve collateral tokens being controlled by PDA
    // Update on withdraw or modification
    pub collateral_amount: u64,
    // DCA schedule for deposit
    pub schedule: DcaSchedule,
    // Pubkey of reserve account of pool where liquidity is deposited
    pub reserve_account: Pubkey,
    // Token mint of token to run dca strategy on
    pub dca_mint: Pubkey,
    // Set this as ATA of signer
    pub dca_recipient: Pubkey,
    // OOA Pubkey
    pub ooa: Option<Pubkey>,

    // Unix timestamp of deposit
    pub created_at: i64,
    // Integer representing the amount of times a DCA has executed
    pub counter: u16,
    // Nonce
    pub nonce: u8,
}

// Market accounts are the accounts used to place orders against the dex minus
// common accounts, i.e., program ids, sysvars, and the `pc_wallet`.
#[derive(Accounts, Clone)]
pub struct MarketAccounts<'info> {
    #[account(mut)]
    market: AccountInfo<'info>,
    // User supplied OOA
    #[account(mut)]
    open_orders: AccountInfo<'info>,
    #[account(mut)]
    request_queue: AccountInfo<'info>,
    #[account(mut)]
    event_queue: AccountInfo<'info>,
    #[account(mut)]
    bids: AccountInfo<'info>,
    #[account(mut)]
    asks: AccountInfo<'info>,
    // The `spl_token::Account` that funds will be taken from, i.e., transferred
    // from the user into the market's vault.
    //
    // For bids, this is the base currency. For asks, the quote.
    #[account(mut)]
    order_payer_token_account: AccountInfo<'info>,
    // Also known as the "base" currency. For a given A/B market,
    // this is the vault for the A mint.
    #[account(mut)]
    coin_vault: AccountInfo<'info>,
    // Also known as the "quote" currency. For a given A/B market,
    // this is the vault for the B mint.
    #[account(mut)]
    pc_vault: AccountInfo<'info>,
    // PDA owner of the DEX's token accounts for base + quote currencies.
    vault_signer: AccountInfo<'info>,
    // User wallets.
    #[account(mut)]
    destination_liquidity: Account<'info, TokenAccount>,
}

// Client for sending orders to the Serum DEX.
struct OrderbookClient<'info> {
    market: MarketAccounts<'info>,
    authority: AccountInfo<'info>,
    // pc_wallet: AccountInfo<'info>,
    dex_program: AccountInfo<'info>,
    token_program: AccountInfo<'info>,
    rent: AccountInfo<'info>,
}

impl<'info> OrderbookClient<'info> {
    // Executes the sell order portion of the swap, purchasing as much of the
    // quote currency as possible for the given `base_amount`.
    //
    // `base_amount` is the "native" amount of the base currency, i.e., token
    // amount including decimals.
    fn sell(&self, base_amount: u64, referral: Option<AccountInfo<'info>>) -> ProgramResult {
        let limit_price = 1;
        let max_coin_qty = {
            // The loaded market must be dropped before CPI.
            let market = MarketState::load(&self.market.market, &dex::ID)?;
            coin_lots(&market, base_amount)
        };
        let max_native_pc_qty = u64::MAX;
        self.order_cpi(
            limit_price,
            max_coin_qty,
            max_native_pc_qty,
            Side::Ask,
            referral,
        )
    }

    // Executes the buy order portion of the swap, purchasing as much of the
    // base currency as possible, for the given `quote_amount`.
    //
    // `quote_amount` is the "native" amount of the quote currency, i.e., token
    // amount including decimals.
    fn buy(&self, quote_amount: u64, referral: Option<AccountInfo<'info>>) -> ProgramResult {
        let limit_price = u64::MAX;
        let max_coin_qty = u64::MAX;
        let max_native_pc_qty = quote_amount;
        self.order_cpi(
            limit_price,
            max_coin_qty,
            max_native_pc_qty,
            Side::Bid,
            referral,
        )
    }

    // Executes a new order on the serum dex via CPI.
    //
    // * `limit_price` - the limit order price in lot units.
    // * `max_coin_qty`- the max number of the base currency lot units.
    // * `max_native_pc_qty` - the max number of quote currency in native token
    //                         units (includes decimals).
    // * `side` - bid or ask, i.e. the type of order.
    // * `referral` - referral account, earning a fee.
    fn order_cpi(
        &self,
        limit_price: u64,
        max_coin_qty: u64,
        max_native_pc_qty: u64,
        side: Side,
        referral: Option<AccountInfo<'info>>,
    ) -> ProgramResult {
        // Client order id is only used for cancels. Not used here so hardcode.
        let client_order_id = 0;
        // Limit is the dex's custom compute budge parameter, setting an upper
        // bound on the number of matching cycles the program can perform
        // before giving up and posting the remaining unmatched order.
        let limit = 65535;

        let dex_accs = dex::NewOrderV3 {
            market: self.market.market.clone(),
            open_orders: self.market.open_orders.clone(),
            request_queue: self.market.request_queue.clone(),
            event_queue: self.market.event_queue.clone(),
            market_bids: self.market.bids.clone(),
            market_asks: self.market.asks.clone(),
            order_payer_token_account: self.market.order_payer_token_account.clone(),
            open_orders_authority: self.authority.clone(),
            coin_vault: self.market.coin_vault.clone(),
            pc_vault: self.market.pc_vault.clone(),
            token_program: self.token_program.clone(),
            rent: self.rent.clone(),
        };
        let mut ctx = CpiContext::new(self.dex_program.clone(), dex_accs);
        if let Some(referral) = referral {
            ctx = ctx.with_remaining_accounts(vec![referral]);
        }
        dex::new_order_v3(
            ctx,
            side.into(),
            NonZeroU64::new(limit_price).unwrap(),
            NonZeroU64::new(max_coin_qty).unwrap(),
            NonZeroU64::new(max_native_pc_qty).unwrap(),
            SelfTradeBehavior::DecrementTake,
            OrderType::ImmediateOrCancel,
            client_order_id,
            limit,
        )
    }

    fn settle(
        &self,
        referral: Option<AccountInfo<'info>>,
        quote_wallet: &Account<'info, TokenAccount>,
    ) -> ProgramResult {
        let settle_accs = dex::SettleFunds {
            market: self.market.market.clone(),
            open_orders: self.market.open_orders.clone(),
            open_orders_authority: self.authority.clone(),
            coin_vault: self.market.coin_vault.clone(),
            pc_vault: self.market.pc_vault.clone(),
            coin_wallet: self.market.destination_liquidity.to_account_info().clone(),
            pc_wallet: quote_wallet.to_account_info().clone(),
            vault_signer: self.market.vault_signer.clone(),
            token_program: self.token_program.clone(),
        };
        let mut ctx = CpiContext::new(self.dex_program.clone(), settle_accs);
        if let Some(referral) = referral {
            ctx = ctx.with_remaining_accounts(vec![referral]);
        }
        dex::settle_funds(ctx)
    }
}

// Utility functions

/// Derive the pubkey of the PDA meant to be in control of the source liquidity token account
/// and reserve collateral destination account
// fn derive_deposit_authority(
//     user: &AccountInfo,
//     reserve: &AccountInfo,
//     program_id: &Pubkey,
//     nonce: u8,
// ) -> Result<Pubkey> {
//     Pubkey::create_program_address(
//         &[
//             &user.key.to_bytes()[..32],    // Signer
//             &reserve.key.to_bytes()[..32], // Solend reserve account
//             &[nonce],                      // Nonce - usually 0
//         ],
//         program_id,
//     )
//     .or(Err(ErrorCode::InvalidDerivedAuthority.into()))
// }

fn validate_admin(ctx: &Context<RunDcaStrategy>) -> ProgramResult {
    if *ctx.accounts.user_authority.key != fee_recipient::ID {
        return Err(ErrorCode::InvalidAdmin.into());
    }
    Ok(())
}

#[derive(Clone, AnchorDeserialize, AnchorSerialize)]
pub enum DcaSchedule {
    Daily,
    Weekly,
    Biweekly,
    Monthly,
    Quarterly,
}

// Returns the amount of lots for the base currency of a trade with `size`.
fn coin_lots(market: &MarketState, size: u64) -> u64 {
    size.checked_div(market.coin_lot_size).unwrap()
}

// Asserts the swap event is valid.
fn apply_risk_checks(event: DidSwap) -> Result<()> {
    // Reject if the resulting amount is less than the client's expectation.
    if event.to_amount < event.min_expected_swap_amount {
        return Err(ErrorCode::SlippageExceeded.into());
    }
    emit!(event);
    Ok(())
}

#[derive(AnchorSerialize, AnchorDeserialize)]
pub enum Side {
    Bid,
    Ask,
}

impl From<Side> for SerumSide {
    fn from(side: Side) -> SerumSide {
        match side {
            Side::Bid => SerumSide::Bid,
            Side::Ask => SerumSide::Ask,
        }
    }
}

#[error]
pub enum ErrorCode {
    #[msg("Template Error")]
    InvalidSomething,
    #[msg("Invalid authority derivation")]
    InvalidDerivedAuthority,
    #[msg("The tokens being swapped must have different mints")]
    SwapTokensCannotMatch,
    #[msg("Slippage tolerance exceeded")]
    SlippageExceeded,
    #[msg("Privileged instruction called by incorrect admin")]
    InvalidAdmin,
    #[msg("Collateral account is already empty")]
    CollateralAccountIsEmpty,
}

// Event emitted when a swap occurs for two base currencies on two different
// markets (quoted in the same token).
#[event]
pub struct DidSwap {
    // User given (max) amount to swap.
    pub given_amount: u64,
    // The minimum amount of the *to* token expected to be received from
    // executing the swap.
    pub min_expected_swap_amount: u64,
    // Amount of the `from` token sold.
    pub from_amount: u64,
    // Amount of the `to` token purchased.
    pub to_amount: u64,
    // Amount of the quote currency accumulated from the swap.
    pub spill_amount: u64,
    // Mint sold.
    pub from_mint: Pubkey,
    // Mint purchased.
    pub to_mint: Pubkey,
    // Mint of the token used as the quote currency in the two markets used
    // for swapping.
    pub quote_mint: Pubkey,
    // User that signed the transaction.
    pub authority: Pubkey,
}

pub mod fee_recipient {
    solana_program::declare_id!("rohanrAYfWTd7DtNHVtoJFxdLYspwToEr55BqFdfkZd");
}
