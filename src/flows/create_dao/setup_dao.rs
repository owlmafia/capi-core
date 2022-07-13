use super::{
    model::{SetupDaoSigned, SetupDaoToSign, SubmitSetupDaoResult},
    setup_dao_specs::SetupDaoSpecs,
};
use crate::{
    algo_helpers::wait_for_p_tx_with_id,
    common_txs::pay,
    flows::create_dao::{
        model::Dao,
        setup::setup_app::{setup_app_tx, DaoInitData},
    },
};
use algonaut::{
    algod::v2::Algod,
    core::{to_app_address, Address, MicroAlgos},
    model::algod::v2::PendingTransaction,
    transaction::{tx_group::TxGroup, TransferAsset, TxnBuilder},
};
use anyhow::{anyhow, Result};
use mbase::{
    api::version::VersionedTealSourceTemplate,
    models::{
        dao_app_id::DaoAppId,
        funds::{FundsAmount, FundsAssetId},
        nft::Nft,
    },
};

#[allow(clippy::too_many_arguments)]
pub async fn setup_dao_txs(
    algod: &Algod,
    specs: &SetupDaoSpecs,
    creator: Address,
    shares_asset_id: u64,
    funds_asset_id: FundsAssetId,
    programs: &Programs,
    precision: u64,
    app_id: DaoAppId,
    image_nft_url: Option<String>,
) -> Result<SetupDaoToSign> {
    log::debug!(
        "Creating dao with specs: {:?}, shares_asset_id: {}, precision: {}",
        specs,
        shares_asset_id,
        precision
    );

    let params = algod.suggested_transaction_params().await?;

    // The non-investor shares currently just stay in the creator's wallet
    let mut transfer_shares_to_app_tx = TxnBuilder::with(
        &params,
        TransferAsset::new(
            creator,
            shares_asset_id,
            specs.shares_for_investors().val(),
            app_id.address(),
        )
        .build(),
    )
    .build()?;

    let mut setup_app_tx = setup_app_tx(
        app_id,
        &creator,
        &params,
        &DaoInitData {
            app_approval_version: programs.central_app_approval.version,
            app_clear_version: programs.central_app_clear.version,
            shares_asset_id,
            funds_asset_id,
            project_name: specs.name.clone(),
            descr_hash: specs.descr_hash.clone(),
            share_price: specs.share_price,
            investors_share: specs.investors_share,
            image_hash: specs.image_hash.clone(),
            image_nft_url,
            social_media_url: specs.social_media_url.clone(),
            min_raise_target: specs.raise_min_target,
            min_raise_target_end_date: specs.raise_end_date,
        },
    )
    .await?;

    let app_address = to_app_address(app_id.0);
    // min balance to hold 3 assets (shares, funds asset, optional image nft)
    let mut fund_app_tx = pay(&params, &creator, &app_address, MicroAlgos(400_000))?;
    // pay the opt-in inner tx fees (shares, funds asset and optional create image nft) (arbitrarily with this tx - could be any other in this group)
    fund_app_tx.fee = fund_app_tx.fee * 4;

    TxGroup::assign_group_id(&mut [
        &mut fund_app_tx,
        &mut setup_app_tx,
        &mut transfer_shares_to_app_tx,
    ])?;

    Ok(SetupDaoToSign {
        specs: specs.to_owned(),
        creator,

        fund_app_tx,
        setup_app_tx,

        transfer_shares_to_app_tx,
    })
}

pub async fn submit_setup_dao(
    algod: &Algod,
    signed: SetupDaoSigned,
) -> Result<SubmitSetupDaoResult> {
    // crate::debug_msg_pack_submit_par::log_to_msg_pack(&signed);
    log::debug!(
        "Submitting dao setup, specs: {:?}, creator: {:?}",
        signed.specs,
        signed.creator,
    );

    let app_call_tx_id = signed.setup_app_tx.transaction.id()?;

    let signed_txs = vec![
        signed.app_funding_tx,
        signed.setup_app_tx,
        signed.transfer_shares_to_app_tx,
    ];

    // crate::dryrun_util::dryrun_all(algod, &signed_txs).await?;
    // mbase::teal::debug_teal_rendered(&signed_txs, "dao_app_approval").unwrap();

    let _ = algod
        .broadcast_signed_transactions(&signed_txs)
        .await?
        .tx_id;

    let p_tx = wait_for_p_tx_with_id(algod, &app_call_tx_id.parse()?).await?;
    let image_nft = to_nft(&p_tx, signed.image_url)?;

    Ok(SubmitSetupDaoResult {
        dao: Dao {
            shares_asset_id: signed.shares_asset_id,
            funds_asset_id: signed.funds_asset_id,
            app_id: signed.app_id,
            owner: signed.creator,

            name: signed.specs.name,
            descr_hash: signed.specs.descr_hash,
            token_name: signed.specs.shares.token_name,
            token_supply: signed.specs.shares.supply,
            investors_share: signed.specs.investors_share,
            share_price: signed.specs.share_price,
            image_hash: signed.specs.image_hash,
            image_nft,
            social_media_url: signed.specs.social_media_url,
            raise_end_date: signed.specs.raise_end_date,
            raise_min_target: signed.specs.raise_min_target,
            raised: FundsAmount::new(0), // dao is just being setup - nothing raised yet
        },
    })
}

/// creates nft (optional) instance with the created asset (in teal) from inner txs and optional url
/// if the state is inconsistent (e.g. there's no url but there's a created asset or vice versa) returns an error
/// assumes p_tx to be setup dao tx (which creates the nft asset via inner tx, if the optional nft url arg is set)
fn to_nft(p_tx: &PendingTransaction, url: Option<String>) -> Result<Option<Nft>> {
    let created_asset_ids: Vec<u64> = p_tx
        .inner_txs
        .iter()
        .filter_map(|tx| tx.asset_index)
        .collect();

    log::trace!("inner txs: {:?}", p_tx.inner_txs);
    log::trace!("created_asset_ids: {created_asset_ids:?}");

    let image_nft_asset_id = if let Some((created_asset_id, should_be_empty)) =
        created_asset_ids.split_first()
    {
        if !should_be_empty.is_empty() {
            return Err(anyhow!(
                    // in dao setup inner txs, we create only the image nft asset id
                    "Invalid state: inner txs in dao setup created more than one asset: {created_asset_ids:?}"
                ));
        }

        Some(*created_asset_id)
    } else {
        None
    };

    Ok(match (image_nft_asset_id, &url) {
        (Some(asset_id), Some(url)) => Some(Nft {
            asset_id,
            url: url.to_owned(),
        }),
        (None, None) => None,
        _ => {
            return Err(anyhow!(
                "Illegal state: image: both or none must be set: asset id: {image_nft_asset_id:?}, url: {url:?}"
            ))
        }
    })
}

#[derive(Debug)]
pub struct Programs {
    pub central_app_approval: VersionedTealSourceTemplate,
    pub central_app_clear: VersionedTealSourceTemplate,
}

// TODO remove
/// TEAL related to the capi token
#[derive(Debug)]
pub struct CapiPrograms {
    pub app_approval: VersionedTealSourceTemplate,
    pub app_clear: VersionedTealSourceTemplate,
}
