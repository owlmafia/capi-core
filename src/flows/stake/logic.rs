use algonaut::{
    algod::v2::Algod,
    core::{Address, MicroAlgos, SuggestedTransactionParams},
    transaction::{
        account::ContractAccount, builder::CallApplication, tx_group::TxGroup, SignedTransaction,
        Transaction, TransferAsset, TxnBuilder,
    },
};
use anyhow::Result;

use crate::flows::invest::logic::withdrawal_slot_investor_setup_tx;

// TODO no constants
pub const MIN_BALANCE: MicroAlgos = MicroAlgos(100_000);
pub const FIXED_FEE: MicroAlgos = MicroAlgos(1_000);

/// Note that this is only for shares that have been bought in the market
/// The investing flow doesn't use this: there's an xfer from the investing account to the staking escrow in the investing tx group
pub async fn stake(
    algod: &Algod,
    investor: Address,
    share_count: u64,
    shares_asset_id: u64,
    central_app_id: u64,
    withdrawal_slot_ids: &[u64],
    staking_escrow: &ContractAccount,
) -> Result<StakeToSign> {
    let params = algod.suggested_transaction_params().await?;

    // Central app setup app call (init investor's local state)
    let mut app_call_tx = TxnBuilder::with(
        SuggestedTransactionParams {
            fee: FIXED_FEE,
            ..params.clone()
        },
        CallApplication::new(investor, central_app_id).build(),
    )
    .build();

    // Withdrawal apps setup txs
    let mut slot_setup_txs = vec![];
    for slot_id in withdrawal_slot_ids {
        slot_setup_txs.push(withdrawal_slot_investor_setup_tx(
            &params, *slot_id, investor,
        )?);
    }

    // Send investor's assets to staking escrow
    let mut shares_xfer_tx = TxnBuilder::with(
        SuggestedTransactionParams {
            fee: FIXED_FEE,
            ..params
        },
        TransferAsset::new(
            investor,
            shares_asset_id,
            share_count,
            staking_escrow.address,
        )
        .build(),
    )
    .build();

    let mut txs_for_group = vec![&mut app_call_tx, &mut shares_xfer_tx];
    txs_for_group.extend(slot_setup_txs.iter_mut().collect::<Vec<_>>());
    TxGroup::assign_group_id(txs_for_group)?;

    Ok(StakeToSign {
        central_app_call_setup_tx: app_call_tx.clone(),
        slot_setup_app_calls_txs: slot_setup_txs.clone(),
        shares_xfer_tx: shares_xfer_tx.clone(),
    })
}

