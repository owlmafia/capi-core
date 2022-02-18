use super::{
    create_project_specs::CreateProjectSpecs,
    model::{CreateProjectSigned, CreateProjectToSign, SubmitCreateProjectResult},
};
use crate::{
    flows::create_project::{
        model::Project,
        setup::{
            central_escrow::setup_central_escrow, create_app::create_app_tx,
            customer_escrow::setup_customer_escrow, investing_escrow::setup_investing_escrow_txs,
            staking_escrow::setup_staking_escrow_txs,
        },
    },
    funds::FundsAssetId,
    network_util::wait_for_pending_transaction,
    teal::{TealSource, TealSourceTemplate},
};
use algonaut::{
    algod::v2::Algod,
    core::Address,
    transaction::{tx_group::TxGroup, SignedTransaction},
};
use anyhow::{anyhow, Result};

pub async fn create_project_txs(
    algod: &Algod,
    specs: &CreateProjectSpecs,
    creator: Address,
    shares_asset_id: u64,
    funds_asset_id: FundsAssetId,
    programs: Programs,
    precision: u64,
) -> Result<CreateProjectToSign> {
    log::debug!(
        "Creating project with specs: {:?}, shares_asset_id: {}, precision: {}",
        specs,
        shares_asset_id,
        precision
    );

    let params = algod.suggested_transaction_params().await?;

    let mut central_to_sign = setup_central_escrow(
        algod,
        &creator,
        &programs.escrows.central_escrow,
        &params,
        funds_asset_id,
    )
    .await?;

    let mut customer_to_sign = setup_customer_escrow(
        algod,
        &creator,
        central_to_sign.escrow.address(),
        &programs.escrows.customer_escrow,
        &params,
        funds_asset_id,
    )
    .await?;

    let mut create_app_tx = create_app_tx(
        algod,
        &programs.central_app_approval,
        programs.central_app_clear,
        &creator,
        specs.shares.supply,
        precision,
        specs.investors_part(),
        customer_to_sign.escrow.address(),
        central_to_sign.escrow.address(),
        &params,
    )
    .await?;

    // TODO why do we do this (invest and staking escrows setup) here instead of directly on project creation? there seem to be no deps on post-creation things?
    let mut setup_staking_escrow_to_sign = setup_staking_escrow_txs(
        algod,
        &programs.escrows.staking_escrow,
        shares_asset_id,
        &creator,
        &params,
    )
    .await?;
    let mut setup_invest_escrow_to_sign = setup_investing_escrow_txs(
        algod,
        &programs.escrows.invest_escrow,
        shares_asset_id,
        specs.shares.supply,
        &specs.share_price,
        &funds_asset_id,
        &creator,
        setup_staking_escrow_to_sign.escrow.address(),
        &params,
    )
    .await?;

    // First tx group to submit - everything except the asset (shares) xfer to the escrow (which requires opt-in to be submitted first)
    TxGroup::assign_group_id(vec![
        // app create (must be first in the group to return the app id, apparently)
        &mut create_app_tx,
        // funding
        &mut central_to_sign.fund_min_balance_tx,
        &mut customer_to_sign.fund_min_balance_tx,
        &mut setup_staking_escrow_to_sign.escrow_funding_algos_tx,
        &mut setup_invest_escrow_to_sign.escrow_funding_algos_tx,
        // asset (shares) opt-ins
        &mut setup_staking_escrow_to_sign.escrow_shares_optin_tx,
        &mut setup_invest_escrow_to_sign.escrow_shares_optin_tx,
        // asset (funds asset) opt-ins
        &mut central_to_sign.optin_to_funds_asset_tx,
        &mut customer_to_sign.optin_to_funds_asset_tx,
        // asset (shares) transfer to investing escrow
        &mut setup_invest_escrow_to_sign.escrow_funding_shares_asset_tx,
    ])?;

    // Now that the lsig txs have been assigned a group id, sign (by their respective programs)
    let staking_escrow = setup_staking_escrow_to_sign.escrow.clone();
    let staking_escrow_shares_optin_tx_signed =
        staking_escrow.sign(&setup_staking_escrow_to_sign.escrow_shares_optin_tx, vec![])?;
    let invest_escrow = setup_invest_escrow_to_sign.escrow.clone();
    let invest_escrow_shares_optin_tx_signed =
        invest_escrow.sign(&setup_invest_escrow_to_sign.escrow_shares_optin_tx, vec![])?;
    let central_escrow_optin_to_funds_asset_tx_signed = central_to_sign
        .escrow
        .sign(&central_to_sign.optin_to_funds_asset_tx, vec![])?;
    let customer_escrow_optin_to_funds_asset_tx_signed = customer_to_sign
        .escrow
        .sign(&customer_to_sign.optin_to_funds_asset_tx, vec![])?;
    let optin_txs = vec![
        staking_escrow_shares_optin_tx_signed,
        invest_escrow_shares_optin_tx_signed,
        central_escrow_optin_to_funds_asset_tx_signed,
        customer_escrow_optin_to_funds_asset_tx_signed,
    ];

    Ok(CreateProjectToSign {
        specs: specs.to_owned(),
        creator,

        staking_escrow: setup_staking_escrow_to_sign.escrow,
        invest_escrow: setup_invest_escrow_to_sign.escrow,
        central_escrow: central_to_sign.escrow,
        customer_escrow: customer_to_sign.escrow,

        // initial funding (algos), to be signed by creator
        escrow_funding_txs: vec![
            central_to_sign.fund_min_balance_tx,
            customer_to_sign.fund_min_balance_tx,
            setup_staking_escrow_to_sign.escrow_funding_algos_tx,
            setup_invest_escrow_to_sign.escrow_funding_algos_tx,
        ],
        optin_txs,
        create_app_tx,

        // xfers to escrows: have to be executed after escrows are opted in
        xfer_shares_to_invest_escrow: setup_invest_escrow_to_sign.escrow_funding_shares_asset_tx,
    })
}

