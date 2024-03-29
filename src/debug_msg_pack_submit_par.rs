use std::fs;

use serde::Serialize;

#[cfg(test)]
#[allow(unused_imports)]
mod tests {
    use crate::{
        dependencies,
        flows::{
            claim::claim::{submit_claim, ClaimSigned},
            create_dao::setup::{create_app::render_central_app_approval_v1, setup_app},
            drain::drain::{submit_drain, DrainSigned},
            invest::{invest::submit_invest, model::InvestSigned},
            withdraw::withdraw::{submit_withdraw, WithdrawSigned},
        },
        testing::TESTS_DEFAULT_PRECISION,
    };
    use algonaut::core::Address;
    use anyhow::{Error, Result};
    use mbase::{
        dependencies::algod_for_tests,
        models::{capi_deps::CapiAddress, funds::FundsAmount, share_amount::ShareAmount},
        teal::load_teal_template,
        util::decimal_util::AsDecimal,
    };
    use rust_decimal::Decimal;
    use std::{convert::TryInto, str::FromStr};
    use tokio::test;

    // helper for environments that don't allow to open directly the TEAL debugger (e.g. WASM)
    // Copy the parameters, serialized to msg pack, here and run the test
    // (Note that Algonaut doesn't suppot JSON deserialization yet, otherwise we could use it alternatively)
    #[test]
    #[ignore]
    async fn debug_msg_pack_submit_par() -> Result<()> {
        let algod = algod_for_tests();

        // Set parameters to match current environment

        // let shares_asset_id = 20;
        let shares_price = FundsAmount::new(10000000);
        // let funds_asset_id = FundsAssetId(6);
        let share_supply = ShareAmount::new(100);
        let investors_share = Decimal::from_str("0.4")?.try_into()?;
        // let app_id = DaoAppId(123);
        let capi_address = CapiAddress("".parse().unwrap());
        let capi_share = 123u64.as_decimal().try_into()?;
        let max_raisable_amount = FundsAmount::new(5_000_000_000_000);

        // let capi_deps = &CapiAssetDaoDeps {
        //     escrow_percentage: Decimal::from_str("0.1").unwrap().try_into()?,
        //     app_id: CapiAppId(123),
        //     asset_id: CapiAssetId(123),
        // };

        // update rendered teal if needed - since teal was rendered with WASM,
        // it's possible that the saved teal used here is outdated

        let approval_template = load_teal_template("dao_app_approval")?;
        render_central_app_approval_v1(
            &approval_template,
            share_supply,
            TESTS_DEFAULT_PRECISION,
            investors_share,
            &capi_address,
            capi_share,
            shares_price,
            max_raisable_amount,
        )?;

        // insert msg pack serialized bytes
        let bytes = vec![];

        // let signed: ClaimSigned = rmp_serde::from_slice(&bytes).unwrap();

        // let signed: WithdrawSigned = rmp_serde::from_slice(&bytes).unwrap();
        // submit_withdraw(&algod, &signed).await?;

        let signed: InvestSigned = rmp_serde::from_slice(&bytes).unwrap();
        submit_invest(&algod, &signed).await?;

        Ok(())
    }
}

#[allow(dead_code)]
pub fn log_to_msg_pack<T>(obj: &T)
where
    T: Serialize + ?Sized,
{
    log::info!("log_to_msg_pack:");
    // Unwrap: only for debugging
    log::info!("{:?}", rmp_serde::to_vec_named(obj).unwrap());
}

/// To import things in SDKs that use signed bytes
#[allow(dead_code)]
pub fn log_to_msg_pack_signed<T>(obj: &T)
where
    T: Serialize + ?Sized,
{
    log::info!("log_to_msg_pack (signed):");
    // Unwrap: only for debugging
    let bytes = rmp_serde::to_vec_named(obj).unwrap();
    let signed_bytes = bytes.into_iter().map(|b| b as i8).collect::<Vec<i8>>();
    log::info!("{:?}", signed_bytes);
}

#[allow(dead_code)]
pub fn write_bytes_to_tmp_file(bytes: &[u8]) {
    // just a (gitignore) file in the root directory
    fs::write("./some_bytes", bytes).unwrap();
}
