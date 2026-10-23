mod filter_specs;

use crate::to_wide;
use anyhow::Result;
use anyhow::bail;
use anyhow::ensure;
use serde::Serialize;
use std::ffi::OsStr;
use std::ffi::c_void;
use std::mem::zeroed;
use std::ptr::null;
use std::ptr::null_mut;
use windows_sys::Win32::Foundation::FWP_E_ALREADY_EXISTS;
use windows_sys::Win32::Foundation::FWP_E_FILTER_NOT_FOUND;
use windows_sys::Win32::Foundation::FWP_E_NOT_FOUND;
use windows_sys::Win32::Foundation::FWP_E_PROVIDER_NOT_FOUND;
use windows_sys::Win32::Foundation::FWP_E_SUBLAYER_NOT_FOUND;
use windows_sys::Win32::Foundation::HANDLE;
use windows_sys::Win32::Foundation::HLOCAL;
use windows_sys::Win32::Foundation::LocalFree;
use windows_sys::Win32::NetworkManagement::WindowsFilteringPlatform::FWP_ACTION_BLOCK;
use windows_sys::Win32::NetworkManagement::WindowsFilteringPlatform::FWP_ACTRL_MATCH_FILTER;
use windows_sys::Win32::NetworkManagement::WindowsFilteringPlatform::FWP_BYTE_BLOB;
use windows_sys::Win32::NetworkManagement::WindowsFilteringPlatform::FWP_CONDITION_VALUE0;
use windows_sys::Win32::NetworkManagement::WindowsFilteringPlatform::FWP_CONDITION_VALUE0_0;
use windows_sys::Win32::NetworkManagement::WindowsFilteringPlatform::FWP_EMPTY;
use windows_sys::Win32::NetworkManagement::WindowsFilteringPlatform::FWP_MATCH_EQUAL;
use windows_sys::Win32::NetworkManagement::WindowsFilteringPlatform::FWP_SECURITY_DESCRIPTOR_TYPE;
use windows_sys::Win32::NetworkManagement::WindowsFilteringPlatform::FWP_UINT8;
use windows_sys::Win32::NetworkManagement::WindowsFilteringPlatform::FWP_UINT16;
use windows_sys::Win32::NetworkManagement::WindowsFilteringPlatform::FWP_VALUE0;
use windows_sys::Win32::NetworkManagement::WindowsFilteringPlatform::FWPM_ACTION0;
use windows_sys::Win32::NetworkManagement::WindowsFilteringPlatform::FWPM_ACTION0_0;
use windows_sys::Win32::NetworkManagement::WindowsFilteringPlatform::FWPM_CONDITION_ALE_USER_ID;
use windows_sys::Win32::NetworkManagement::WindowsFilteringPlatform::FWPM_CONDITION_IP_PROTOCOL;
use windows_sys::Win32::NetworkManagement::WindowsFilteringPlatform::FWPM_CONDITION_IP_REMOTE_PORT;
use windows_sys::Win32::NetworkManagement::WindowsFilteringPlatform::FWPM_DISPLAY_DATA0;
use windows_sys::Win32::NetworkManagement::WindowsFilteringPlatform::FWPM_FILTER_CONDITION0;
use windows_sys::Win32::NetworkManagement::WindowsFilteringPlatform::FWPM_FILTER_FLAG_PERSISTENT;
use windows_sys::Win32::NetworkManagement::WindowsFilteringPlatform::FWPM_FILTER0;
use windows_sys::Win32::NetworkManagement::WindowsFilteringPlatform::FWPM_FILTER0_0;
use windows_sys::Win32::NetworkManagement::WindowsFilteringPlatform::FWPM_PROVIDER_FLAG_PERSISTENT;
use windows_sys::Win32::NetworkManagement::WindowsFilteringPlatform::FWPM_PROVIDER0;
use windows_sys::Win32::NetworkManagement::WindowsFilteringPlatform::FWPM_SESSION0;
use windows_sys::Win32::NetworkManagement::WindowsFilteringPlatform::FWPM_SUBLAYER_FLAG_PERSISTENT;
use windows_sys::Win32::NetworkManagement::WindowsFilteringPlatform::FWPM_SUBLAYER0;
use windows_sys::Win32::NetworkManagement::WindowsFilteringPlatform::FwpmEngineClose0;
use windows_sys::Win32::NetworkManagement::WindowsFilteringPlatform::FwpmEngineOpen0;
use windows_sys::Win32::NetworkManagement::WindowsFilteringPlatform::FwpmFilterAdd0;
use windows_sys::Win32::NetworkManagement::WindowsFilteringPlatform::FwpmFilterDeleteByKey0;
use windows_sys::Win32::NetworkManagement::WindowsFilteringPlatform::FwpmFilterGetByKey0;
use windows_sys::Win32::NetworkManagement::WindowsFilteringPlatform::FwpmFreeMemory0;
use windows_sys::Win32::NetworkManagement::WindowsFilteringPlatform::FwpmProviderAdd0;
use windows_sys::Win32::NetworkManagement::WindowsFilteringPlatform::FwpmProviderDeleteByKey0;
use windows_sys::Win32::NetworkManagement::WindowsFilteringPlatform::FwpmProviderGetByKey0;
use windows_sys::Win32::NetworkManagement::WindowsFilteringPlatform::FwpmSubLayerAdd0;
use windows_sys::Win32::NetworkManagement::WindowsFilteringPlatform::FwpmSubLayerDeleteByKey0;
use windows_sys::Win32::NetworkManagement::WindowsFilteringPlatform::FwpmSubLayerGetByKey0;
use windows_sys::Win32::NetworkManagement::WindowsFilteringPlatform::FwpmTransactionAbort0;
use windows_sys::Win32::NetworkManagement::WindowsFilteringPlatform::FwpmTransactionBegin0;
use windows_sys::Win32::NetworkManagement::WindowsFilteringPlatform::FwpmTransactionCommit0;
use windows_sys::Win32::Security::Authorization::BuildExplicitAccessWithNameW;
use windows_sys::Win32::Security::Authorization::BuildSecurityDescriptorW;
use windows_sys::Win32::Security::Authorization::EXPLICIT_ACCESS_W;
use windows_sys::Win32::Security::Authorization::GRANT_ACCESS;
use windows_sys::Win32::Security::PSECURITY_DESCRIPTOR;
use windows_sys::Win32::System::Rpc::RPC_C_AUTHN_DEFAULT;
use windows_sys::Win32::System::Threading::INFINITE;
use windows_sys::core::GUID;

use filter_specs::ConditionSpec;
use filter_specs::FILTER_SPECS;
use filter_specs::FULL_OFFLINE_FILTER_SPECS;
use filter_specs::FilterSpec;

const SESSION_NAME: &str = "Codex Windows Sandbox WFP";
const PROVIDER_NAME: &str = "Codex Windows Sandbox WFP";
const PROVIDER_DESCRIPTION: &str = "Persistent WFP provider for Codex Windows sandbox filters";
const SUBLAYER_NAME: &str = "Codex Windows Sandbox WFP";
const SUBLAYER_DESCRIPTION: &str = "Persistent WFP sublayer for Codex Windows sandbox filters";
const FWP_E_IN_USE_CODE: u32 = 0x8032_0017;

// WFP identifies persistent providers, sublayers, and filters by stable GUIDs.
// These values are Codex-owned identities; do not regenerate them unless we
// intentionally want to orphan old objects and create a new WFP namespace.
const PROVIDER_KEY: GUID = GUID::from_u128(0x2e31d31c_3948_4753_9117_e5d1a6496f41);
const SUBLAYER_KEY: GUID = GUID::from_u128(0xe65054fd_4d32_4c7c_95ef_621f0cf6431a);

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct WfpGuid {
    pub(crate) data1: u32,
    pub(crate) data2: u16,
    pub(crate) data3: u16,
    pub(crate) data4: [u8; 8],
}

impl From<GUID> for WfpGuid {
    fn from(value: GUID) -> Self {
        Self {
            data1: value.data1,
            data2: value.data2,
            data3: value.data3,
            data4: value.data4,
        }
    }
}

