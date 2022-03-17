#[cfg(test)]
pub use test::{claim_flow, claim_precs, ClaimTestFlowRes, ClaimTestPrecsRes};

#[cfg(test)]
pub mod test {
    use crate::flows::create_dao::{model::Dao, share_amount::ShareAmount};
    use crate::funds::FundsAmount;
    use crate::state::account_state::funds_holdings;
    use crate::testing::flow::customer_payment_and_drain_flow::CustomerPaymentAndDrainFlowRes;
    use crate::testing::flow::invest_in_dao_flow::invests_optins_flow;
    use crate::{
        flows::claim::claim::{claim, submit_claim, ClaimSigned},
        network_util::wait_for_pending_transaction,
        testing::flow::{
            create_dao_flow::create_dao_flow,
            customer_payment_and_drain_flow::customer_payment_and_drain_flow,
            invest_in_dao_flow::invests_flow,
        },
        testing::network_test_util::TestDeps,
    };
    use algonaut::transaction::account::Account;
    use anyhow::Result;

    pub async fn claim_precs(
        td: &TestDeps,
        share_amount: ShareAmount,
        payment_and_drain_amount: FundsAmount,
        drainer: &Account,
        claimer: &Account,
    ) -> Result<ClaimTestPrecsRes> {
        let algod = &td.algod;

        let dao = create_dao_flow(&td).await?;

        // investor buys shares: this can be called after draining as well (without affecting test results)
        // the only order required for this is draining->claiming, obviously claiming has to be executed after draining (if it's to claim the drained funds)
        invests_optins_flow(algod, &claimer, &dao.dao).await?;
        let _ = invests_flow(&td, &claimer, share_amount, &dao.dao, &dao.dao_id).await?;

        // payment and draining
        let drain_res =
            customer_payment_and_drain_flow(td, &dao.dao, payment_and_drain_amount, &drainer)
                .await?;

        let central_escrow_balance_after_drain = funds_holdings(
            algod,
            drain_res.dao.central_escrow.address(),
            td.funds_asset_id,
        )
        .await?;

        // end precs

        Ok(ClaimTestPrecsRes {
            dao: dao.dao,
            central_escrow_balance_after_drain,
            drain_res,
        })
    }

    pub async fn claim_flow(
        td: &TestDeps,
        dao: &Dao,
        claimer: &Account,
        amount: FundsAmount,
    ) -> Result<ClaimTestFlowRes> {
        let algod = &td.algod;

        // remember state
        let claimer_balance_before_claiming =
            funds_holdings(algod, &claimer.address(), td.funds_asset_id).await?;

        let to_sign = claim(
            &algod,
            &claimer.address(),
            dao.central_app_id,
            td.funds_asset_id,
            amount,
            &dao.central_escrow,
        )
        .await?;

        let app_call_tx_signed = claimer.sign_transaction(to_sign.app_call_tx)?;

        let claim_tx_id = submit_claim(
            &algod,
            &ClaimSigned {
                app_call_tx_signed,
                claim_tx: to_sign.claim_tx,
            },
        )
        .await?;

        wait_for_pending_transaction(&algod, &claim_tx_id).await?;

        Ok(ClaimTestFlowRes {
            dao: dao.clone(),
            claimer_balance_before_claiming,
            claimed: amount.to_owned(),
            // drain_res: precs.drain_res.clone(),
        })
    }

    // Any data we want to return from the flow to the tests
    #[derive(Debug, Clone, PartialEq, Eq)]
    pub struct ClaimTestFlowRes {
        pub dao: Dao,
        pub claimer_balance_before_claiming: FundsAmount,
        pub claimed: FundsAmount,
    }

    // Any data we want to return from the flow to the tests
    #[derive(Debug, Clone, PartialEq, Eq)]
    pub struct ClaimTestPrecsRes {
        pub dao: Dao,
        pub central_escrow_balance_after_drain: FundsAmount,
        pub drain_res: CustomerPaymentAndDrainFlowRes,
    }
}
