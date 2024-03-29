use algonaut::{
    algod::v2::Algod,
    core::{Address, CompiledTeal, SuggestedTransactionParams},
    transaction::{transaction::StateSchema, CreateApplication, Transaction, TxnBuilder},
};
use anyhow::{anyhow, Result};
use mbase::{
    api::version::VersionedTealSourceTemplate,
    models::{
        capi_deps::{CapiAddress, CapiAssetDaoDeps},
        funds::FundsAmount,
        share_amount::ShareAmount,
        shares_percentage::SharesPercentage,
    },
    state::dao_app_state::{
        GLOBAL_SCHEMA_NUM_BYTE_SLICES, GLOBAL_SCHEMA_NUM_INTS, LOCAL_SCHEMA_NUM_BYTE_SLICES,
        LOCAL_SCHEMA_NUM_INTS,
    },
    teal::{render_template_new, TealSource, TealSourceTemplate},
    util::decimal_util::AsDecimal,
};
use rust_decimal::prelude::ToPrimitive;
use serde::Serialize;

#[cfg(not(target_arch = "wasm32"))]
use mbase::teal::save_rendered_teal;

#[allow(clippy::too_many_arguments)]
pub async fn create_app_tx(
    algod: &Algod,
    approval_template: &VersionedTealSourceTemplate,
    clear_template: &VersionedTealSourceTemplate,
    creator: &Address,
    share_supply: ShareAmount,
    precision: u64,
    investors_share: SharesPercentage,
    params: &SuggestedTransactionParams,
    capi_deps: &CapiAssetDaoDeps,
    share_price: FundsAmount,
    // checked in teal (it's guaranteed that no more can be raised)
    // expected to be determined by regulations, normally
    max_raisable_amount: FundsAmount,
) -> Result<Transaction> {
    log::debug!("Creating central app with asset supply: {}", share_supply);

    let compiled_approval_program = render_and_compile_app_approval(
        algod,
        approval_template,
        share_supply,
        precision,
        investors_share,
        &capi_deps.address,
        capi_deps.escrow_percentage,
        share_price,
        max_raisable_amount,
    )
    .await?;
    let compiled_clear_program = render_and_compile_app_clear(algod, clear_template).await?;

    let tx = TxnBuilder::with(
        params,
        CreateApplication::new(
            *creator,
            compiled_approval_program.clone(),
            compiled_clear_program,
            StateSchema {
                number_ints: GLOBAL_SCHEMA_NUM_INTS,
                number_byteslices: GLOBAL_SCHEMA_NUM_BYTE_SLICES,
            },
            StateSchema {
                number_ints: LOCAL_SCHEMA_NUM_INTS,
                number_byteslices: LOCAL_SCHEMA_NUM_BYTE_SLICES,
            },
        )
        .extra_pages(1)
        .build(),
    )
    .build()?;

    Ok(tx)
}

#[allow(clippy::too_many_arguments)]
pub async fn render_and_compile_app_approval(
    algod: &Algod,
    template: &VersionedTealSourceTemplate,
    share_supply: ShareAmount,
    precision: u64,
    investors_share: SharesPercentage,
    capi_address: &CapiAddress,
    capi_percentage: SharesPercentage,
    share_price: FundsAmount,
    max_raisable_amount: FundsAmount,
) -> Result<CompiledTeal> {
    let source = match template.version.0 {
        1 => render_central_app_approval_v1(
            &template.template,
            share_supply,
            precision,
            investors_share,
            capi_address,
            capi_percentage,
            share_price,
            max_raisable_amount,
        ),
        _ => Err(anyhow!(
            "Dao app approval version not supported: {:?}",
            template.version
        )),
    }?;

    Ok(algod.compile_teal(&source.0).await?)
}

#[allow(clippy::too_many_arguments)]
pub fn render_central_app_approval_v1(
    source: &TealSourceTemplate,
    share_supply: ShareAmount,
    precision: u64,
    investors_share: SharesPercentage,
    capi_address: &CapiAddress,
    capi_percentage: SharesPercentage,
    share_price: FundsAmount,
    max_raisable_amount: FundsAmount,
) -> Result<TealSource> {
    let precision_square = precision
        .checked_pow(2)
        .ok_or_else(|| anyhow!("Precision squared overflow: {}", precision))?;

    // TODO write tests that catch incorrect/variable supply - previously it was hardcoded to 100 and everything was passing
    let investors_share_percentage = (investors_share.value() * precision.as_decimal().floor())
        .to_u64()
        .ok_or_else(|| anyhow!("Unexpected: couldn't convert decimal to u64"))?;

    let capi_share = (capi_percentage
        .value()
        .checked_mul(precision.as_decimal())
        .ok_or_else(|| anyhow!("Precision squared overflow: {}", precision))?)
    .floor();

    let source = render_template_new(
        source,
        &[
            ("TMPL_SHARE_SUPPLY", &share_supply.to_string()),
            (
                "TMPL_INVESTORS_SHARE",
                &investors_share_percentage.to_string(),
            ),
            ("TMPL_PRECISION__", &precision.to_string()),
            ("TMPL_PRECISION_SQUARE", &precision_square.to_string()),
            ("TMPL_CAPI_ESCROW_ADDRESS", &capi_address.0.to_string()),
            ("TMPL_CAPI_SHARE", &capi_share.to_string()),
            ("TMPL_SHARE_PRICE", &share_price.val().to_string()),
            (
                "TMPL_MAX_RAISABLE_AMOUNT",
                &max_raisable_amount.val().to_string(),
            ),
        ],
    )?;
    #[cfg(not(target_arch = "wasm32"))]
    save_rendered_teal("dao_app_approval", source.clone())?; // debugging
    Ok(source)
}