impl From<WfpGuid> for GUID {
    fn from(value: WfpGuid) -> Self {
        Self {
            data1: value.data1,
            data2: value.data2,
            data3: value.data3,
            data4: value.data4,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum OwnedWfpConditionSpec {
    UserSd(Vec<u8>),
    Protocol(u8),
    RemotePort(u16),
}

#[derive(Debug, PartialEq, Eq)]
pub(crate) struct OwnedWfpFilterProof {
    pub(crate) name: &'static str,
    pub(crate) key: WfpGuid,
    pub(crate) filter_id: u64,
    pub(crate) layer_key: WfpGuid,
    pub(crate) sublayer_key: WfpGuid,
    pub(crate) provider_key: WfpGuid,
    pub(crate) expected_conditions: Vec<OwnedWfpConditionSpec>,
    pub(crate) expected_sd_blob: Vec<u8>,
    pub(crate) offline_sid_bytes: Vec<u8>,
}

/// Derived from the original requested policy in both the helper and its parent.
/// Receipt length or filter names must never select the enforcement profile.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum OwnedWfpProfile {
    Supplemental,
    FullOffline,
}

impl OwnedWfpProfile {
    pub(crate) fn for_network_policy(proxy_ports: &[u16], allow_local_binding: bool) -> Self {
        if proxy_ports.is_empty() && !allow_local_binding {
            Self::FullOffline
        } else {
            Self::Supplemental
        }
    }

    fn specs(self) -> impl Iterator<Item = &'static FilterSpec> {
        let additional = match self {
            Self::Supplemental => &[][..],
            Self::FullOffline => FULL_OFFLINE_FILTER_SPECS,
        };
        FILTER_SPECS.iter().chain(additional.iter())
    }

    fn filter_count(self) -> usize {
        self.specs().count()
    }
}

/// Non-deserializable proof of WFP objects created during a specific owned setup run.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct OwnedWfpProof {
    profile: OwnedWfpProfile,
    pub(crate) provider_key: WfpGuid,
    pub(crate) sublayer_key: WfpGuid,
    pub(crate) provider_created: bool,
    pub(crate) sublayer_created: bool,
    pub(crate) offline_sid: String,
    pub(crate) offline_sid_bytes: Vec<u8>,
    pub(crate) filters: Vec<OwnedWfpFilterProof>,
}

