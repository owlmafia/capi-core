#[cfg(not(target_arch = "wasm32"))]
use crate::teal::save_rendered_teal;
use crate::{
    capi_asset::capi_asset_id::CapiAssetId,
    flows::create_project::storage::load_project::TxId,
    funds::FundsAssetId,
    teal::{render_template, TealSource, TealSourceTemplate},
};
use algonaut::{
    algod::v2::Algod,
    core::{Address, MicroAlgos, SuggestedTransactionParams},
    transaction::{
        contract_account::ContractAccount, tx_group::TxGroup, AcceptAsset, Pay, SignedTransaction,
        Transaction, TxnBuilder,
    },
};
use anyhow::Result;
use serde::Serialize;

// TODO no constants
// 2 assets (funds asset and capi asset)
pub const MIN_BALANCE: MicroAlgos = MicroAlgos(300_000);

pub async fn setup_capi_escrow(
    algod: &Algod,
    min_balance_sender: &Address,
    source: &TealSourceTemplate,
    params: &SuggestedTransactionParams,
    capi_asset_id: CapiAssetId,
    funds_asset_id: FundsAssetId,
) -> Result<SetupCentralEscrowToSign> {
    let escrow = render_and_compile_capi_escrow(algod, source).await?;

    let fund_min_balance_tx = &mut create_payment_tx(
        min_balance_sender,
        escrow.address(),
        // TODO use SDK calculated fee (* 2)
        MIN_BALANCE + params.fee.max(params.min_fee) * 2, // 2 txs signed by the escrow
        params,
    )
    .await?;

    let optin_to_capi_asset_tx = &mut TxnBuilder::with(
        params,
        AcceptAsset::new(*escrow.address(), capi_asset_id.0).build(),
    )
    .build()?;

    let optin_to_funds_asset_tx = &mut TxnBuilder::with(
        params,
        AcceptAsset::new(*escrow.address(), funds_asset_id.0).build(),
    )
    .build()?;

    TxGroup::assign_group_id(vec![
        fund_min_balance_tx,
        optin_to_capi_asset_tx,
        optin_to_funds_asset_tx,
    ])?;

    let optin_to_capi_asset_tx_signed = escrow.sign(&optin_to_capi_asset_tx, vec![])?;
    let optin_to_funds_asset_tx_signed = escrow.sign(&optin_to_funds_asset_tx, vec![])?;

    Ok(SetupCentralEscrowToSign {
        optin_to_capi_asset_tx: optin_to_capi_asset_tx_signed,
        optin_to_funds_asset_tx: optin_to_funds_asset_tx_signed,
        fund_min_balance_tx: fund_min_balance_tx.clone(),
        escrow,
    })
}

pub async fn render_and_compile_capi_escrow(
    algod: &Algod,
    source: &TealSourceTemplate,
) -> Result<ContractAccount> {
    let source = render_capi_escrow(source)?;
    Ok(ContractAccount::new(algod.compile_teal(&source.0).await?))
}

fn render_capi_escrow(source: &TealSourceTemplate) -> Result<TealSource> {
    let escrow_source = render_template(source, CapiEscrowTemplateContext {})?;
    #[cfg(not(target_arch = "wasm32"))]
    save_rendered_teal("capi_escrow", escrow_source.clone())?; // debugging
    Ok(escrow_source)
}

pub async fn submit_setup_capi_escrow(
    algod: &Algod,
    signed: &SetupCentralEscrowSigned,
) -> Result<TxId> {
    let res = algod
        .broadcast_signed_transactions(&[
            signed.fund_min_balance_tx.clone(),
            signed.optin_to_capi_asset_tx.clone(),
            signed.optin_to_funds_asset_tx.clone(),
        ])
        .await?;
    log::debug!("Payment tx id: {:?}", res.tx_id);
    Ok(res.tx_id.parse()?)
}

async fn create_payment_tx(
    sender: &Address,
    receiver: &Address,
    amount: MicroAlgos,
    params: &SuggestedTransactionParams,
) -> Result<Transaction> {
    let tx = &mut TxnBuilder::with(params, Pay::new(*sender, *receiver, amount).build()).build()?;
    Ok(tx.clone())
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SetupCentralEscrowToSign {
    pub optin_to_capi_asset_tx: SignedTransaction,
    pub optin_to_funds_asset_tx: SignedTransaction,
    pub fund_min_balance_tx: Transaction,
    pub escrow: ContractAccount,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SetupCentralEscrowSigned {
    pub optin_to_capi_asset_tx: SignedTransaction,
    pub optin_to_funds_asset_tx: SignedTransaction,
    pub fund_min_balance_tx: SignedTransaction,
}

#[derive(Serialize)]
struct CapiEscrowTemplateContext {}

#[derive(Serialize)]
struct SomeContext {
    address: String,
}