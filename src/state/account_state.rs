use algonaut::{
    algod::v2::Algod,
    core::Address,
    model::algod::v2::{Account, AssetHolding},
};
use anyhow::{anyhow, Result};
use mbase::models::{
    asset_amount::AssetAmount,
    funds::{FundsAmount, FundsAssetId},
    share_amount::ShareAmount,
};

/// Returns asset holdings. If not opted in, returns 0 holdings.
pub async fn asset_holdings(
    algod: &Algod,
    address: &Address,
    asset_id: u64,
) -> Result<AssetAmount> {
    asset_holdings_from_account(&algod.account_information(address).await?, asset_id)
}

/// Returns asset holdings. If not opted in, returns 0 holdings.
pub fn asset_holdings_from_account(account: &Account, asset_id: u64) -> Result<AssetAmount> {
    Ok(account
        .assets
        .iter()
        .find(|a| a.asset_id == asset_id)
        .map(|h| AssetAmount(h.amount))
        // asset id not found -> user not opted in -> 0 holdings
        // we don't differentiate here between not opted in or opted in with no holdings
        .unwrap_or(AssetAmount(0)))
}

/// Returns asset holdings if opted in, otherwise error
pub async fn asset_holdings_if_opted_in(
    algod: &Algod,
    address: &Address,
    asset_id: u64,
) -> Result<AssetAmount> {
    asset_holdings_from_account_if_opted_in(&algod.account_information(address).await?, asset_id)
}

/// Returns asset holdings if opted in, otherwise error
pub fn asset_holdings_from_account_if_opted_in(
    account: &Account,
    asset_id: u64,
) -> Result<AssetAmount> {
    account
        .assets
        .iter()
        .find(|a| a.asset_id == asset_id)
        .map(|h| AssetAmount(h.amount))
        .ok_or_else(|| anyhow!("Not opted in to asset: {asset_id}"))
}

pub async fn funds_holdings(
    algod: &Algod,
    address: &Address,
    asset_id: FundsAssetId,
) -> Result<FundsAmount> {
    Ok(FundsAmount(
        asset_holdings(algod, address, asset_id.0).await?,
    ))
}

pub fn funds_holdings_from_account(
    account: &Account,
    asset_id: FundsAssetId,
) -> Result<FundsAmount> {
    Ok(FundsAmount(asset_holdings_from_account(
        account, asset_id.0,
    )?))
}

pub fn find_asset_holding(holdings: &[AssetHolding], asset_id: u64) -> Option<AssetHolding> {
    holdings.iter().find(|a| a.asset_id == asset_id).cloned()
}

pub fn find_asset_holding_or_err(holdings: &[AssetHolding], asset_id: u64) -> Result<AssetHolding> {
    find_asset_holding(holdings, asset_id)
        .ok_or_else(|| anyhow!("Didn't find asset_id: {}", asset_id))
}

pub async fn share_holdings(
    algod: &Algod,
    address: &Address,
    share_asset_id: u64,
) -> Result<ShareAmount> {
    Ok(ShareAmount(
        asset_holdings(algod, address, share_asset_id).await?,
    ))
}
