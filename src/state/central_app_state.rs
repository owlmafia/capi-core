use std::convert::TryInto;

use crate::flows::create_project::storage::load_project::ProjectId;

use super::app_state::{
    global_state, local_state, local_state_from_account, AppStateKey, ApplicationLocalStateError,
    ApplicationStateExt,
};
use algonaut::{
    algod::v2::Algod,
    core::{Address, MicroAlgos},
    model::algod::v2::{Account, ApplicationLocalState},
};
use anyhow::{anyhow, Result};

const GLOBAL_TOTAL_RECEIVED: AppStateKey = AppStateKey("CentralReceivedTotal");

const LOCAL_HARVESTED_TOTAL: AppStateKey = AppStateKey("HarvestedTotal");
const LOCAL_SHARES: AppStateKey = AppStateKey("Shares");
const LOCAL_PROJECT: AppStateKey = AppStateKey("Project");

pub struct CentralAppGlobalState {
    pub received: MicroAlgos,
}

pub async fn central_global_state(algod: &Algod, app_id: u64) -> Result<CentralAppGlobalState> {
    let global_state = global_state(algod, app_id).await?;
    let total_received = MicroAlgos(global_state.find_uint(&GLOBAL_TOTAL_RECEIVED).unwrap_or(0));
    Ok(CentralAppGlobalState {
        received: total_received,
    })
}

pub struct CentralAppInvestorState {
    pub shares: u64,
    pub harvested: MicroAlgos,
    pub project_id: ProjectId,
}

pub async fn central_investor_state(
    algod: &Algod,
    investor: &Address,
    app_id: u64,
) -> Result<CentralAppInvestorState, ApplicationLocalStateError> {
    let local_state = local_state(algod, investor, app_id).await?;
    central_investor_state_from_local_state(&local_state)
        .map_err(|e| ApplicationLocalStateError::Msg(e.to_string()))
}

pub fn central_investor_state_from_acc(
    account: &Account,
    app_id: u64,
) -> Result<CentralAppInvestorState, ApplicationLocalStateError> {
    let local_state = local_state_from_account(account, app_id)?;
    central_investor_state_from_local_state(&local_state)
        .map_err(|e| ApplicationLocalStateError::Msg(e.to_string()))
}

/// Private: assumes that local state belongs to the central app (thus returns defaults values if local state isn't set)
/// It also expects (as the name implies) the user to be currently invested - returns error otherwise.
fn central_investor_state_from_local_state(
    state: &ApplicationLocalState,
) -> Result<CentralAppInvestorState> {
    // TODO why do these 2 fallback to 0 again? they're initialized to 0 when investing in TEAL, is it because the backend doesn't return 0s?
    // TODO related with above, we're not differentiating between (local state doesn't exist (not opted in)) and 0 value here - problematic?
    let shares = state.find_uint(&LOCAL_SHARES).unwrap_or(0);
    let harvested = MicroAlgos(state.find_uint(&LOCAL_HARVESTED_TOTAL).unwrap_or(0));

    // Assumes that user has already invested (exits with error if project id not set, which is done during investing)
    let project_id_bytes = state
        .find_bytes(&LOCAL_PROJECT)
        .ok_or_else(|| anyhow!("Project id not found in central local state."))?;

    let project_id: ProjectId = project_id_bytes.as_slice().try_into()?;

    Ok(CentralAppInvestorState {
        shares,
        harvested,
        project_id,
    })
}

/// Gets project ids for all the capi apps where the user is opted in
pub fn find_state_with_a_capi_project_id(
    app_local_state: &ApplicationLocalState,
) -> Result<Option<ProjectId>> {
    let maybe_bytes = app_local_state.find_bytes(&LOCAL_PROJECT);
    match maybe_bytes {
        Some(bytes) => {
            let project_id: ProjectId = bytes.as_slice().try_into()?;
            Ok(Some(project_id))
        }
        // Not found is Ok: we just didn't find a matching key value
        None => Ok(None),
    }
}