pub async fn submit_create_project(
    algod: &Algod,
    signed: CreateProjectSigned,
) -> Result<SubmitCreateProjectResult> {
    // crate::debug_msg_pack_submit_par::log_to_msg_pack(&signed);
    log::debug!(
        "Submitting, created project specs: {:?}, creator: {:?}",
        signed.specs,
        signed.creator
    );

    let mut signed_txs = vec![signed.create_app_tx];
    for tx in signed.escrow_funding_txs {
        signed_txs.push(tx)
    }
    for tx in signed.optin_txs {
        signed_txs.push(tx)
    }
    signed_txs.push(signed.xfer_shares_to_invest_escrow);

    // crate::teal::debug_teal_rendered(&signed_txs, "app_central_approval").unwrap();
    // crate::teal::debug_teal_rendered(&signed_txs, "investing_escrow").unwrap();
    // crate::teal::debug_teal_rendered(&signed_txs, "staking_escrow").unwrap();

    let central_app_id = broadcast_txs_and_retrieve_app_id(algod, &signed_txs).await?;

    Ok(SubmitCreateProjectResult {
        project: Project {
            specs: signed.specs,
            shares_asset_id: signed.shares_asset_id,
            funds_asset_id: signed.funds_asset_id,
            central_app_id,
            invest_escrow: signed.invest_escrow,
            staking_escrow: signed.staking_escrow,
            customer_escrow: signed.customer_escrow,
            central_escrow: signed.central_escrow,
            creator: signed.creator,
        },
    })
}

async fn broadcast_txs_and_retrieve_app_id(
    algod: &Algod,
    txs: &[SignedTransaction],
) -> Result<u64> {
    // crate::teal::debug_teal_rendered(&txs, "app_central_approval").unwrap();
    // crate::teal::debug_teal_rendered(&txs, "investing_escrow").unwrap();
    // crate::teal::debug_teal_rendered(&txs, "staking_escrow").unwrap();

    let create_app_res = algod.broadcast_signed_transactions(txs).await?;
    let p_tx = wait_for_pending_transaction(algod, &create_app_res.tx_id.parse()?)
        .await?
        .ok_or_else(|| anyhow!("Couldn't get pending tx"))?;
    let central_app_id = p_tx
        .application_index
        .ok_or_else(|| anyhow!("Pending tx didn't have app id"))?;

    log::debug!("Central app created, id: {}", central_app_id);

    Ok(central_app_id)
}

pub struct Programs {
    pub central_app_approval: TealSourceTemplate,
    pub central_app_clear: TealSource,
    pub escrows: Escrows,
}

pub struct Escrows {
    pub central_escrow: TealSourceTemplate,
    pub customer_escrow: TealSourceTemplate,
    pub invest_escrow: TealSourceTemplate,
    pub staking_escrow: TealSourceTemplate,
}
