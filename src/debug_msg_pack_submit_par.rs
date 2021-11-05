use serde::Serialize;

#[cfg(test)]
mod tests {
    use anyhow::Result;
    use tokio::test;

    use crate::flows::vote::logic::{submit_vote, VoteSigned};

    use crate::{
        dependencies, flows::create_project::setup::create_app::render_central_app,
        teal::load_teal_template, testing::TESTS_DEFAULT_PRECISION,
    };

    // helper for environments that don't allow to open directly the TEAL debugger (e.g. WASM)
    // Copy the parameters, serialized to msg pack, here and run the test
    // (Note that Algonaut doesn't suppot JSON deserialization yet, otherwise we could use it alternatively)
    #[test]
    #[ignore]
    async fn debug_msg_pack_submit_par() -> Result<()> {
        let algod = dependencies::algod();

        // update rendered teal if needed - since teal was rendered with WASM,
        // it's possible that the saved teal used here is outdated
        let approval_template = load_teal_template("app_central_approval")?;
        // use parameters corresponding to current environment
        let _ = render_central_app(approval_template, 2, 100, TESTS_DEFAULT_PRECISION, 40)?;

        // insert msg pack serialized bytes
        let bytes = vec![];

        // replace these with correct payload/submit call (if needed)
        // it might be needed to temporarily derive Serialize/Deserialize
        let signed: VoteSigned = rmp_serde::from_slice(&bytes).unwrap();
        submit_vote(&algod, &signed).await?;

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
