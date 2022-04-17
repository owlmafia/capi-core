use super::{
    contract::Contract,
    version::{Version, VersionedTealSourceTemplate, Versions},
};
use crate::teal::load_teal_template;
use anyhow::{anyhow, Result};

pub trait Api {
    fn last_versions(&self) -> Versions;
    fn template(&self, contract: Contract, version: Version)
        -> Result<VersionedTealSourceTemplate>;
}

pub struct LocalApi {}

impl Api for LocalApi {
    fn last_versions(&self) -> Versions {
        Versions {
            app_approval: Version(1),
            app_clear: Version(1),
            central_escrow: Version(1),
            customer_escrow: Version(1),
            investing_escrow: Version(1),
            locking_escrow: Version(1),
        }
    }

    fn template(
        &self,
        contract: Contract,
        version: Version,
    ) -> Result<VersionedTealSourceTemplate> {
        match contract {
            Contract::DaoCentral => dao_central_teal(version),
            Contract::DaoCustomer => dao_customer_teal(version),
            Contract::DaoInvesting => dao_investing_teal(version),
            Contract::Daolocking => dao_locking_teal(version),
            Contract::DaoAppApproval => dao_app_approval_teal(version),
            Contract::DaoAppClear => dao_app_clear_teal(version),
            Contract::CapiCentral => capi_central_teal(version),
            Contract::CapiAppApproval => capi_app_approval_teal(version),
            Contract::CapiAppClear => capi_app_clear_teal(version),
        }
    }
}

fn dao_central_teal(version: Version) -> Result<VersionedTealSourceTemplate> {
    match version.0 {
        1 => load_versioned_teal_template(version, "central_escrow"),
        _ => not_found_err("dao central", version),
    }
}

fn dao_customer_teal(version: Version) -> Result<VersionedTealSourceTemplate> {
    match version.0 {
        1 => load_versioned_teal_template(version, "customer_escrow"),
        _ => not_found_err("dao customer", version),
    }
}

fn dao_investing_teal(version: Version) -> Result<VersionedTealSourceTemplate> {
    match version.0 {
        1 => load_versioned_teal_template(version, "investing_escrow"),
        _ => not_found_err("dao investing", version),
    }
}

fn dao_locking_teal(version: Version) -> Result<VersionedTealSourceTemplate> {
    match version.0 {
        1 => load_versioned_teal_template(version, "locking_escrow"),
        _ => not_found_err("dao locking", version),
    }
}

fn dao_app_approval_teal(version: Version) -> Result<VersionedTealSourceTemplate> {
    match version.0 {
        1 => load_versioned_teal_template(version, "dao_app_approval"),
        _ => not_found_err("dao app", version),
    }
}

fn dao_app_clear_teal(version: Version) -> Result<VersionedTealSourceTemplate> {
    match version.0 {
        1 => load_versioned_teal_template(version, "dao_app_clear"),
        _ => not_found_err("dao app", version),
    }
}

fn capi_central_teal(version: Version) -> Result<VersionedTealSourceTemplate> {
    match version.0 {
        1 => load_versioned_teal_template(version, "capi_escrow"),
        _ => not_found_err("capi central", version),
    }
}

fn capi_app_approval_teal(version: Version) -> Result<VersionedTealSourceTemplate> {
    match version.0 {
        1 => load_versioned_teal_template(version, "capi_app_approval"),
        _ => not_found_err("capi approval app", version),
    }
}

fn capi_app_clear_teal(version: Version) -> Result<VersionedTealSourceTemplate> {
    match version.0 {
        1 => load_versioned_teal_template(version, "capi_app_clear"),
        _ => not_found_err("capi clear app", version),
    }
}

fn load_versioned_teal_template(
    version: Version,
    file_name: &str,
) -> Result<VersionedTealSourceTemplate> {
    Ok(VersionedTealSourceTemplate {
        version,
        template: load_teal_template(file_name)?,
    })
}

fn not_found_err<T>(id: &str, version: Version) -> Result<T> {
    Err(anyhow!("Not found version: {version:?} for: {id}"))
}