/// Bounded typed summary of scoped WFP retirement during normal cleanup.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct WfpRetirementReport {
    pub verified_filter_count: usize,
    pub retired_filter_ids: Vec<u64>,
    pub sublayer_retired: bool,
    pub sublayer_retained_shared: bool,
    pub provider_retired: bool,
    pub provider_retained_shared: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ObservedWfpFilter {
    pub(crate) key: WfpGuid,
    pub(crate) filter_id: u64,
    pub(crate) layer_key: WfpGuid,
    pub(crate) sublayer_key: WfpGuid,
    pub(crate) provider_key: Option<WfpGuid>,
    pub(crate) flags: u32,
    pub(crate) action_type: u32,
    pub(crate) conditions: Vec<OwnedWfpConditionSpec>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ObservedWfpState {
    pub(crate) provider_present_and_persistent: bool,
    pub(crate) sublayer_present_and_linked: bool,
    pub(crate) filters: Vec<ObservedWfpFilter>,
}

/// Installs the persistent Codex WFP filters for `account`.
///
/// This is intended to run from the already-elevated setup helper. Callers
/// may continue ordinary setup after an error, but must restore these filters
/// before re-enabling accounts left disabled by interrupted cleanup.
pub fn install_wfp_filters_for_account(account: &str) -> Result<usize> {
    let engine = Engine::open(INFINITE)?;
    let mut transaction = engine.begin_transaction()?;
    ensure_provider(engine.handle)?;
    ensure_sublayer(engine.handle)?;

    let user_condition = UserMatchCondition::for_account(account)?;
    let mut installed_filter_count = 0;
    for spec in FILTER_SPECS {
        delete_filter_if_present(engine.handle, &spec.key)?;
        add_filter(engine.handle, spec, &user_condition)?;
        installed_filter_count += 1;
    }

    transaction.commit()?;
    Ok(installed_filter_count)
}

/// Installs persistent Codex WFP filters for a newly created owned sandbox offline account,
/// refusing to overwrite pre-existing filters and recording exact runtime `filterId` and
/// security descriptor proof inside a single native WFP transaction.
pub(crate) fn install_owned_wfp_filters_for_account(
    profile: OwnedWfpProfile,
    account: &str,
    offline_sid: &str,
    offline_sid_bytes: &[u8],
) -> Result<OwnedWfpProof> {
    ensure!(
        !offline_sid.is_empty() && !offline_sid_bytes.is_empty(),
        "owned WFP installation requires non-empty offline SID proof"
    );
    let engine = Engine::open(INFINITE)?;
    let mut transaction = engine.begin_transaction()?;
    let provider_created = ensure_or_observe_provider(engine.handle)?;
    let sublayer_created = ensure_or_observe_sublayer(engine.handle)?;

    let user_condition = UserMatchCondition::for_account(account)?;
    let condition_sd_bytes = unsafe {
        std::slice::from_raw_parts(
            user_condition.blob.data,
            user_condition.blob.size as usize,
        )
    };
    let (mask, extracted_sid) = extract_wfp_sd_single_allowed_sid(condition_sd_bytes)?;
    ensure!(
        mask == FWP_ACTRL_MATCH_FILTER && extracted_sid.as_slice() == offline_sid_bytes,
        "WFP user match condition security descriptor SID does not match created offline SID"
    );

    let mut filters = Vec::with_capacity(profile.filter_count());
    for spec in profile.specs() {
        ensure!(
            observe_wfp_filter(engine.handle, &spec.key)?.is_none(),
            "pre-existing WFP filter {} present before owned setup; refusing to overwrite or adopt",
            spec.name
        );
        let filter_id = add_filter_returning_id(engine.handle, spec, &user_condition)?;
        ensure!(
            filter_id != 0,
            "FwpmFilterAdd0({}) returned zero filterId",
            spec.name
        );
        let observed = observe_wfp_filter(engine.handle, &spec.key)?.ok_or_else(|| {
            anyhow::anyhow!("newly added WFP filter {} could not be read back", spec.name)
        })?;
        ensure!(
            observed.filter_id == filter_id,
            "newly added WFP filter {} filterId mismatch ({} != {filter_id})",
            spec.name,
            observed.filter_id
        );
        let observed_sd = extract_single_user_sd(&observed.conditions)?;
        let (obs_mask, obs_sid) = extract_wfp_sd_single_allowed_sid(&observed_sd)?;
        ensure!(
            obs_mask == FWP_ACTRL_MATCH_FILTER && obs_sid.as_slice() == offline_sid_bytes,
            "newly added WFP filter {} security descriptor SID mismatch",
            spec.name
        );
        let expected_conditions = expected_conditions_for_spec(spec.conditions, &observed_sd);
        ensure!(
            observed.conditions == expected_conditions,
            "newly added WFP filter {} conditions do not match specification",
            spec.name
        );
        filters.push(OwnedWfpFilterProof {
            name: spec.name,
            key: WfpGuid::from(spec.key),
            filter_id,
            layer_key: WfpGuid::from(spec.layer_key),
            sublayer_key: WfpGuid::from(SUBLAYER_KEY),
            provider_key: WfpGuid::from(PROVIDER_KEY),
            expected_conditions,
            expected_sd_blob: observed_sd,
            offline_sid_bytes: offline_sid_bytes.to_vec(),
        });
    }

    transaction.commit()?;
    Ok(OwnedWfpProof {
        profile,
        provider_key: WfpGuid::from(PROVIDER_KEY),
        sublayer_key: WfpGuid::from(SUBLAYER_KEY),
        provider_created,
        sublayer_created,
        offline_sid: offline_sid.to_string(),
        offline_sid_bytes: offline_sid_bytes.to_vec(),
        filters,
    })
}

pub(crate) fn build_owned_wfp_proof_from_receipt_specs(
    profile: OwnedWfpProfile,
    provider_created: bool,
    sublayer_created: bool,
    offline_sid: &str,
    offline_sid_bytes: &[u8],
    reported_filters: &[(String, u64, Vec<u8>)],
) -> Result<OwnedWfpProof> {
    ensure!(
        !offline_sid.is_empty() && !offline_sid_bytes.is_empty(),
        "owned WFP receipt binding requires non-empty offline SID proof"
    );
    ensure!(
        reported_filters.len() == profile.filter_count(),
        "owned WFP receipt filter count {} does not match requested profile ({})",
        reported_filters.len(),
        profile.filter_count()
    );
    let mut filters = Vec::with_capacity(profile.filter_count());
    for (spec, (reported_name, filter_id, expected_sd_blob)) in
        profile.specs().zip(reported_filters.iter())
    {
        ensure!(
            reported_name == spec.name,
            "owned WFP receipt filter name {} does not match expected spec {}",
            reported_name,
            spec.name
        );
        ensure!(
            *filter_id != 0,
            "owned WFP receipt filter {} has zero filterId",
            spec.name
        );
        let (mask, sid_bytes) = extract_wfp_sd_single_allowed_sid(expected_sd_blob)?;
        ensure!(
            mask == FWP_ACTRL_MATCH_FILTER && sid_bytes.as_slice() == offline_sid_bytes,
            "owned WFP receipt filter {} security descriptor SID does not match offline SID",
            spec.name
        );
        let expected_conditions = expected_conditions_for_spec(spec.conditions, expected_sd_blob);
        filters.push(OwnedWfpFilterProof {
            name: spec.name,
            key: WfpGuid::from(spec.key),
            filter_id: *filter_id,
            layer_key: WfpGuid::from(spec.layer_key),
            sublayer_key: WfpGuid::from(SUBLAYER_KEY),
            provider_key: WfpGuid::from(PROVIDER_KEY),
            expected_conditions,
            expected_sd_blob: expected_sd_blob.clone(),
            offline_sid_bytes: offline_sid_bytes.to_vec(),
        });
    }
    Ok(OwnedWfpProof {
        profile,
        provider_key: WfpGuid::from(PROVIDER_KEY),
        sublayer_key: WfpGuid::from(SUBLAYER_KEY),
        provider_created,
        sublayer_created,
        offline_sid: offline_sid.to_string(),
        offline_sid_bytes: offline_sid_bytes.to_vec(),
        filters,
    })
}

/// Independently queries the live WFP engine in the parent process and verifies that every
/// filter reported by the pinned `codex-windows-sandbox-setup.exe` child exists in the OS with
/// the exact runtime `filterId`, security descriptor, and offline SID before constructing
/// `OwnedWfpProof`.
pub(crate) fn verify_and_bind_live_owned_wfp_proof(
    profile: OwnedWfpProfile,
    provider_created: bool,
    sublayer_created: bool,
    offline_sid: &str,
    offline_sid_bytes: &[u8],
    reported_filters: &[(String, u64, Vec<u8>)],
) -> Result<OwnedWfpProof> {
    let candidate = build_owned_wfp_proof_from_receipt_specs(
        profile,
        provider_created,
        sublayer_created,
        offline_sid,
        offline_sid_bytes,
        reported_filters,
    )?;
    let engine = Engine::open(INFINITE)?;
    let observed = observe_wfp_state(engine.handle, &candidate)?;
    let _verified_ids = verify_owned_wfp_state(&candidate, &observed)?;
    Ok(candidate)
}

/// Verifies and retires only the exact WFP filters proven to belong to `proof` inside a single
/// native WFP transaction. Shared provider/sublayer objects not created by this setup (or still
/// referenced by other WFP objects) are retained rather than globally deleted.
pub(crate) fn retire_owned_wfp_filters(proof: &OwnedWfpProof) -> Result<WfpRetirementReport> {
    let engine = Engine::open(INFINITE)?;
    let mut transaction = engine.begin_transaction()?;
    let observed = observe_wfp_state(engine.handle, proof)?;
    let verified_ids = verify_owned_wfp_state(proof, &observed)?;

    for filter_proof in &proof.filters {
        let key: GUID = filter_proof.key.into();
        let result = unsafe { FwpmFilterDeleteByKey0(engine.handle, &key) };
        ensure_success(
            result,
            &format!("FwpmFilterDeleteByKey0({})", filter_proof.name),
        )?;
    }

    let (sublayer_retired, sublayer_retained_shared) = if proof.sublayer_created {
        let result = unsafe { FwpmSubLayerDeleteByKey0(engine.handle, &SUBLAYER_KEY) };
        if result == 0 {
            (true, false)
        } else if result == FWP_E_IN_USE_CODE {
            (false, true)
        } else {
            ensure_success(result, "FwpmSubLayerDeleteByKey0")?;
            unreachable!()
        }
    } else {
        (false, true)
    };

    let (provider_retired, provider_retained_shared) =
        if proof.provider_created && sublayer_retired {
            let result = unsafe { FwpmProviderDeleteByKey0(engine.handle, &PROVIDER_KEY) };
            if result == 0 {
                (true, false)
            } else if result == FWP_E_IN_USE_CODE {
                (false, true)
            } else {
                ensure_success(result, "FwpmProviderDeleteByKey0")?;
                unreachable!()
            }
        } else {
            (false, true)
        };

    transaction.commit()?;
    Ok(WfpRetirementReport {
        verified_filter_count: verified_ids.len(),
        retired_filter_ids: verified_ids,
        sublayer_retired,
        sublayer_retained_shared,
        provider_retired,
        provider_retained_shared,
    })
}

pub(crate) fn remove_wfp_filters() -> Result<()> {
    // Leave time for other cleanup if a WFP policy writer holds the transaction lock.
    let engine = Engine::open(/*transaction_wait_timeout_ms*/ 1_000)?;
    let mut transaction = engine.begin_transaction()?;
    for spec in FILTER_SPECS {
        delete_filter_if_present(engine.handle, &spec.key)?;
    }
    for (result, operation, missing) in [
        (
            unsafe { FwpmSubLayerDeleteByKey0(engine.handle, &SUBLAYER_KEY) },
            "FwpmSubLayerDeleteByKey0",
            FWP_E_SUBLAYER_NOT_FOUND as u32,
        ),
        (
            unsafe { FwpmProviderDeleteByKey0(engine.handle, &PROVIDER_KEY) },
            "FwpmProviderDeleteByKey0",
            FWP_E_PROVIDER_NOT_FOUND as u32,
        ),
    ] {
        ensure_success_or(result, operation, &[missing, FWP_E_NOT_FOUND as u32])?;
    }
    transaction.commit()
}

/// Owns an open WFP engine handle and closes it on drop.
struct Engine {
    handle: HANDLE,
}

impl Engine {
    fn open(transaction_wait_timeout_ms: u32) -> Result<Self> {
        let session_name = to_wide(OsStr::new(SESSION_NAME));
        let mut session: FWPM_SESSION0 = unsafe { zeroed() };
        session.displayData = FWPM_DISPLAY_DATA0 {
            name: session_name.as_ptr() as *mut _,
            description: null_mut(),
        };
        session.txnWaitTimeoutInMSec = transaction_wait_timeout_ms;

        let mut handle = HANDLE::default();
        let result = unsafe {
            FwpmEngineOpen0(
                null(),
                RPC_C_AUTHN_DEFAULT as u32,
                null(),
                &session,
                &mut handle,
            )
        };
        ensure_success(result, "FwpmEngineOpen0")?;
        Ok(Self { handle })
    }

    fn begin_transaction(&self) -> Result<Transaction<'_>> {
        let result = unsafe { FwpmTransactionBegin0(self.handle, 0) };
        ensure_success(result, "FwpmTransactionBegin0")?;
        Ok(Transaction {
            engine: self,
            committed: false,
        })
    }
}

impl Drop for Engine {
    fn drop(&mut self) {
        unsafe {
            FwpmEngineClose0(self.handle);
        }
    }
}

/// Aborts an open WFP transaction unless it was explicitly committed.
struct Transaction<'a> {
    engine: &'a Engine,
    committed: bool,
}

impl Transaction<'_> {
    fn commit(&mut self) -> Result<()> {
        let result = unsafe { FwpmTransactionCommit0(self.engine.handle) };
        ensure_success(result, "FwpmTransactionCommit0")?;
        self.committed = true;
        Ok(())
    }
}

impl Drop for Transaction<'_> {
    fn drop(&mut self) {
        if !self.committed {
            unsafe {
                FwpmTransactionAbort0(self.engine.handle);
            }
        }
    }
}

/// Builds the ALE_USER_ID condition blob that scopes filters to one account.
struct UserMatchCondition {
    security_descriptor: PSECURITY_DESCRIPTOR,
    blob: FWP_BYTE_BLOB,
}