pub async fn render_and_compile_app_clear(
    algod: &Algod,
    template: &VersionedTealSourceTemplate,
) -> Result<CompiledTeal> {
    let source = match template.version.0 {
        1 => render_central_app_clear_v1(&template.template),
        _ => Err(anyhow!(
            "Dao app clear version not supported: {:?}",
            template.version
        )),
    }?;

    Ok(algod.compile_teal(&source.0).await?)
}

pub fn render_central_app_clear_v1(template: &TealSourceTemplate) -> Result<TealSource> {
    Ok(TealSource(template.0.clone()))
}

#[derive(Serialize)]
struct RenderCentralAppContext {
    share_supply: String,
    investors_share: String,
    precision: String,
    precision_square: String,
    capi_escrow_address: String,
    capi_app_id: String,
    capi_share: String,
    share_price: String,
}

#[cfg(test)]
mod tests {
    use crate::testing::TESTS_DEFAULT_PRECISION;
    use algonaut::{
        model::algod::v2::TealKeyValue,
        transaction::{transaction::StateSchema, Transaction, TransactionType},
    };
    use anyhow::{anyhow, Result};
    use mbase::{
        api::version::{Version, VersionedTealSourceTemplate},
        dependencies::algod_for_tests,
        models::{
            capi_deps::{CapiAddress, CapiAssetDaoDeps},
            funds::FundsAmount,
            share_amount::ShareAmount,
        },
        teal::load_teal_template,
        util::{decimal_util::AsDecimal, network_util::wait_for_pending_transaction},
    };
    use network_test_util::{
        test_data::{creator, investor1},
        test_init,
    };
    use rust_decimal::Decimal;
    use serial_test::serial;
    use std::{convert::TryInto, str::FromStr};
    use tokio::test;

    use super::create_app_tx;

    #[test]
    #[serial] // reset network (cmd)
    async fn test_create_app() -> Result<()> {
        test_init().await?;

        // deps
        let algod = algod_for_tests();
        let creator = creator();

        let approval_template =
            VersionedTealSourceTemplate::new(load_teal_template("dao_app_approval")?, Version(1));
        let clear_template =
            VersionedTealSourceTemplate::new(load_teal_template("dao_app_clear")?, Version(1));

        let params = algod.suggested_transaction_params().await?;

        // asset supply isn't used here so we can pass anything (0 in this case)
        let tx = create_app_tx(
            &algod,
            &approval_template,
            &clear_template,
            &creator.address(),
            ShareAmount::new(1),
            TESTS_DEFAULT_PRECISION,
            Decimal::from_str("0.4")?.try_into()?,
            &params,
            // Arbitrary - not used
            &CapiAssetDaoDeps {
                escrow_percentage: 0_u64.as_decimal().try_into()?,
                address: CapiAddress(investor1().address()),
            },
            // Arbitrary - not used
            FundsAmount::new(10),
            FundsAmount::new(5_000_000_000_000),
        )
        .await?;

        let signed_tx = creator.sign_transaction(tx)?;
        let res = algod.broadcast_signed_transaction(&signed_tx).await?;

        log::debug!("App created! tx id: {:?}", res.tx_id);
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
        // assert_eq!(creator.address(), app.params.creator);
        assert_eq!(Vec::<TealKeyValue>::new(), app.params.global_state);
        assert_eq!(p_tx_app_index, app.id); // just a general sanity check: id returning in pending tx is the same as in creator account
        assert!(app.params.global_state_schema.is_some());
        assert!(app.params.local_state_schema.is_some());

        // the repetition here wouldn't be needed if algonaut used the same types for transaction and algod::v2..
        let params_global_schema = app.params.global_state_schema.as_ref().unwrap();
        let params_local_schema = app.params.local_state_schema.as_ref().unwrap();
        assert_eq!(
            global_state_schema(&signed_tx.transaction)?
                .unwrap()
                .number_ints,
            params_global_schema.num_uint
        );
        assert_eq!(
            global_state_schema(&signed_tx.transaction)?
                .unwrap()
                .number_byteslices,
            params_global_schema.num_byte_slice
        );
        assert_eq!(
            local_state_schema(&signed_tx.transaction)?
                .unwrap()
                .number_ints,
            params_local_schema.num_uint
        );
        assert_eq!(
            local_state_schema(&signed_tx.transaction)?
                .unwrap()
                .number_byteslices,
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
