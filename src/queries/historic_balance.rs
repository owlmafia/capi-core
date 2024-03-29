use crate::{
    flows::withdraw::withdrawals::withdrawals, queries::received_payments::received_payments,
};
use algonaut::{algod::v2::Algod, indexer::v2::Indexer};
use anyhow::{anyhow, Result};
use chrono::{DateTime, Utc};
use mbase::models::{
    capi_deps::CapiAssetDaoDeps,
    dao_id::DaoId,
    funds::{FundsAmount, FundsAssetId},
};

/// The balance of an account at some date
/// it's determined by fetching all the transactions involving the address before date
/// this should be optimized (e.g. pagination), for now it's ok since at the beginning with the beta/MVP there will not be that much txs
///
/// If date is before the dao was created / had balance, the returned balance will be 0
#[allow(clippy::too_many_arguments)]
pub async fn historic_dao_funds_balance(
    algod: &Algod,
    indexer: &Indexer,
    funds_asset: FundsAssetId,
    dao_id: DaoId,
    capi_deps: &CapiAssetDaoDeps,
    date: DateTime<Utc>,
) -> Result<FundsAmount> {
    let dao_address = dao_id.0.address();
    log::debug!("DAO address: {:?}", dao_address);

    // let before_time_formatted = date.to_rfc3339();

    let received = received_payments(
        indexer,
        &dao_address,
        funds_asset,
        &Some(date),
        // &None, // debugging: fetch all
        &None,
        capi_deps,
    )
    .await?;
    let income: u64 = received.iter().map(|p| p.amount.val()).sum();

    let withdrawals = withdrawals(
        algod,
        indexer,
        dao_id,
        funds_asset,
        &Some(date),
        // &None, // debugging: fetch all
        &None,
    )
    .await?;
    let spending: u64 = withdrawals.iter().map(|p| p.amount.val()).sum();

    if spending > income {
        return Err(anyhow!("Illegal state: spending ({spending}) > income ({income}). The Algorand protocol doesn't allow overspending."));
    }

    // unchecked subtraction: we just checked that income > spending
    let balance = FundsAmount::new(income - spending);

    log::debug!("Income: {income}");
    log::debug!("Spending: {spending}");
    log::debug!("Balance: {balance:?}");

    Ok(balance)
}

#[cfg(test)]
mod tests {
    use crate::queries::historic_balance::historic_dao_funds_balance;
    use anyhow::Result;
    use chrono::Utc;
    use mbase::{
        dependencies::{algod, indexer},
        logger::init_logger,
        models::{
            capi_deps::{CapiAddress, CapiAssetDaoDeps},
            dao_app_id::DaoAppId,
            dao_id::DaoId,
            funds::FundsAssetId,
        },
    };
    use rust_decimal::Decimal;
    use std::{convert::TryInto, str::FromStr};
    use tokio::test;

    // wasm debugging: query balance for existing dao
    #[test]
    #[ignore]
    async fn query_balance() -> Result<()> {
        init_logger()?;

        let algod = algod();
        let indexer = indexer();

        // existing dao params
        let dao_id = DaoId(DaoAppId(35));
        let funds_asset = FundsAssetId(11);
        let capi_deps = &CapiAssetDaoDeps {
            escrow_percentage: Decimal::from_str("0.1").unwrap().try_into()?,
            address: CapiAddress("".parse().unwrap()),
        };

        let date = Utc::now();

        let balance =
            historic_dao_funds_balance(&algod, &indexer, funds_asset, dao_id, &capi_deps, date)
                .await?;
        println!("balance: {:?}", balance);

        Ok(())
    }
}