impl UserMatchCondition {
    fn for_account(account: &str) -> Result<Self> {
        let account_w = to_wide(OsStr::new(account));
        let mut access: EXPLICIT_ACCESS_W = unsafe { zeroed() };
        unsafe {
            BuildExplicitAccessWithNameW(
                &mut access,
                account_w.as_ptr(),
                FWP_ACTRL_MATCH_FILTER,
                GRANT_ACCESS,
                0,
            );
        }

        let mut security_descriptor: PSECURITY_DESCRIPTOR = null_mut();
        let mut security_descriptor_len = 0;
        let result = unsafe {
            BuildSecurityDescriptorW(
                null(),
                null(),
                1,
                &access,
                0,
                null(),
                null_mut(),
                &mut security_descriptor_len,
                &mut security_descriptor,
            )
        };
        ensure_success(result, "BuildSecurityDescriptorW")?;

        Ok(Self {
            security_descriptor,
            blob: FWP_BYTE_BLOB {
                size: security_descriptor_len,
                data: security_descriptor as *mut u8,
            },
        })
    }
}

impl Drop for UserMatchCondition {
    fn drop(&mut self) {
        if !self.security_descriptor.is_null() {
            unsafe {
                LocalFree(self.security_descriptor as HLOCAL);
            }
        }
    }
}

/// Ensures the persistent Codex WFP provider exists.
fn ensure_provider(engine: HANDLE) -> Result<()> {
    let provider_name = to_wide(OsStr::new(PROVIDER_NAME));
    let provider_description = to_wide(OsStr::new(PROVIDER_DESCRIPTION));
    let provider = FWPM_PROVIDER0 {
        providerKey: PROVIDER_KEY,
        displayData: FWPM_DISPLAY_DATA0 {
            name: provider_name.as_ptr() as *mut _,
            description: provider_description.as_ptr() as *mut _,
        },
        flags: FWPM_PROVIDER_FLAG_PERSISTENT,
        providerData: empty_blob(),
        serviceName: null_mut(),
    };

    let result = unsafe { FwpmProviderAdd0(engine, &provider, null_mut()) };
    ensure_success_or(result, "FwpmProviderAdd0", &[FWP_E_ALREADY_EXISTS as u32])
}

/// Checks whether the persistent Codex WFP provider already exists; if absent, creates it and
/// returns `true`. If present, verifies its persistent flag and returns `false` (shared).
fn ensure_or_observe_provider(engine: HANDLE) -> Result<bool> {
    let mut existing: *mut FWPM_PROVIDER0 = null_mut();
    let get_result = unsafe { FwpmProviderGetByKey0(engine, &PROVIDER_KEY, &mut existing) };
    if get_result == 0 {
        ensure!(
            !existing.is_null(),
            "FwpmProviderGetByKey0 returned null provider"
        );
        let flags = unsafe { (*existing).flags };
        let key = unsafe { WfpGuid::from((*existing).providerKey) };
        let mut raw: *mut c_void = existing.cast();
        unsafe { FwpmFreeMemory0(&mut raw) };
        ensure!(
            key == WfpGuid::from(PROVIDER_KEY) && (flags & FWPM_PROVIDER_FLAG_PERSISTENT) != 0,
            "existing WFP provider has unexpected key or non-persistent flags"
        );
        return Ok(false);
    }
    ensure_success_or(
        get_result,
        "FwpmProviderGetByKey0",
        &[FWP_E_PROVIDER_NOT_FOUND as u32, FWP_E_NOT_FOUND as u32],
    )?;

    let provider_name = to_wide(OsStr::new(PROVIDER_NAME));
    let provider_description = to_wide(OsStr::new(PROVIDER_DESCRIPTION));
    let provider = FWPM_PROVIDER0 {
        providerKey: PROVIDER_KEY,
        displayData: FWPM_DISPLAY_DATA0 {
            name: provider_name.as_ptr() as *mut _,
            description: provider_description.as_ptr() as *mut _,
        },
        flags: FWPM_PROVIDER_FLAG_PERSISTENT,
        providerData: empty_blob(),
        serviceName: null_mut(),
    };
    let add_result = unsafe { FwpmProviderAdd0(engine, &provider, null_mut()) };
    ensure_success(add_result, "FwpmProviderAdd0")?;
    Ok(true)
}

/// Ensures the persistent Codex sublayer exists under the Codex provider.
fn ensure_sublayer(engine: HANDLE) -> Result<()> {
    let sublayer_name = to_wide(OsStr::new(SUBLAYER_NAME));
    let sublayer_description = to_wide(OsStr::new(SUBLAYER_DESCRIPTION));
    let provider_key = PROVIDER_KEY;
    let sublayer = FWPM_SUBLAYER0 {
        subLayerKey: SUBLAYER_KEY,
        displayData: FWPM_DISPLAY_DATA0 {
            name: sublayer_name.as_ptr() as *mut _,
            description: sublayer_description.as_ptr() as *mut _,
        },
        flags: FWPM_SUBLAYER_FLAG_PERSISTENT,
        providerKey: &provider_key as *const _ as *mut _,
        providerData: empty_blob(),
        weight: 0x8000,
    };

    let result = unsafe { FwpmSubLayerAdd0(engine, &sublayer, null_mut()) };
    ensure_success_or(result, "FwpmSubLayerAdd0", &[FWP_E_ALREADY_EXISTS as u32])
}

/// Checks whether the persistent Codex sublayer already exists; if absent, creates it and
/// returns `true`. If present, verifies its persistent flag and provider link and returns `false`.
fn ensure_or_observe_sublayer(engine: HANDLE) -> Result<bool> {
    let mut existing: *mut FWPM_SUBLAYER0 = null_mut();
    let get_result = unsafe { FwpmSubLayerGetByKey0(engine, &SUBLAYER_KEY, &mut existing) };
    if get_result == 0 {
        ensure!(
            !existing.is_null(),
            "FwpmSubLayerGetByKey0 returned null sublayer"
        );
        let flags = unsafe { (*existing).flags };
        let key = unsafe { WfpGuid::from((*existing).subLayerKey) };
        let provider_key = unsafe {
            if (*existing).providerKey.is_null() {
                None
            } else {
                Some(WfpGuid::from(*(*existing).providerKey))
            }
        };
        let mut raw: *mut c_void = existing.cast();
        unsafe { FwpmFreeMemory0(&mut raw) };
        ensure!(
            key == WfpGuid::from(SUBLAYER_KEY)
                && (flags & FWPM_SUBLAYER_FLAG_PERSISTENT) != 0
                && provider_key == Some(WfpGuid::from(PROVIDER_KEY)),
            "existing WFP sublayer has unexpected key, flags, or provider link"
        );
        return Ok(false);
    }
    ensure_success_or(
        get_result,
        "FwpmSubLayerGetByKey0",
        &[FWP_E_SUBLAYER_NOT_FOUND as u32, FWP_E_NOT_FOUND as u32],
    )?;

    let sublayer_name = to_wide(OsStr::new(SUBLAYER_NAME));
    let sublayer_description = to_wide(OsStr::new(SUBLAYER_DESCRIPTION));
    let provider_key = PROVIDER_KEY;
    let sublayer = FWPM_SUBLAYER0 {
        subLayerKey: SUBLAYER_KEY,
        displayData: FWPM_DISPLAY_DATA0 {
            name: sublayer_name.as_ptr() as *mut _,
            description: sublayer_description.as_ptr() as *mut _,
        },
        flags: FWPM_SUBLAYER_FLAG_PERSISTENT,
        providerKey: &provider_key as *const _ as *mut _,
        providerData: empty_blob(),
        weight: 0x8000,
    };
    let add_result = unsafe { FwpmSubLayerAdd0(engine, &sublayer, null_mut()) };
    ensure_success(add_result, "FwpmSubLayerAdd0")?;
    Ok(true)
}

/// Adds one blocking WFP filter from the static filter spec list.
fn add_filter(
    engine: HANDLE,
    spec: &FilterSpec,
    user_condition: &UserMatchCondition,
) -> Result<()> {
    let _ = add_filter_returning_id(engine, spec, user_condition)?;
    Ok(())
}

