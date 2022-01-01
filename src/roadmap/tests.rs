#[cfg(test)]
mod tests {
    use anyhow::Result;
    use serial_test::serial;
    use tokio::test;

    use crate::{
        dependencies,
        roadmap::add_roadmap_item::{
            add_roadmap_item, submit_add_roadmap_item, AddRoadmapItemToSigned, RoadmapItemInputs,
        },
        testing::{
            flow::create_project_flow::create_project_flow,
            network_test_util::test_init,
            test_data::{creator, project_specs},
            TESTS_DEFAULT_PRECISION,
        },
    };

    #[test]
    #[serial]
    async fn test_add_roadmap_item() -> Result<()> {
        test_init()?;

        // deps
        let algod = dependencies::algod_for_tests();
        // let indexer = dependencies::indexer_for_tests();
        let creator = creator();

        // UI
        let specs = project_specs();

        let project =
            create_project_flow(&algod, &creator, &specs, TESTS_DEFAULT_PRECISION).await?;

        let inputs = RoadmapItemInputs {
            project_uuid: project.uuid,
            title: "MVP Release".to_owned(),
            parent: Box::new(None),
        };

        let to_sign = add_roadmap_item(&algod, &creator.address(), &inputs).await?;

        // UI
        let signed_tx = creator.sign_transaction(&to_sign.tx)?;

        let tx_id =
            submit_add_roadmap_item(&algod, &AddRoadmapItemToSigned { tx: signed_tx }).await?;
        log::debug!("Add roadmap item tx id: {}", tx_id);

        // // check that the item was added correctly
        // // commented for now as it needs a long pause for the indexer to get the transaction, and we don't want to add this delay to the tests
        // //
        // // maybe it's possible to trigger indexing manually to not have to wait?
        // std::thread::sleep(std::time::Duration::from_secs(120));

        // // check that we can retrieve the same item we just saved

        // let saved_roadmap = get_roadmap(&indexer, &creator.address(), &project.uuid).await?;

        // assert_eq!(1, saved_roadmap.items.len());

        // let saved_item = &saved_roadmap.items[0];
        // assert_eq!(
        //     inputs,
        //     saved_roadmap_item_into_roadmap_item_inputs(saved_item)
        // );

        Ok(())
    }

    // // we need this convertion only for tests so here and explicit
    // fn saved_roadmap_item_into_roadmap_item_inputs(
    //     saved_item: &SavedRoadmapItem,
    // ) -> RoadmapItemInputs {
    //     RoadmapItemInputs {
    //         project_uuid: saved_item.project_uuid.clone(),
    //         title: saved_item.title.clone(),
    //         parent: saved_item.parent.clone(),
    //     }
    // }
}