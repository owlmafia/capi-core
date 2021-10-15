#[cfg(test)]
use crate::app_state_util::app_local_state_or_err;
#[cfg(test)]
use crate::app_state_util::app_local_var_or_err;
#[cfg(test)]
use algonaut::algod::v2::Algod;
#[cfg(test)]
use algonaut::core::Address;
#[cfg(test)]
use algonaut::core::MicroAlgos;
#[cfg(test)]
use algonaut::model::algod::v2::Application;
#[cfg(test)]
use algonaut::model::algod::v2::ApplicationLocalState;
#[cfg(test)]
use anyhow::Result;
#[cfg(test)]
use data_encoding::BASE64;

#[cfg(test)]
pub fn check_schema(app: &Application) {
    assert!(app.params.global_state_schema.is_some());
    let app_global_state_schema = app.params.global_state_schema.as_ref().unwrap();
    assert_eq!(0, app_global_state_schema.num_byte_slice);
    assert_eq!(1, app_global_state_schema.num_uint);
    assert!(app.params.local_state_schema.is_some());
    let app_local_state_schema = app.params.local_state_schema.as_ref().unwrap();
    assert_eq!(0, app_local_state_schema.num_byte_slice);
    assert_eq!(2, app_local_state_schema.num_uint);
}

#[cfg(test)]
pub fn check_investor_local_state(
    local_state: Vec<ApplicationLocalState>,
    central_app_id: u64,
    expected_shares: u64,
    expected_harvested_total: MicroAlgos,
) {
    assert_eq!(1, local_state.len());
    let local_state = &local_state[0];
    assert_eq!(central_app_id, local_state.id);
    let local_key_values = &local_state.key_value;
    assert_eq!(2, local_key_values.len());

    let shares_local_key_value_opt = &local_key_values
        .iter()
        .find(|kv| kv.key == BASE64.encode(b"Shares").to_owned());
    assert!(shares_local_key_value_opt.is_some());
    let shares_local_key_value = shares_local_key_value_opt.unwrap();
    assert_eq!(Vec::<u8>::new(), shares_local_key_value.value.bytes);
    assert_eq!(expected_shares, shares_local_key_value.value.uint);

    let harvested_total_local_key_value_opt = &local_key_values
        .iter()
        .find(|kv| kv.key == BASE64.encode(b"HarvestedTotal").to_owned());
    assert!(harvested_total_local_key_value_opt.is_some());
    let harvested_total_local_key_value = harvested_total_local_key_value_opt.unwrap();
    assert_eq!(
        Vec::<u8>::new(),
        harvested_total_local_key_value.value.bytes
    );
    assert_eq!(
        expected_harvested_total.0,
        harvested_total_local_key_value.value.uint
    );
}

#[cfg(test)]
pub async fn test_withdrawal_slot_local_state_initialized_correctly(
    algod: &Algod,
    investor_address: &Address,
    app_id: u64,
) -> Result<()> {
    let account = algod.account_information(investor_address).await?;
    let local_state = account.apps_local_state;

    let app_local_state = app_local_state_or_err(&local_state, app_id)?;

    // initialized with 0 votes (to be easy to increment when voting)
    let lvotes = app_local_var_or_err(&app_local_state, "LVotes")?;
    assert!(lvotes.bytes.is_empty());
    assert_eq!(0, lvotes.uint);

    // when investing, valid is set to true (1)
    let valid = app_local_var_or_err(&app_local_state, "Valid")?;
    assert!(valid.bytes.is_empty());
    assert_eq!(1, valid.uint);

    Ok(())
}
