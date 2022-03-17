#[cfg(test)]
mod tests {
    use algonaut::{algod::v2::Algod, core::Address};
    use anyhow::Result;
    use serial_test::serial;
    use tokio::test;

    use crate::{
        flows::{
            create_dao::share_amount::ShareAmount,
            withdraw::withdraw::{submit_withdraw, withdraw, WithdrawSigned, WithdrawalInputs},
        },
        funds::{FundsAmount, FundsAssetId},
        state::account_state::funds_holdings,
        testing::{
            flow::{
                create_dao_flow::create_dao_flow,
                customer_payment_and_drain_flow::customer_payment_and_drain_flow,
                invest_in_dao_flow::{invests_flow, invests_optins_flow},
                withdraw_flow::{test::withdraw_flow, withdraw_precs},
            },
            network_test_util::test_dao_init,
            test_data::{dao_specs, investor2},
        },
    };

    #[test]
    #[serial]
    async fn test_withdraw_success() -> Result<()> {
        let td = &test_dao_init().await?;
        let algod = &td.algod;
        let drainer = &td.investor1;

        // precs

        let withdraw_amount = FundsAmount::new(1_000_000);

        let dao = create_dao_flow(&td).await?;
        let pay_and_drain_amount = FundsAmount::new(10 * 1_000_000);

        withdraw_precs(td, drainer, &dao.dao, pay_and_drain_amount).await?;

        // remeber state
        let central_balance_before_withdrawing =
            funds_holdings(&algod, dao.dao.central_escrow.address(), td.funds_asset_id).await?;
        let creator_balance_bafore_withdrawing =
            funds_holdings(&algod, &td.creator.address(), td.funds_asset_id).await?;

        // flow

        withdraw_flow(
            &algod,
            &dao.dao,
            &td.creator,
            withdraw_amount,
            td.funds_asset_id,
        )
        .await?;

        // test

        after_withdrawal_success_or_failure_tests(
            &algod,
            &td.creator.address(),
            td.funds_asset_id,
            dao.dao.central_escrow.address(),
            // creator got the amount
            creator_balance_bafore_withdrawing + withdraw_amount,
            // central lost the withdrawn amount
            central_balance_before_withdrawing - withdraw_amount,
        )
        .await
    }

    #[test]
    #[serial]
    async fn test_withdraw_without_enough_funds_fails() -> Result<()> {
        let td = &test_dao_init().await?;
        let algod = &td.algod;
        let investor = &td.investor1;

        // precs

        let dao_specs = dao_specs();
        let investor_share_amount = ShareAmount::new(10);

        let investment_amount = dao_specs.share_price * investor_share_amount.val();

        let withdraw_amount = investment_amount + FundsAmount::new(1); // > investment amount (which is in the funds when withdrawing)

        let dao = create_dao_flow(td).await?;

        // Investor buys some shares
        invests_optins_flow(algod, &investor, &dao.dao).await?;
        invests_flow(td, investor, investor_share_amount, &dao.dao, &dao.dao_id).await?;

        // remember state
        let central_balance_before_withdrawing =
            funds_holdings(&algod, dao.dao.central_escrow.address(), td.funds_asset_id).await?;
        let creator_balance_bafore_withdrawing =
            funds_holdings(algod, &td.creator.address(), td.funds_asset_id).await?;

        // flow

        let to_sign = withdraw(
            algod,
            td.creator.address(),
            td.funds_asset_id,
            &WithdrawalInputs {
                amount: withdraw_amount,
                description: "Withdrawing from tests".to_owned(),
            },
            &dao.dao.central_escrow,
        )
        .await?;

        let pay_withdraw_fee_tx_signed =
            td.creator.sign_transaction(to_sign.pay_withdraw_fee_tx)?;

        let withdraw_res = submit_withdraw(
            algod,
            &WithdrawSigned {
                withdraw_tx: to_sign.withdraw_tx,
                pay_withdraw_fee_tx: pay_withdraw_fee_tx_signed,
            },
        )
        .await;

        // test

        assert!(withdraw_res.is_err());

        test_withdrawal_did_not_succeed(
            algod,
            &td.creator.address(),
            td.funds_asset_id,
            dao.dao.central_escrow.address(),
            creator_balance_bafore_withdrawing,
            central_balance_before_withdrawing,
        )
        .await
    }

