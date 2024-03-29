use crate::flows::{create_dao::model::Dao, shared::app::optin_to_dao_app};
use algonaut::{
    algod::v2::Algod,
    core::Address,
    transaction::{SignedTransaction, Transaction},
};
use anyhow::Result;
use mbase::models::tx_id::TxId;

pub async fn invest_or_locking_app_optin_tx(
    algod: &Algod,
    dao: &Dao,
    investor: &Address,
) -> Result<Transaction> {
    let params = algod.suggested_transaction_params().await?;
    let central_optin_tx = optin_to_dao_app(&params, dao.app_id, *investor)?;
    Ok(central_optin_tx)
}

pub async fn submit_invest_or_locking_app_optin(
    algod: &Algod,
    signed: SignedTransaction,
) -> Result<TxId> {
    // mbase::teal::debug_teal_rendered(&signed, "dao_app_approval").unwrap();
    let res = algod.broadcast_signed_transaction(&signed).await?;
    log::debug!("Investor app optins tx id: {}", res.tx_id);
    res.tx_id.parse()
}