/// Adds one blocking WFP filter and returns its runtime `filterId`.
fn add_filter_returning_id(
    engine: HANDLE,
    spec: &FilterSpec,
    user_condition: &UserMatchCondition,
) -> Result<u64> {
    let filter_name = to_wide(OsStr::new(spec.name));
    let filter_description = to_wide(OsStr::new(spec.description));
    let mut filter_conditions = build_conditions(spec.conditions, user_condition);
    let provider_key = PROVIDER_KEY;
    let filter = FWPM_FILTER0 {
        filterKey: spec.key,
        displayData: FWPM_DISPLAY_DATA0 {
            name: filter_name.as_ptr() as *mut _,
            description: filter_description.as_ptr() as *mut _,
        },
        flags: FWPM_FILTER_FLAG_PERSISTENT,
        providerKey: &provider_key as *const _ as *mut _,
        providerData: empty_blob(),
        layerKey: spec.layer_key,
        subLayerKey: SUBLAYER_KEY,
        weight: empty_value(),
        numFilterConditions: filter_conditions.len() as u32,
        filterCondition: filter_conditions.as_mut_ptr(),
        action: FWPM_ACTION0 {
            r#type: FWP_ACTION_BLOCK,
            Anonymous: FWPM_ACTION0_0 {
                filterType: zero_guid(),
            },
        },
        Anonymous: FWPM_FILTER0_0 { rawContext: 0 },
        reserved: null_mut(),
        filterId: 0,
        effectiveWeight: empty_value(),
    };

    let mut filter_id = 0_u64;
    let result = unsafe { FwpmFilterAdd0(engine, &filter, null_mut(), &mut filter_id) };
    ensure_success(result, &format!("FwpmFilterAdd0({})", spec.name))?;
    Ok(filter_id)
}

fn observe_wfp_filter(engine: HANDLE, key: &GUID) -> Result<Option<ObservedWfpFilter>> {
    let mut raw_filter: *mut FWPM_FILTER0 = null_mut();
    let result = unsafe { FwpmFilterGetByKey0(engine, key, &mut raw_filter) };
    if result == FWP_E_FILTER_NOT_FOUND as u32 || result == FWP_E_NOT_FOUND as u32 {
        return Ok(None);
    }
    ensure_success(result, "FwpmFilterGetByKey0")?;
    ensure!(
        !raw_filter.is_null(),
        "FwpmFilterGetByKey0 returned null filter pointer"
    );

    let parse_result = unsafe {
        let f = &*raw_filter;
        let provider_key = if f.providerKey.is_null() {
            None
        } else {
            Some(WfpGuid::from(*f.providerKey))
        };
        let count = f.numFilterConditions as usize;
        let mut conditions = Vec::with_capacity(count);
        if count > 0 {
            ensure!(
                !f.filterCondition.is_null(),
                "WFP filter has non-zero condition count with null condition pointer"
            );
            let slice = std::slice::from_raw_parts(f.filterCondition, count);
            for cond in slice {
                ensure!(
                    cond.matchType == FWP_MATCH_EQUAL,
                    "unexpected WFP condition matchType {}",
                    cond.matchType
                );
                let field_key = WfpGuid::from(cond.fieldKey);
                if field_key == WfpGuid::from(FWPM_CONDITION_ALE_USER_ID) {
                    ensure!(
                        cond.conditionValue.r#type == FWP_SECURITY_DESCRIPTOR_TYPE,
                        "ALE_USER_ID condition does not have security descriptor type"
                    );
                    let sd_blob = cond.conditionValue.Anonymous.sd;
                    ensure!(
                        !sd_blob.is_null() && !(*sd_blob).data.is_null() && (*sd_blob).size > 0,
                        "ALE_USER_ID condition has empty security descriptor blob"
                    );
                    let sd_bytes =
                        std::slice::from_raw_parts((*sd_blob).data, (*sd_blob).size as usize)
                            .to_vec();
                    conditions.push(OwnedWfpConditionSpec::UserSd(sd_bytes));
                } else if field_key == WfpGuid::from(FWPM_CONDITION_IP_PROTOCOL) {
                    ensure!(
                        cond.conditionValue.r#type == FWP_UINT8,
                        "IP_PROTOCOL condition does not have uint8 type"
                    );
                    conditions.push(OwnedWfpConditionSpec::Protocol(
                        cond.conditionValue.Anonymous.uint8,
                    ));
                } else if field_key == WfpGuid::from(FWPM_CONDITION_IP_REMOTE_PORT) {
                    ensure!(
                        cond.conditionValue.r#type == FWP_UINT16,
                        "IP_REMOTE_PORT condition does not have uint16 type"
                    );
                    conditions.push(OwnedWfpConditionSpec::RemotePort(
                        cond.conditionValue.Anonymous.uint16,
                    ));
                } else {
                    bail!("unexpected WFP filter condition fieldKey {field_key:?}");
                }
            }
        }
        Ok(ObservedWfpFilter {
            key: WfpGuid::from(f.filterKey),
            filter_id: f.filterId,
            layer_key: WfpGuid::from(f.layerKey),
            sublayer_key: WfpGuid::from(f.subLayerKey),
            provider_key,
            flags: f.flags,
            action_type: f.action.r#type,
            conditions,
        })
    };

    let mut raw: *mut c_void = raw_filter.cast();
    unsafe { FwpmFreeMemory0(&mut raw) };
    parse_result.map(Some)
}

fn observe_wfp_state(engine: HANDLE, proof: &OwnedWfpProof) -> Result<ObservedWfpState> {
    let provider_present_and_persistent = {
        let mut existing: *mut FWPM_PROVIDER0 = null_mut();
        let res = unsafe { FwpmProviderGetByKey0(engine, &PROVIDER_KEY, &mut existing) };
        if res == 0 && !existing.is_null() {
            let flags = unsafe { (*existing).flags };
            let key = unsafe { WfpGuid::from((*existing).providerKey) };
            let mut raw: *mut c_void = existing.cast();
            unsafe { FwpmFreeMemory0(&mut raw) };
            key == WfpGuid::from(PROVIDER_KEY) && (flags & FWPM_PROVIDER_FLAG_PERSISTENT) != 0
        } else if res == FWP_E_PROVIDER_NOT_FOUND as u32 || res == FWP_E_NOT_FOUND as u32 {
            false
        } else {
            ensure_success(res, "FwpmProviderGetByKey0")?;
            false
        }
    };

    let sublayer_present_and_linked = {
        let mut existing: *mut FWPM_SUBLAYER0 = null_mut();
        let res = unsafe { FwpmSubLayerGetByKey0(engine, &SUBLAYER_KEY, &mut existing) };
        if res == 0 && !existing.is_null() {
            let flags = unsafe { (*existing).flags };
            let key = unsafe { WfpGuid::from((*existing).subLayerKey) };
            let provider_key = unsafe {
                if (*existing).providerKey.is_null() {
                    None
                } else {
                    Some(WfpGuid::from(*(*existing).providerKey))
                }
            };
            let mut raw: *mut c_void = existing.cast();
            unsafe { FwpmFreeMemory0(&mut raw) };
            key == WfpGuid::from(SUBLAYER_KEY)
                && (flags & FWPM_SUBLAYER_FLAG_PERSISTENT) != 0
                && provider_key == Some(WfpGuid::from(PROVIDER_KEY))
        } else if res == FWP_E_SUBLAYER_NOT_FOUND as u32 || res == FWP_E_NOT_FOUND as u32 {
            false
        } else {
            ensure_success(res, "FwpmSubLayerGetByKey0")?;
            false
        }
    };

    let mut filters = Vec::with_capacity(proof.filters.len());
    for filter_proof in &proof.filters {
        let key: GUID = filter_proof.key.into();
        if let Some(observed) = observe_wfp_filter(engine, &key)? {
            filters.push(observed);
        }
    }

    Ok(ObservedWfpState {
        provider_present_and_persistent,
        sublayer_present_and_linked,
        filters,
    })
}