    // TODO: test is failing after removing governance - add creator check to central escrow
    #[test]
    #[serial]
    async fn test_withdraw_by_not_creator_fails() -> Result<()> {
        let td = &test_dao_init().await?;
        let algod = &td.algod;
        let drainer = &td.investor1;
        let investor = &td.investor2;
        let not_creator = &investor2();

        // precs

        let withdraw_amount = FundsAmount::new(1_000_000);

        let dao = create_dao_flow(&td).await?;
        let pay_and_drain_amount = FundsAmount::new(10 * 1_000_000);

        // customer payment and draining, to have some funds to withdraw
        customer_payment_and_drain_flow(td, &dao.dao, pay_and_drain_amount, drainer).await?;

        // Investor buys some shares
        let investor_share_amount = ShareAmount::new(10);
        invests_optins_flow(algod, investor, &dao.dao).await?;
        invests_flow(td, investor, investor_share_amount, &dao.dao, &dao.dao_id).await?;

        // remember state
        let central_balance_before_withdrawing =
            funds_holdings(algod, dao.dao.central_escrow.address(), td.funds_asset_id).await?;
        let creator_balance_bafore_withdrawing =
            funds_holdings(algod, &td.creator.address(), td.funds_asset_id).await?;

        // flow

        let to_sign = withdraw(
            algod,
            not_creator.address(),
            td.funds_asset_id,
            &WithdrawalInputs {
                amount: withdraw_amount,
                description: "Withdrawing from tests".to_owned(),
            },
            &dao.dao.central_escrow,
        )
        .await?;

        let pay_withdraw_fee_tx_signed =
            not_creator.sign_transaction(to_sign.pay_withdraw_fee_tx)?;

        let withdraw_res = submit_withdraw(
            algod,
            &WithdrawSigned {
                withdraw_tx: to_sign.withdraw_tx,
                pay_withdraw_fee_tx: pay_withdraw_fee_tx_signed,
            },
        )
        .await;

        // test

        assert!(withdraw_res.is_err());

        test_withdrawal_did_not_succeed(
            algod,
            &td.creator.address(),
            td.funds_asset_id,
            dao.dao.central_escrow.address(),
            creator_balance_bafore_withdrawing,
            central_balance_before_withdrawing,
        )
        .await
    }

    async fn test_withdrawal_did_not_succeed(
        algod: &Algod,
        creator_address: &Address,
        funds_asset_id: FundsAssetId,
        central_escrow_address: &Address,
        creator_balance_before_withdrawing: FundsAmount,
        central_balance_before_withdrawing: FundsAmount,
    ) -> Result<()> {
        after_withdrawal_success_or_failure_tests(
            algod,
            creator_address,
            funds_asset_id,
            central_escrow_address,
            creator_balance_before_withdrawing,
            central_balance_before_withdrawing,
        )
        .await
    }

    async fn after_withdrawal_success_or_failure_tests(
        algod: &Algod,
        creator_address: &Address,
        funds_asset_id: FundsAssetId,
        central_escrow_address: &Address,
        expected_withdrawer_amount: FundsAmount,
        expected_central_amount: FundsAmount,
    ) -> Result<()> {
        // check creator's balance
        let withdrawer_amount = funds_holdings(algod, creator_address, funds_asset_id).await?;
        assert_eq!(expected_withdrawer_amount, withdrawer_amount);

        // check central's balance
        let central_escrow_amount =
            funds_holdings(algod, central_escrow_address, funds_asset_id).await?;
        assert_eq!(expected_central_amount, central_escrow_amount);

        Ok(())
    }
}
