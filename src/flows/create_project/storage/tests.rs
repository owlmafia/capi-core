#[cfg(test)]
mod tests {
    use anyhow::Result;
    use tokio::test;

    use crate::{
        dependencies::{algod_for_tests, indexer_for_tests},
        flows::create_project::storage::{load_project::load_project, save_project::save_project},
        hashable::Hashable,
        testing::{
            flow::create_project_flow::{create_project_flow, programs},
            network_test_util::test_init,
            test_data::{creator, project_specs},
            TESTS_DEFAULT_PRECISION,
        },
    };

    #[test]
    // For now ignore, as it needs a long delay (> 1 min) to wait for indexing
    // TODO: can we trigger indexing manually?
    #[ignore]
    async fn saves_and_loads_project() -> Result<()> {
        test_init()?;

        // deps
        let algod = algod_for_tests();
        let indexer = indexer_for_tests();
        let creator = creator();
        let programs = programs()?;

        // UI
        let specs = project_specs();

        let precision = TESTS_DEFAULT_PRECISION;

        let project = create_project_flow(&algod, &creator, &specs, precision).await?;

        let to_sign = save_project(&algod, &creator.address(), &project.project).await?;

        let signed_tx = creator.sign_transaction(&to_sign.tx)?;

        let tx_id = algod.broadcast_signed_transaction(&signed_tx).await?.tx_id;

        println!(
            "Creator: {:?}, project hash: {:?}, tx id: {:?}. Will wait for indexing..",
            creator.address(),
            to_sign.project.hash()?,
            tx_id
        );

        std::thread::sleep(std::time::Duration::from_secs(70));

        println!("Fetching project..");

        let project_id = tx_id.parse()?;

        let loaded_project = load_project(&algod, &indexer, &project_id, &programs.escrows).await?;

        assert_eq!(project.project, loaded_project);
        // double check
        assert_eq!(
            project.project.compute_hash()?,
            loaded_project.compute_hash()?
        );

        Ok(())
    }
}
