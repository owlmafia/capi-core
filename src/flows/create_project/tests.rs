#[cfg(test)]
mod tests {
    use crate::{
        dependencies,
        testing::{
            flow::create_project_flow::create_project_flow, test_data::project_specs,
            TESTS_DEFAULT_PRECISION,
        },
        testing::{network_test_util::reset_network, test_data::creator},
    };
    use anyhow::Result;
    use serial_test::serial;
    use tokio::test;

    #[test]
    #[serial] // reset network (cmd)
    async fn test_create_project_flow() -> Result<()> {
        reset_network()?;

        // deps
        let algod = dependencies::algod();
        let creator = creator();

        // UI
        let specs = project_specs();

        let precision = TESTS_DEFAULT_PRECISION;
        let project = create_project_flow(&algod, &creator, &specs, precision).await?;

        // UI
        println!("Submitted create project txs, project: {:?}", project);

        let creator_infos = algod.account_information(&creator.address()).await?;
        let created_assets = creator_infos.created_assets;

        assert_eq!(created_assets.len(), 1);

        println!("created_assets {:?}", created_assets);

        // created asset checks
        assert_eq!(created_assets[0].params.creator, creator.address());
        // name matches specs
        assert_eq!(
            created_assets[0].params.name,
            Some(project.specs.shares.token_name.clone())
        );
        // unit matches specs
        assert_eq!(
            created_assets[0].params.unit_name,
            Some(project.specs.shares.token_name.clone())
        );
        assert_eq!(specs.shares.count, created_assets[0].params.total);
        let creator_assets = creator_infos.assets;
        // creator sent all the assets to the escrow (during project creation): has 0
        assert_eq!(1, creator_assets.len()); // not opted-out (TODO maybe do this, no reason for creator to be opted in in the investor assets) so still there
        assert_eq!(0, creator_assets[0].amount);

        // investing escrow funding checks
        let escrow = project.invest_escrow;
        let escrow_infos = algod.account_information(&escrow.address).await?;
        // TODO refactor and check min algos balance
        let escrow_held_assets = escrow_infos.assets;
        assert_eq!(escrow_held_assets.len(), 1);
        assert_eq!(escrow_held_assets[0].asset_id, project.shares_asset_id);
        assert_eq!(escrow_held_assets[0].amount, project.specs.shares.count);

        // staking escrow funding checks
        let staking_escrow = project.staking_escrow;
        let staking_escrow_infos = algod.account_information(&staking_escrow.address).await?;
        let staking_escrow_held_assets = staking_escrow_infos.assets;
        // TODO refactor and check min algos balance
        assert_eq!(staking_escrow_held_assets.len(), 1);
        assert_eq!(
            staking_escrow_held_assets[0].asset_id,
            project.shares_asset_id
        );
        assert_eq!(staking_escrow_held_assets[0].amount, 0); // nothing staked yet

        Ok(())
    }
}