pub(crate) fn verify_owned_wfp_state(
    proof: &OwnedWfpProof,
    observed: &ObservedWfpState,
) -> Result<Vec<u64>> {
    ensure!(
        proof.provider_key == WfpGuid::from(PROVIDER_KEY)
            && proof.sublayer_key == WfpGuid::from(SUBLAYER_KEY),
        "owned WFP proof has invalid provider or sublayer key"
    );
    ensure!(
        !proof.offline_sid.is_empty() && !proof.offline_sid_bytes.is_empty(),
        "owned WFP proof is missing offline SID identity"
    );
    ensure!(
        proof.filters.len() == proof.profile.filter_count(),
        "owned WFP proof filter count {} does not match requested profile ({})",
        proof.filters.len(),
        proof.profile.filter_count()
    );
    ensure!(
        observed.provider_present_and_persistent,
        "WFP provider missing or no longer persistent; refusing scoped WFP retirement"
    );
    ensure!(
        observed.sublayer_present_and_linked,
        "WFP sublayer missing or no longer linked to provider; refusing scoped WFP retirement"
    );
    ensure!(
        observed.filters.len() == proof.filters.len(),
        "observed WFP filter count {} does not match owned proof count {}; refusing retirement",
        observed.filters.len(),
        proof.filters.len()
    );

    let mut verified_ids = Vec::with_capacity(proof.filters.len());
    for ((spec, filter_proof), actual) in proof.profile
        .specs()
        .zip(proof.filters.iter())
        .zip(observed.filters.iter())
    {
        ensure!(
            filter_proof.name == spec.name
                && filter_proof.key == WfpGuid::from(spec.key)
                && filter_proof.layer_key == WfpGuid::from(spec.layer_key)
                && filter_proof.sublayer_key == WfpGuid::from(SUBLAYER_KEY)
                && filter_proof.provider_key == WfpGuid::from(PROVIDER_KEY)
                && filter_proof.filter_id != 0
                && filter_proof.offline_sid_bytes == proof.offline_sid_bytes,
            "owned WFP filter proof for {} is internally inconsistent",
            spec.name
        );
        ensure!(
            actual.key == filter_proof.key,
            "WFP filter {} key mismatch",
            filter_proof.name
        );
        ensure!(
            actual.filter_id == filter_proof.filter_id,
            "WFP filter {} runtime filterId changed ({} != {}); object was replaced",
            filter_proof.name,
            actual.filter_id,
            filter_proof.filter_id
        );
        ensure!(
            actual.layer_key == filter_proof.layer_key
                && actual.sublayer_key == filter_proof.sublayer_key
                && actual.provider_key == Some(filter_proof.provider_key),
            "WFP filter {} layer/sublayer/provider association changed",
            filter_proof.name
        );
        ensure!(
            (actual.flags & FWPM_FILTER_FLAG_PERSISTENT) != 0
                && actual.action_type == FWP_ACTION_BLOCK,
            "WFP filter {} flags or block action changed",
            filter_proof.name
        );
        let expected_conditions =
            expected_conditions_for_spec(spec.conditions, &filter_proof.expected_sd_blob);
        ensure!(
            filter_proof.expected_conditions == expected_conditions
                && actual.conditions == filter_proof.expected_conditions,
            "WFP filter {} conditions do not match owned setup proof",
            filter_proof.name
        );
        let actual_sd = extract_single_user_sd(&actual.conditions)?;
        ensure!(
            actual_sd == filter_proof.expected_sd_blob,
            "WFP filter {} user security descriptor blob changed",
            filter_proof.name
        );
        let (mask, sid_bytes) = extract_wfp_sd_single_allowed_sid(&actual_sd)?;
        ensure!(
            mask == FWP_ACTRL_MATCH_FILTER && sid_bytes == proof.offline_sid_bytes,
            "WFP filter {} user security descriptor SID does not match owned offline SID",
            filter_proof.name
        );
        verified_ids.push(actual.filter_id);
    }

    Ok(verified_ids)
}

fn expected_conditions_for_spec(
    specs: &[ConditionSpec],
    user_sd: &[u8],
) -> Vec<OwnedWfpConditionSpec> {
    specs
        .iter()
        .map(|spec| match spec {
            ConditionSpec::User => OwnedWfpConditionSpec::UserSd(user_sd.to_vec()),
            ConditionSpec::Protocol(protocol) => OwnedWfpConditionSpec::Protocol(*protocol),
            ConditionSpec::RemotePort(port) => OwnedWfpConditionSpec::RemotePort(*port),
        })
        .collect()
}

fn extract_single_user_sd(conditions: &[OwnedWfpConditionSpec]) -> Result<Vec<u8>> {
    let mut found: Option<Vec<u8>> = None;
    for cond in conditions {
        if let OwnedWfpConditionSpec::UserSd(sd) = cond {
            ensure!(
                found.is_none(),
                "WFP filter has multiple ALE_USER_ID conditions"
            );
            found = Some(sd.clone());
        }
    }
    found.ok_or_else(|| anyhow::anyhow!("WFP filter is missing ALE_USER_ID condition"))
}

/// Parses a self-relative Windows `SECURITY_DESCRIPTOR` and verifies that its DACL contains
/// exactly one `ACCESS_ALLOWED_ACE` with `AceFlags == 0`, returning `(access_mask, sid_bytes)`.
pub(crate) fn extract_wfp_sd_single_allowed_sid(sd_bytes: &[u8]) -> Result<(u32, Vec<u8>)> {
    const SE_DACL_PRESENT: u16 = 0x0004;
    const SE_SELF_RELATIVE: u16 = 0x8000;
    const ACCESS_ALLOWED_ACE_TYPE: u8 = 0;

    ensure!(
        sd_bytes.len() >= 20,
        "WFP security descriptor shorter than 20-byte self-relative header"
    );
    let revision = sd_bytes[0];
    let control = u16::from_le_bytes([sd_bytes[2], sd_bytes[3]]);
    ensure!(
        revision == 1 && (control & SE_SELF_RELATIVE) != 0 && (control & SE_DACL_PRESENT) != 0,
        "WFP security descriptor is not a self-relative descriptor with DACL"
    );
    let dacl_offset = u32::from_le_bytes([sd_bytes[16], sd_bytes[17], sd_bytes[18], sd_bytes[19]])
        as usize;
    ensure!(
        dacl_offset >= 20 && dacl_offset + 8 <= sd_bytes.len(),
        "WFP security descriptor DACL offset out of bounds"
    );

    let acl = &sd_bytes[dacl_offset..];
    let acl_size = u16::from_le_bytes([acl[2], acl[3]]) as usize;
    let ace_count = u16::from_le_bytes([acl[4], acl[5]]) as usize;
    ensure!(
        acl_size >= 8 && acl_size <= acl.len() && ace_count == 1,
        "WFP security descriptor DACL must contain exactly 1 ACE (found {ace_count})"
    );

    let ace = &acl[8..acl_size];
    ensure!(
        ace.len() >= 16,
        "WFP security descriptor ACE shorter than minimum ACCESS_ALLOWED_ACE"
    );
    let ace_type = ace[0];
    let ace_flags = ace[1];
    let ace_size = u16::from_le_bytes([ace[2], ace[3]]) as usize;
    ensure!(
        ace_type == ACCESS_ALLOWED_ACE_TYPE && ace_flags == 0 && ace_size >= 16 && ace_size <= ace.len(),
        "WFP security descriptor ACE has unexpected type ({ace_type}), flags ({ace_flags}), or size ({ace_size})"
    );
    let mask = u32::from_le_bytes([ace[4], ace[5], ace[6], ace[7]]);
    let sid_slice = &ace[8..ace_size];
    let sid_revision = sid_slice[0];
    let sub_authority_count = sid_slice[1] as usize;
    let expected_sid_len = 8 + 4 * sub_authority_count;
    ensure!(
        sid_revision == 1
            && (1..=15).contains(&sub_authority_count)
            && expected_sid_len <= sid_slice.len(),
        "WFP security descriptor ACE contains invalid binary SID"
    );
    Ok((mask, sid_slice[..expected_sid_len].to_vec()))
}