pub async fn submit_stake(algod: &Algod, signed: StakeSigned) -> Result<String> {
    let mut txs = vec![
        signed.central_app_call_setup_tx.clone(),
        signed.shares_xfer_tx_signed.clone(),
    ];
    txs.extend(signed.slot_setup_app_calls_txs);
    // crate::teal::debug_teal_rendered(&txs, "withdrawal_slot_approval").unwrap();
    // crate::teal::debug_teal_rendered(&txs, "app_central_approval").unwrap();
    let res = algod.broadcast_signed_transactions(&txs).await?;
    log::debug!("Stake tx id: {:?}", res.tx_id);
    Ok(res.tx_id)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StakeToSign {
    pub central_app_call_setup_tx: Transaction,
    pub slot_setup_app_calls_txs: Vec<Transaction>,
    pub shares_xfer_tx: Transaction,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StakeSigned {
    pub central_app_call_setup_tx: SignedTransaction,
    pub slot_setup_app_calls_txs: Vec<SignedTransaction>,
    pub shares_xfer_tx_signed: SignedTransaction,
}

#[cfg(test)]
mod tests {
    use algonaut::{
        core::MicroAlgos,
        transaction::{AcceptAsset, TransferAsset, TxnBuilder},
    };
    use anyhow::Result;
    use serial_test::serial;
    use tokio::test;

    use crate::{
        app_state_util::app_local_state_or_err,
        central_app_logic::calculate_entitled_harvest,
        dependencies,
        flows::{
            invest::app_optins::{
                invest_or_staking_app_optins_txs, submit_invest_or_staking_app_optins,
            },
            stake::logic::FIXED_FEE,
        },
        network_util::wait_for_pending_transaction,
        testing::{
            flow::{
                create_project::create_project_flow,
                customer_payment_and_drain_flow::customer_payment_and_drain_flow,
                harvest::harvest_flow,
                invest_in_project::{invests_flow, invests_optins_flow},
                stake::stake_flow,
                unstake::unstake_flow,
            },
            network_test_util::reset_network,
            project_general::{
                check_investor_central_app_local_state,
                test_withdrawal_slot_local_state_initialized_correctly,
            },
            test_data::{self, creator, customer, investor1, investor2, project_specs},
        },
    };

    #[test]
    #[serial]
    async fn test_stake() -> Result<()> {
        reset_network()?;

        // deps

        let algod = dependencies::algod();
        let creator = creator();
        let investor1 = investor1();
        let investor2 = investor2();
        // repurposing creator as drainer here, as there are only 2 investor test accounts and we prefer them in a clean state for these tests
        let drainer = test_data::creator();
        let customer = customer();

        // UI

        let buy_asset_amount = 10;

        // precs

        let project = create_project_flow(&algod, &creator, &project_specs(), 3).await?;

        invests_optins_flow(&algod, &investor1, &project).await?;
        let _ = invests_flow(&algod, &investor1, buy_asset_amount, &project).await?;

        // drain (to generate dividend). note that investor doesn't reclaim it (doesn't seem relevant for this test)
        // (the draining itself may also not be relevant, just for a more realistic pre-trade scenario)
        let customer_payment_amount = MicroAlgos(10 * 1_000_000);
        let _ = customer_payment_and_drain_flow(
            &algod,
            &drainer,
            &customer,
            customer_payment_amount,
            &project,
        )
        .await?;

        // investor1 unstakes
        let traded_shares = buy_asset_amount;
        let unstake_tx_id = unstake_flow(&algod, &project, &investor1, traded_shares).await?;
        let _ = wait_for_pending_transaction(&algod, &unstake_tx_id).await?;

        // investor2 gets shares from investor1 externally
        // normally this will be a swap in a dex. could also be a gift or some other service

        // investor2 opts in to the asset (this is done in the external service, e.g. dex)
        let params = algod.suggested_transaction_params().await?;
        let shares_optin_tx = &mut TxnBuilder::with(
            params.clone(),
            AcceptAsset::new(investor2.address(), project.shares_asset_id).build(),
        )
        .build();
        let signed_shares_optin_tx = investor2.sign_transaction(shares_optin_tx)?;
        let res = algod
            .broadcast_signed_transaction(&signed_shares_optin_tx)
            .await?;
        let _ = wait_for_pending_transaction(&algod, &res.tx_id);

        // investor1 sends shares to investor2 (e.g. as part of atomic swap in a dex)
        let trade_tx = &mut TxnBuilder::with(
            params.clone(),
            TransferAsset::new(
                investor1.address(),
                project.shares_asset_id,
                traded_shares,
                investor2.address(),
            )
            .build(),
        )
        .build();
        let signed_trade_tx = investor1.sign_transaction(trade_tx)?;
        let res = algod.broadcast_signed_transaction(&signed_trade_tx).await?;
        let _ = wait_for_pending_transaction(&algod, &res.tx_id);

        // investor2 opts in to our app. this will be on our website.
        // TODO confirm: can't we opt in in the same group (accessing local state during opt in fails)?,
        // is there a way to avoid the investor confirming txs 2 times here?

        let app_optins_txs =
            invest_or_staking_app_optins_txs(&algod, &project, &investor2.address()).await?;
        // UI
        let mut app_optins_signed_txs = vec![];
        for optin_tx in app_optins_txs {
            app_optins_signed_txs.push(investor2.sign_transaction(&optin_tx)?);
        }
        let app_optins_tx_id =
            submit_invest_or_staking_app_optins(&algod, app_optins_signed_txs).await?;
        let _ = wait_for_pending_transaction(&algod, &app_optins_tx_id);

        // flow

        // investor2 stakes the acquired shares
        stake_flow(&algod, &project, &investor2, traded_shares).await?;

        // tests

        // investor2 lost staked assets
        let investor2_infos = algod.account_information(&investor2.address()).await?;
        let investor2_assets = investor2_infos.assets;
        assert_eq!(1, investor2_assets.len()); // opted in to shares
        assert_eq!(0, investor2_assets[0].amount);

        // already harvested local state initialized to entitled algos

        // the amount drained to the central (all income so far)
        let central_total_received = customer_payment_amount;
        let investor2_entitled_amount = calculate_entitled_harvest(
            central_total_received,
            project.specs.shares.count,
            traded_shares,
        );

        let central_app_local_state =
            app_local_state_or_err(&investor2_infos.apps_local_state, project.central_app_id)?;

        check_investor_central_app_local_state(
            central_app_local_state,
            project.central_app_id,
            // shares local state initialized to shares
            traded_shares,
            // harvested total is initialized to entitled amount
            investor2_entitled_amount,
        );
        // renaming for clarity
        let total_withdrawn_after_staking_setup_call = investor2_entitled_amount;

        // staking escrow got assets
        let staking_escrow_infos = algod
            .account_information(&project.staking_escrow.address)
            .await?;
        let staking_escrow_assets = staking_escrow_infos.assets;
        assert_eq!(1, staking_escrow_assets.len()); // opted in to shares
        assert_eq!(traded_shares, staking_escrow_assets[0].amount);

        // investor2 harvests: doesn't get anything, because there has not been new income (customer payments) since they bought the shares
        // the harvest amount is the smallest number possible, to show that we can't retrieve anything
        let harvest_flow_res = harvest_flow(&algod, &project, &investor2, MicroAlgos(1)).await;
        println!("Expected error harvesting: {:?}", harvest_flow_res);
        // If there's nothing to harvest, the smart contract fails (transfer amount > allowed)
        assert!(harvest_flow_res.is_err());

        // drain again to generate dividend and be able to harvest
        let customer_payment_amount_2 = MicroAlgos(10 * 1_000_000);
        let _ = customer_payment_and_drain_flow(
            &algod,
            &drainer,
            &customer,
            customer_payment_amount_2,
            &project,
        )
        .await?;

        // harvest again: this time there's something to harvest and we expect success

        // remember state
        let investor2_amount_before_harvest = algod
            .account_information(&investor2.address())
            .await?
            .amount;

        // we'll harvest the max possible amount
        let investor2_entitled_amount = calculate_entitled_harvest(
            customer_payment_amount_2,
            project.specs.shares.count,
            traded_shares,
        );
        println!(
            "Harvesting max possible amount (expected to succeed): {:?}",
            investor2_entitled_amount
        );
        let _ = harvest_flow(&algod, &project, &investor2, investor2_entitled_amount).await?;
        // just a rename to help with clarity a bit
        let expected_harvested_amount = investor2_entitled_amount;
        let investor2_infos = algod.account_information(&investor2.address()).await?;
        // the balance is increased with the harvest - fees for the harvesting txs (app call + pay for harvest tx fee + fee for this tx)
        assert_eq!(
            investor2_amount_before_harvest + expected_harvested_amount - FIXED_FEE * 3,
            investor2_infos.amount
        );

        let central_app_local_state =
            app_local_state_or_err(&investor2_infos.apps_local_state, project.central_app_id)?;

        // investor's harvested local state was updated:
        check_investor_central_app_local_state(
            central_app_local_state,
            project.central_app_id,
            // the shares haven't changed
            traded_shares,
            // the harvested total was updated:
            // initial (total_withdrawn_after_staking_setup_call: entitled amount when staking the shares) + just harvested
            total_withdrawn_after_staking_setup_call + expected_harvested_amount,
        );

        for slot_id in project.withdrawal_slot_ids {
            test_withdrawal_slot_local_state_initialized_correctly(
                &algod,
                &investor2.address(),
                slot_id,
            )
            .await?;
        }

        Ok(())
    }
}
