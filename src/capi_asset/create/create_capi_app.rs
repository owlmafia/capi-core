use algonaut::{
    algod::v2::Algod,
    core::{Address, SuggestedTransactionParams},
    transaction::{transaction::StateSchema, CreateApplication, Transaction, TxnBuilder},
};
use anyhow::Result;
use serde::Serialize;

#[cfg(not(target_arch = "wasm32"))]
use crate::teal::save_rendered_teal;
use crate::{
    capi_asset::capi_asset_id::CapiAssetAmount,
    teal::{render_template, TealSource, TealSourceTemplate},
};

/// Capi app: remembers total dividend retrieved (global) and already retrieved dividend (local), to prevent double harvesting.
#[allow(clippy::too_many_arguments)]
pub async fn create_app(
    algod: &Algod,
    approval_source: &TealSourceTemplate,
    clear_source: &TealSource,
    sender: &Address,
    asset_supply: CapiAssetAmount,
    precision: u64,
    params: &SuggestedTransactionParams,
) -> Result<Transaction> {
    log::debug!("Creating capi app");

    let approval_rendered = render_app(approval_source, asset_supply, precision)?;

    let compiled_approval_program = algod.compile_teal(&approval_rendered.0).await?;
    let compiled_clear_program = algod.compile_teal(&clear_source.0).await?;

    let tx = TxnBuilder::with(
        params,
        CreateApplication::new(
            *sender,
            compiled_approval_program.clone(),
            compiled_clear_program,
            StateSchema {
                number_ints: 1, // "total received"
                number_byteslices: 0,
            },
            StateSchema {
                number_ints: 2, // for investors: "shares", "already retrieved"
                number_byteslices: 0,
            },
        )
        .build(),
    )
    .build()?;

    Ok(tx)
}

pub fn render_app(
    source: &TealSourceTemplate,
    asset_supply: CapiAssetAmount,
    precision: u64,
) -> Result<TealSource> {
    let source = render_template(
        source,
        RenderCapiAppContext {
            asset_supply: asset_supply.0.to_string(),
            precision: precision.to_string(),
        },
    )?;
    #[cfg(not(target_arch = "wasm32"))]
    save_rendered_teal("app_capi_approval", source.clone())?; // debugging
    Ok(source)
}

#[derive(Serialize)]
struct RenderCapiAppContext {
    asset_supply: String,
    precision: String,
}

#[cfg(test)]
mod tests {
    use crate::{
        capi_asset::{capi_asset_id::CapiAssetAmount, create::create_capi_app::create_app},
        dependencies,
        network_util::wait_for_pending_transaction,
        teal::{load_teal, load_teal_template},
        testing::{network_test_util::test_init, test_data::creator, TESTS_DEFAULT_PRECISION},
    };
    use algonaut::{
        model::algod::v2::TealKeyValue,
        transaction::{transaction::StateSchema, Transaction, TransactionType},
    };
    use anyhow::{anyhow, Result};
    use serial_test::serial;
    use tokio::test;

    #[test]
    #[serial] // reset network (cmd)
    async fn test_create_app() -> Result<()> {
        test_init()?;

        // deps
        let algod = dependencies::algod_for_tests();
        let creator = creator();

        let approval_template = load_teal_template("app_capi_approval")?;
        let clear_source = load_teal("app_capi_clear")?;

        let params = algod.suggested_transaction_params().await?;

        // asset supply isn't used here so we can pass anything (0 in this case)
        let tx = create_app(
            &algod,
            &approval_template,
            &clear_source,
            &creator.address(),
            CapiAssetAmount::new(0),
            TESTS_DEFAULT_PRECISION,
            &params,
        )
        .await?;

        let signed_tx = creator.sign_transaction(&tx)?;
        let res = algod.broadcast_signed_transaction(&signed_tx).await?;

        log::debug!("Capi app created! tx id: {:?}", res.tx_id);
        let p_tx_opt = wait_for_pending_transaction(&algod, &res.tx_id.parse()?).await?;
        assert!(p_tx_opt.is_some());
        let p_tx = p_tx_opt.unwrap();
        assert!(p_tx.application_index.is_some());
        let p_tx_app_index = p_tx.application_index.unwrap();

        let creator_infos = algod.account_information(&creator.address()).await?;

        let apps = creator_infos.created_apps;
        assert_eq!(1, apps.len());

        let app = &apps[0];
        assert!(!app.params.approval_program.is_empty());
        assert!(!app.params.clear_state_program.is_empty());
        assert_eq!(creator.address(), app.params.creator);
        assert_eq!(Vec::<TealKeyValue>::new(), app.params.global_state);
        assert_eq!(p_tx_app_index, app.id); // just a general sanity check: id returning in pending tx is the same as in creator account
        assert!(app.params.global_state_schema.is_some());
        assert!(app.params.local_state_schema.is_some());

        // the repetition here wouldn't be needed if algonaut used the same types for transaction and algod::v2..
        let params_global_schema = app.params.global_state_schema.as_ref().unwrap();
        let params_local_schema = app.params.local_state_schema.as_ref().unwrap();
        assert_eq!(
            global_state_schema(&tx)?.unwrap().number_ints,
            params_global_schema.num_uint
        );
        assert_eq!(
            global_state_schema(&tx)?.unwrap().number_byteslices,
            params_global_schema.num_byte_slice
        );
        assert_eq!(
            local_state_schema(&tx)?.unwrap().number_ints,
            params_local_schema.num_uint
        );
        assert_eq!(
            local_state_schema(&tx)?.unwrap().number_byteslices,
            params_local_schema.num_byte_slice
        );
        Ok(())
    }

    fn global_state_schema(tx: &Transaction) -> Result<Option<StateSchema>> {
        match &tx.txn_type {
            TransactionType::ApplicationCallTransaction(c) => Ok(c.global_state_schema.to_owned()),
            _ => Err(anyhow!(
                "Invalid state: tx is expected to be an app call tx: {:?}",
                tx
            )),
        }
    }

    fn local_state_schema(tx: &Transaction) -> Result<Option<StateSchema>> {
        match &tx.txn_type {
            TransactionType::ApplicationCallTransaction(c) => Ok(c.local_state_schema.to_owned()),
            _ => Err(anyhow!(
                "Invalid state: tx is expected to be an app call tx: {:?}",
                tx
            )),
        }
    }
}