/// Converts our compact condition specs into WFP filter conditions.
fn build_conditions(
    specs: &[ConditionSpec],
    user_condition: &UserMatchCondition,
) -> Vec<FWPM_FILTER_CONDITION0> {
    specs
        .iter()
        .map(|spec| match spec {
            ConditionSpec::User => FWPM_FILTER_CONDITION0 {
                fieldKey: FWPM_CONDITION_ALE_USER_ID,
                matchType: FWP_MATCH_EQUAL,
                conditionValue: FWP_CONDITION_VALUE0 {
                    r#type: FWP_SECURITY_DESCRIPTOR_TYPE,
                    Anonymous: FWP_CONDITION_VALUE0_0 {
                        sd: &user_condition.blob as *const _ as *mut _,
                    },
                },
            },
            ConditionSpec::Protocol(protocol) => FWPM_FILTER_CONDITION0 {
                fieldKey: FWPM_CONDITION_IP_PROTOCOL,
                matchType: FWP_MATCH_EQUAL,
                conditionValue: FWP_CONDITION_VALUE0 {
                    r#type: FWP_UINT8,
                    Anonymous: FWP_CONDITION_VALUE0_0 { uint8: *protocol },
                },
            },
            ConditionSpec::RemotePort(port) => FWPM_FILTER_CONDITION0 {
                fieldKey: FWPM_CONDITION_IP_REMOTE_PORT,
                matchType: FWP_MATCH_EQUAL,
                conditionValue: FWP_CONDITION_VALUE0 {
                    r#type: FWP_UINT16,
                    Anonymous: FWP_CONDITION_VALUE0_0 { uint16: *port },
                },
            },
        })
        .collect()
}

/// Deletes an old copy of a filter before re-adding it.
fn delete_filter_if_present(engine: HANDLE, key: &GUID) -> Result<()> {
    let result = unsafe { FwpmFilterDeleteByKey0(engine, key) };
    ensure_success_or(
        result,
        "FwpmFilterDeleteByKey0",
        &[FWP_E_FILTER_NOT_FOUND as u32, FWP_E_NOT_FOUND as u32],
    )
}

fn ensure_success(result: u32, operation: &str) -> Result<()> {
    ensure_success_or(result, operation, &[])
}

fn ensure_success_or(result: u32, operation: &str, allowed: &[u32]) -> Result<()> {
    if result == 0 || allowed.contains(&result) {
        Ok(())
    } else {
        Err(anyhow::anyhow!(
            "{operation} failed: {}",
            format_error_code(result)
        ))
    }
}

fn format_error_code(result: u32) -> String {
    format!("0x{result:08X}")
}

fn empty_blob() -> FWP_BYTE_BLOB {
    FWP_BYTE_BLOB {
        size: 0,
        data: null_mut(),
    }
}

fn empty_value() -> FWP_VALUE0 {
    FWP_VALUE0 {
        r#type: FWP_EMPTY,
        Anonymous: unsafe { zeroed() },
    }
}

fn zero_guid() -> GUID {
    GUID::from_u128(0)
}

#[cfg(test)]
mod tests {
    use super::FILTER_SPECS;
    use super::FWP_ACTION_BLOCK;
    use super::FWP_ACTRL_MATCH_FILTER;
    use super::FWPM_FILTER_FLAG_PERSISTENT;
    use super::ObservedWfpFilter;
    use super::ObservedWfpState;
    use super::OwnedWfpConditionSpec;
    use super::OwnedWfpFilterProof;
    use super::OwnedWfpProof;
    use super::OwnedWfpProfile;
    use super::PROVIDER_KEY;
    use super::SUBLAYER_KEY;
    use super::WfpGuid;
    use super::expected_conditions_for_spec;
    use super::extract_wfp_sd_single_allowed_sid;
    use super::verify_owned_wfp_state;
    use pretty_assertions::assert_eq;
    use std::collections::BTreeSet;

    fn synthetic_sid(rid: u32) -> Vec<u8> {
        let mut sid = vec![
            1, // Revision
            5, // SubAuthorityCount
            0, 0, 0, 0, 0, 5, // NT Authority
        ];
        for sub in [21_u32, 111, 222, 333, rid] {
            sid.extend_from_slice(&sub.to_le_bytes());
        }
        sid
    }

    fn synthetic_self_relative_sd(mask: u32, sid: &[u8]) -> Vec<u8> {
        let ace_size = (8 + sid.len()) as u16;
        let acl_size = 8 + ace_size;
        let mut sd = Vec::with_capacity(20 + acl_size as usize);
        sd.push(1); // Revision
        sd.push(0); // Sbz1
        sd.extend_from_slice(&(0x8004_u16).to_le_bytes()); // SE_SELF_RELATIVE | SE_DACL_PRESENT
        sd.extend_from_slice(&0_u32.to_le_bytes()); // Owner
        sd.extend_from_slice(&0_u32.to_le_bytes()); // Group
        sd.extend_from_slice(&0_u32.to_le_bytes()); // Sacl
        sd.extend_from_slice(&20_u32.to_le_bytes()); // Dacl offset
        // ACL header
        sd.push(2); // AclRevision
        sd.push(0); // Sbz1
        sd.extend_from_slice(&acl_size.to_le_bytes());
        sd.extend_from_slice(&1_u16.to_le_bytes()); // AceCount = 1
        sd.extend_from_slice(&0_u16.to_le_bytes()); // Sbz2
        // ACCESS_ALLOWED_ACE
        sd.push(0); // ACCESS_ALLOWED_ACE_TYPE
        sd.push(0); // AceFlags = 0
        sd.extend_from_slice(&ace_size.to_le_bytes());
        sd.extend_from_slice(&mask.to_le_bytes());
        sd.extend_from_slice(sid);
        sd
    }

    fn sample_owned_proof_and_state() -> (OwnedWfpProof, ObservedWfpState) {
        sample_profile_proof_and_state(OwnedWfpProfile::Supplemental)
    }

    fn sample_profile_proof_and_state(profile: OwnedWfpProfile) -> (OwnedWfpProof, ObservedWfpState) {
        let sid_bytes = synthetic_sid(1001);
        let sd_blob = synthetic_self_relative_sd(FWP_ACTRL_MATCH_FILTER, &sid_bytes);
        let mut proof_filters = Vec::new();
        let mut observed_filters = Vec::new();
        for (idx, spec) in profile.specs().enumerate() {
            let filter_id = 5000 + idx as u64;
            let expected_conditions = expected_conditions_for_spec(spec.conditions, &sd_blob);
            proof_filters.push(OwnedWfpFilterProof {
                name: spec.name,
                key: WfpGuid::from(spec.key),
                filter_id,
                layer_key: WfpGuid::from(spec.layer_key),
                sublayer_key: WfpGuid::from(SUBLAYER_KEY),
                provider_key: WfpGuid::from(PROVIDER_KEY),
                expected_conditions: expected_conditions.clone(),
                expected_sd_blob: sd_blob.clone(),
                offline_sid_bytes: sid_bytes.clone(),
            });
            observed_filters.push(ObservedWfpFilter {
                key: WfpGuid::from(spec.key),
                filter_id,
                layer_key: WfpGuid::from(spec.layer_key),
                sublayer_key: WfpGuid::from(SUBLAYER_KEY),
                provider_key: Some(WfpGuid::from(PROVIDER_KEY)),
                flags: FWPM_FILTER_FLAG_PERSISTENT,
                action_type: FWP_ACTION_BLOCK,
                conditions: expected_conditions,
            });
        }
        let proof = OwnedWfpProof {
            profile,
            provider_key: WfpGuid::from(PROVIDER_KEY),
            sublayer_key: WfpGuid::from(SUBLAYER_KEY),
            provider_created: true,
            sublayer_created: true,
            offline_sid: "S-1-5-21-111-222-333-1001".to_string(),
            offline_sid_bytes: sid_bytes,
            filters: proof_filters,
        };
        let state = ObservedWfpState {
            provider_present_and_persistent: true,
            sublayer_present_and_linked: true,
            filters: observed_filters,
        };
        (proof, state)
    }

    #[test]
    fn filter_keys_are_unique() {
        let keys = FILTER_SPECS
            .iter()
            .map(|spec| {
                (
                    spec.key.data1,
                    spec.key.data2,
                    spec.key.data3,
                    spec.key.data4,
                )
            })
            .collect::<BTreeSet<_>>();
        assert_eq!(keys.len(), FILTER_SPECS.len());
    }

    #[test]
    fn filter_names_are_unique() {
        let names = FILTER_SPECS
            .iter()
            .map(|spec| spec.name)
            .collect::<BTreeSet<_>>();
        assert_eq!(names.len(), FILTER_SPECS.len());
    }

