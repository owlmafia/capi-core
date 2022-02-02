use algonaut::{
    algod::v2::Algod,
    core::{Address, SuggestedTransactionParams},
    transaction::{CreateAsset, SignedTransaction, Transaction, TxnBuilder},
};
use anyhow::{anyhow, Result};

use crate::{
    flows::create_project::model::{CreateSharesSpecs, CreateSharesToSign},
    network_util::wait_for_pending_transaction,
};

pub async fn create_investor_assets_txs(
    algod: &Algod,
    creator: &Address,
    specs: &CreateSharesSpecs,
) -> Result<CreateSharesToSign> {
    let params = algod.suggested_transaction_params().await?;

    let create_shares_tx = create_shares_tx(&params, specs, *creator).await?;

    Ok(CreateSharesToSign { create_shares_tx })
}

pub async fn submit_create_assets(
    algod: &Algod,
    create_shares: &SignedTransaction,
) -> Result<CreateAssetsResult> {
    let create_shares_tx_res = algod.broadcast_signed_transaction(create_shares).await?;

    let shares_asset_id = wait_for_pending_transaction(algod, &create_shares_tx_res.tx_id.parse()?)
        .await?
        .ok_or_else(|| anyhow!("No pending tx to retrieve shares asset id"))?
        .asset_index
        .ok_or_else(|| anyhow!("Shares asset id in pending tx not set"))?;

    Ok(CreateAssetsResult {
        shares_id: shares_asset_id,
    })
}

#[derive(Debug)]
pub struct CreateAssetsResult {
    pub shares_id: u64,
}

async fn create_shares_tx(
    tx_params: &SuggestedTransactionParams,
    config: &CreateSharesSpecs,
    creator: Address,
) -> Result<Transaction> {
    let unit_and_asset_name = config.token_name.to_owned();
    Ok(TxnBuilder::with(
        tx_params.clone(),
        CreateAsset::new(creator, config.count, 0, false)
            .unit_name(unit_and_asset_name.clone())
            .asset_name(unit_and_asset_name)
            .build(),
    )
    .build())
}
