use std::fmt::{self, Display, Formatter};

use algonaut::{
    algod::v2::Algod,
    core::Address,
    error::AlgonautError,
    model::algod::v2::{Account, ApplicationLocalState, TealKeyValue, TealValue},
};
use anyhow::{anyhow, Result};
use data_encoding::BASE64;

pub async fn global_state(algod: &Algod, app_id: u64) -> Result<ApplicationGlobalState> {
    let app = algod.application_information(app_id).await?;
    Ok(ApplicationGlobalState(app.params.global_state))
}

pub async fn local_state(
    algod: &Algod,
    address: &Address,
    app_id: u64,
) -> Result<ApplicationLocalState, ApplicationLocalStateError> {
    let investor_account_infos = algod.account_information(address).await?;
    local_state_from_account(&investor_account_infos, app_id)
}

pub fn local_state_from_account(
    account: &Account,
    app_id: u64,
) -> Result<ApplicationLocalState, ApplicationLocalStateError> {
    account
        .apps_local_state
        .iter()
        .find(|ls| ls.id == app_id)
        .cloned()
        .ok_or(ApplicationLocalStateError::NotOptedIn)
}

pub fn local_state_with_key(
    app_local_state: ApplicationLocalState,
    key: &AppStateKey,
) -> Option<TealValue> {
    find_value(&app_local_state.key_value, key)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ApplicationLocalStateError {
    NotOptedIn,
    Msg(String),
}

#[derive(Debug, Clone)]
pub struct AppStateKey<'a>(pub &'a str);

/// Just a wrapper equivalent to ApplicationLocalState (provided by the SDK), to offer a similar interface
pub struct ApplicationGlobalState(Vec<TealKeyValue>);

pub trait ApplicationStateExt {
    fn find(&self, key: &AppStateKey) -> Option<TealValue>;
    fn find_uint(&self, key: &AppStateKey) -> Option<u64>;
    fn find_bytes(&self, key: &AppStateKey) -> Option<Vec<u8>>;
}

impl ApplicationStateExt for ApplicationLocalState {
    fn find(&self, key: &AppStateKey) -> Option<TealValue> {
        find_value(&self.key_value, key)
    }

    fn find_uint(&self, key: &AppStateKey) -> Option<u64> {
        self.find(key).map(|kv| kv.uint)
    }

    fn find_bytes(&self, key: &AppStateKey) -> Option<Vec<u8>> {
        self.find(key).map(|kv| kv.bytes)
    }
}

impl ApplicationStateExt for ApplicationGlobalState {
    fn find(&self, key: &AppStateKey) -> Option<TealValue> {
        find_value(&self.0, key)
    }

    fn find_uint(&self, key: &AppStateKey) -> Option<u64> {
        self.find(key).map(|kv| kv.uint)
    }

    fn find_bytes(&self, key: &AppStateKey) -> Option<Vec<u8>> {
        self.find(key).map(|kv| kv.bytes)
    }
}

fn find_value(key_values: &[TealKeyValue], key: &AppStateKey) -> Option<TealValue> {
    key_values
        .iter()
        .find(|kv| kv.key_matches(key))
        .map(|kv| kv.value.clone())
}

trait TealKeyValueExt {
    fn key_matches(&self, key: &AppStateKey) -> bool;
}

impl TealKeyValueExt for TealKeyValue {
    fn key_matches(&self, key: &AppStateKey) -> bool {
        self.key == BASE64.encode(key.0.as_bytes())
    }
}

impl Display for ApplicationLocalStateError {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "{:?}", self)
    }
}

impl From<AlgonautError> for ApplicationLocalStateError {
    fn from(err: AlgonautError) -> Self {
        ApplicationLocalStateError::Msg(err.to_string())
    }
}

impl From<ApplicationLocalStateError> for anyhow::Error {
    fn from(err: ApplicationLocalStateError) -> Self {
        anyhow!("{}", err)
    }
}