    #[test]
    fn owned_wfp_verification_accepts_exact_owned_filters() -> anyhow::Result<()> {
        let (proof, state) = sample_owned_proof_and_state();
        let ids = verify_owned_wfp_state(&proof, &state)?;
        assert_eq!(ids.len(), FILTER_SPECS.len());
        assert_eq!(ids[0], 5000);
        let (mask, sid) = extract_wfp_sd_single_allowed_sid(&proof.filters[0].expected_sd_blob)?;
        assert_eq!(mask, FWP_ACTRL_MATCH_FILTER);
        assert_eq!(sid, proof.offline_sid_bytes);
        Ok(())
    }

    #[test]
    fn owned_wfp_verification_rejects_replaced_filter_id_or_foreign_sid() {
        let (proof, mut state) = sample_owned_proof_and_state();
        state.filters[3].filter_id += 1;
        assert!(verify_owned_wfp_state(&proof, &state).is_err());

        let (proof, mut state) = sample_owned_proof_and_state();
        let foreign_sid = synthetic_sid(9999);
        let foreign_sd = synthetic_self_relative_sd(FWP_ACTRL_MATCH_FILTER, &foreign_sid);
        state.filters[0].conditions[0] = OwnedWfpConditionSpec::UserSd(foreign_sd);
        assert!(verify_owned_wfp_state(&proof, &state).is_err());
    }

    #[test]
    fn owned_wfp_verification_rejects_missing_provider_sublayer_or_altered_condition() {
        let (proof, mut state) = sample_owned_proof_and_state();
        state.provider_present_and_persistent = false;
        assert!(verify_owned_wfp_state(&proof, &state).is_err());

        let (proof, mut state) = sample_owned_proof_and_state();
        state.sublayer_present_and_linked = false;
        assert!(verify_owned_wfp_state(&proof, &state).is_err());

        let (proof, mut state) = sample_owned_proof_and_state();
        state.filters[4].conditions[1] = OwnedWfpConditionSpec::RemotePort(5353);
        assert!(verify_owned_wfp_state(&proof, &state).is_err());
    }

    #[test]
    fn owned_wfp_verification_rejects_altered_action_type_and_bad_receipt_specs()
    -> anyhow::Result<()> {
        let (proof, mut state) = sample_owned_proof_and_state();
        state.filters[0].action_type = FWP_ACTION_BLOCK ^ 1;
        assert!(verify_owned_wfp_state(&proof, &state).is_err());

        let reported = proof
            .filters
            .iter()
            .map(|f| (f.name.to_string(), f.filter_id, f.expected_sd_blob.clone()))
            .collect::<Vec<_>>();
        let rebuilt = super::build_owned_wfp_proof_from_receipt_specs(
            proof.profile,
            proof.provider_created,
            proof.sublayer_created,
            &proof.offline_sid,
            &proof.offline_sid_bytes,
            &reported,
        )?;
        assert_eq!(rebuilt, proof);

        let mut zero_id_reported = reported.clone();
        zero_id_reported[0].1 = 0;
        assert!(
            super::build_owned_wfp_proof_from_receipt_specs(
                proof.profile,
                true,
                true,
                &proof.offline_sid,
                &proof.offline_sid_bytes,
                &zero_id_reported,
            )
            .is_err()
        );

        let foreign_sid = synthetic_sid(9999);
        assert!(
            super::build_owned_wfp_proof_from_receipt_specs(
                proof.profile,
                true,
                true,
                &proof.offline_sid,
                &foreign_sid,
                &reported,
            )
            .is_err()
        );
        Ok(())
    }

    #[test]
    fn full_offline_profile_is_selected_only_without_requested_network_exceptions() {
        for (ports, local_binding, expected) in [
            (&[][..], false, OwnedWfpProfile::FullOffline),
            (&[][..], true, OwnedWfpProfile::Supplemental),
            (&[8080][..], false, OwnedWfpProfile::Supplemental),
            (&[8080][..], true, OwnedWfpProfile::Supplemental),
        ] {
            assert_eq!(OwnedWfpProfile::for_network_policy(ports, local_binding), expected);
        }
    }

    #[test]
    fn full_offline_profile_covers_both_directions_and_address_families_for_only_the_user() {
        use windows_sys::Win32::NetworkManagement::WindowsFilteringPlatform::{
            FWPM_LAYER_ALE_AUTH_CONNECT_V4, FWPM_LAYER_ALE_AUTH_CONNECT_V6,
            FWPM_LAYER_ALE_AUTH_RECV_ACCEPT_V4, FWPM_LAYER_ALE_AUTH_RECV_ACCEPT_V6,
        };
        let (proof, state) = sample_profile_proof_and_state(OwnedWfpProfile::FullOffline);
        assert_eq!(verify_owned_wfp_state(&proof, &state).unwrap().len(), 16);
        let expected_layers = [FWPM_LAYER_ALE_AUTH_CONNECT_V4, FWPM_LAYER_ALE_AUTH_CONNECT_V6,
            FWPM_LAYER_ALE_AUTH_RECV_ACCEPT_V4, FWPM_LAYER_ALE_AUTH_RECV_ACCEPT_V6]
            .map(WfpGuid::from).into_iter().collect::<BTreeSet<_>>();
        let added = &proof.filters[12..];
        assert_eq!(added.iter().map(|f| f.layer_key).collect::<BTreeSet<_>>(), expected_layers);
        for filter in added {
            assert_eq!(filter.expected_conditions,
                vec![OwnedWfpConditionSpec::UserSd(filter.expected_sd_blob.clone())]);
            assert_eq!(extract_wfp_sd_single_allowed_sid(&filter.expected_sd_blob).unwrap().1,
                proof.offline_sid_bytes);
        }
        assert_eq!(proof.filters.iter().map(|f| f.key).collect::<BTreeSet<_>>().len(), 16);
        assert_eq!(proof.filters.iter().map(|f| f.name).collect::<BTreeSet<_>>().len(), 16);
    }

    #[test]
    fn full_offline_receipt_cannot_downgrade_or_select_the_requested_profile() {
        let (proof, _) = sample_profile_proof_and_state(OwnedWfpProfile::FullOffline);
        let reported = proof.filters.iter()
            .map(|f| (f.name.to_string(), f.filter_id, f.expected_sd_blob.clone()))
            .collect::<Vec<_>>();
        let bind = |profile, rows: &[(String, u64, Vec<u8>)]| {
            super::build_owned_wfp_proof_from_receipt_specs(profile, true, true,
                &proof.offline_sid, &proof.offline_sid_bytes, rows)
        };
        assert_eq!(bind(OwnedWfpProfile::FullOffline, &reported).unwrap(), proof);
        assert!(bind(OwnedWfpProfile::FullOffline, &reported[..12]).is_err());
        assert!(bind(OwnedWfpProfile::Supplemental, &reported).is_err());
        let mut swapped = reported.clone();
        swapped.swap(12, 13);
        assert!(bind(OwnedWfpProfile::FullOffline, &swapped).is_err());
        let mut duplicate = reported.clone();
        duplicate[15] = duplicate[12].clone();
        assert!(bind(OwnedWfpProfile::FullOffline, &duplicate).is_err());
        let mut extra = reported;
        extra.push(extra[12].clone());
        assert!(bind(OwnedWfpProfile::FullOffline, &extra).is_err());
    }

    #[test]
    fn full_offline_live_verification_rejects_narrowed_or_replaced_owned_rules() {
        for index in 12..16 {
            let (proof, mut state) = sample_profile_proof_and_state(OwnedWfpProfile::FullOffline);
            state.filters[index].conditions.push(OwnedWfpConditionSpec::RemotePort(443));
            assert!(verify_owned_wfp_state(&proof, &state).is_err());
            let (proof, mut state) = sample_profile_proof_and_state(OwnedWfpProfile::FullOffline);
            state.filters[index].filter_id += 100;
            assert!(verify_owned_wfp_state(&proof, &state).is_err());
            let (proof, mut state) = sample_profile_proof_and_state(OwnedWfpProfile::FullOffline);
            state.filters[index].conditions[0] = OwnedWfpConditionSpec::UserSd(
                synthetic_self_relative_sd(FWP_ACTRL_MATCH_FILTER, &synthetic_sid(9999)));
            assert!(verify_owned_wfp_state(&proof, &state).is_err());
            let (proof, mut state) = sample_profile_proof_and_state(OwnedWfpProfile::FullOffline);
            state.filters.remove(index);
            assert!(verify_owned_wfp_state(&proof, &state).is_err());
        }
    }
}
