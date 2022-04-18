use crate::{
    api::{api::Api, contract::Contract},
    capi_asset::capi_asset_dao_specs::CapiAssetDaoDeps,
    flows::create_dao::{
        create_dao_specs::CreateDaoSpecs,
        model::{CreateSharesSpecs, Dao},
        setup::{
            customer_escrow::render_and_compile_customer_escrow,
            investing_escrow::render_and_compile_investing_escrow,
        },
        share_amount::ShareAmount,
    },
    state::dao_app_state::dao_global_state,
};
use algonaut::{
    algod::v2::Algod,
    core::{to_app_address, Address},
    crypto::HashDigest,
};
use anyhow::{anyhow, Result};
use data_encoding::BASE32_NOPAD;
use futures::join;
use serde::{Deserialize, Serialize};
use std::{
    convert::{TryFrom, TryInto},
    hash::Hash,
    str::FromStr,
};

/// NOTE: this is an expensive function:
/// - Call to load dao app state
/// - Calls to retrieve TEAL templates for ALL the escrows (currently local, later this will come from API. Can be parallelized.)
/// - Call to retrieve asset information (supply etc, using the asset id stored in the app state)
/// - Calls to render and compile ALL the escrows (parallelized - 2 batches)
/// TODO parallelize more (and outside of this function, try to cache the dao, etc. to not have to call this often)
pub async fn load_dao(
    algod: &Algod,
    dao_id: DaoId,
    api: &dyn Api,
    capi_deps: &CapiAssetDaoDeps,
) -> Result<Dao> {
    let app_id = dao_id.0;

    log::debug!("Fetching dao with id: {:?}", app_id);

    let dao_state = dao_global_state(algod, app_id).await?;

    // NOTE currently not async - trait doesn't support async out of the box (TODO)
    // will be needed especially later when fetching the TEAL from a remove location
    let customer_escrow = api.template(Contract::DaoCustomer, dao_state.customer_escrow.version)?;
    let investing_escrow =
        api.template(Contract::DaoInvesting, dao_state.investing_escrow.version)?;

    // TODO store this state (redundantly in the same app field), to prevent this call?
    let asset_infos = algod.asset_information(dao_state.shares_asset_id).await?;

    // Render and compile the escrows
    let customer_escrow_account_fut =
        render_and_compile_customer_escrow(algod, &customer_escrow, &capi_deps.escrow, app_id);
    let investing_escrow_account_fut = render_and_compile_investing_escrow(
        algod,
        dao_state.shares_asset_id,
        &dao_state.share_price,
        &dao_state.funds_asset_id,
        &investing_escrow,
        app_id,
    );
    let (customer_escrow_account_res, investing_escrow_account_res) =
        join!(customer_escrow_account_fut, investing_escrow_account_fut);
    let customer_escrow_account = customer_escrow_account_res?;
    let investing_escrow_account = investing_escrow_account_res?;

    // validate the generated programs against the addresses stored in the app
    expect_match(
        &dao_state.customer_escrow.address,
        customer_escrow_account.account.address(),
    )?;
    expect_match(
        &dao_state.investing_escrow.address,
        investing_escrow_account.account.address(),
    )?;

    let dao = Dao {
        specs: CreateDaoSpecs::new(
            dao_state.project_name.clone(),
            dao_state.project_desc.clone(),
            CreateSharesSpecs {
                token_name: asset_infos.params.name.unwrap_or("".to_owned()),
                supply: ShareAmount::new(asset_infos.params.total),
            },
            dao_state.investors_part,
            dao_state.share_price,
            dao_state.logo_url.clone(),
            dao_state.social_media_url.clone(),
        )?,
        funds_asset_id: dao_state.funds_asset_id,
        creator: dao_state.owner,
        shares_asset_id: dao_state.shares_asset_id,
        app_id,
        invest_escrow: investing_escrow_account,
        customer_escrow: customer_escrow_account,
    };

    Ok(dao)
}

fn expect_match(stored_address: &Address, generated_address: &Address) -> Result<()> {
    if stored_address != generated_address {
        return Err(anyhow!("Stored address: {stored_address:?} doesn't match with generated address: {generated_address:?}"));
    }
    Ok(())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Hash)]
pub struct DaoId(pub DaoAppId);
impl DaoId {
    pub fn bytes(&self) -> [u8; 8] {
        self.0.bytes()
    }
}

impl FromStr for DaoId {
    type Err = anyhow::Error;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let app_id: DaoAppId = s.parse()?;
        Ok(DaoId(app_id))
    }
}
impl ToString for DaoId {
    fn to_string(&self) -> String {
        self.0.to_string()
    }
}

impl TryFrom<&[u8]> for DaoId {
    type Error = anyhow::Error;
    fn try_from(slice: &[u8]) -> Result<Self, Self::Error> {
        let app_id: DaoAppId = slice.try_into()?;
        Ok(DaoId(app_id))
    }
}

// TODO consider smart initializer: return error if id is 0 (invalid dao/app id)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Hash)]
pub struct DaoAppId(pub u64);

impl FromStr for DaoAppId {
    type Err = anyhow::Error;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(DaoAppId(s.parse()?))
    }
}
impl ToString for DaoAppId {
    fn to_string(&self) -> String {
        self.0.to_string()
    }
}

impl DaoAppId {
    pub fn bytes(&self) -> [u8; 8] {
        // note: matches to_le_bytes() in DaoId::from()
        self.0.to_le_bytes()
    }

    pub fn address(&self) -> Address {
        to_app_address(self.0)
    }
}

impl From<[u8; 8]> for DaoAppId {
    fn from(slice: [u8; 8]) -> Self {
        // note: matches to_le_bytes() in DaoId::bytes()
        DaoAppId(u64::from_le_bytes(slice))
    }
}

impl TryFrom<&[u8]> for DaoAppId {
    type Error = anyhow::Error;
    fn try_from(slice: &[u8]) -> Result<Self, Self::Error> {
        let array: [u8; 8] = slice.try_into()?;
        Ok(array.into())
    }
}

#[derive(Debug, Clone, Eq, Serialize, Deserialize)]
pub struct TxId(pub HashDigest);
impl FromStr for TxId {
    type Err = anyhow::Error;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let bytes_vec = BASE32_NOPAD.decode(s.as_bytes())?;
        Ok(Self(HashDigest(bytes_vec.try_into().map_err(
            |v: Vec<u8>| anyhow!("Tx id bytes vec has wrong length: {}", v.len()),
        )?)))
    }
}

impl Hash for TxId {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        // HashDigest doesn't implement Hash - and we can't add it because it's in Algonaut,
        // and seems overkill to add this to Algonaut, so we call it on the wrapped bytes
        self.0 .0.hash(state);
    }
}

// Implemented to be consistent with the manual Hash implementation (also: Clippy complains otherwise)
// with the macro implementation it would compare the wrapped HashDigest instead of the bytes in HashDigest - it leads to the same result but not strictly consistent.
impl PartialEq for TxId {
    fn eq(&self, other: &Self) -> bool {
        self.0 .0 == other.0 .0
    }
}

impl ToString for TxId {
    fn to_string(&self) -> String {
        BASE32_NOPAD.encode(&self.0 .0)
    }
}
impl From<HashDigest> for TxId {
    fn from(digest: HashDigest) -> Self {
        TxId(digest)
    }
}
impl TryFrom<&[u8]> for TxId {
    type Error = anyhow::Error;
    fn try_from(slice: &[u8]) -> Result<Self, Self::Error> {
        Ok(TxId(HashDigest(slice.try_into()?)))
    }
}
