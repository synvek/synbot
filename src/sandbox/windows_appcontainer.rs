// Windows AppContainer sandbox implementation
//
// AppContainer is "zero trust": network is disabled by default. Outbound access requires:
//   (1) Capability SIDs in SECURITY_CAPABILITIES at CreateProcess (INTERNET_CLIENT S-1-15-3-1,
//       INTERNET_CLIENT_SERVER S-1-15-3-2, etc.). See build_capabilities() and spawn path.
//   (2) Firewall + WFP permit for the AppContainer SID.
// If (1) is missing or SID conversion fails, the token has no network capability and
// connections fail even when (2) is present.
//
// If Capabilities and WFP permits are in place but outbound still fails, enable WFP
// packet-drop auditing and check Event 5152/5157 for the blocking Filter Run-Time ID.
// See docs/getting-started/appcontainer-network-troubleshooting.md.

#![cfg(target_os = "windows")]

use super::error::{Result, SandboxError};
use super::sandbox_trait::Sandbox;
use super::types::{
    ExecutionResult, HealthStatus, SandboxConfig, SandboxInfo, SandboxState, SandboxStatus,
};
use chrono::Utc;
use std::collections::HashMap;
use std::ffi::OsStr;
use std::os::windows::ffi::OsStrExt;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::ptr::null_mut;
use std::time::Duration;
use windows::core::{HRESULT, PCWSTR};
use windows::Win32::Foundation::{CloseHandle, HLOCAL, LocalFree};
use windows::Win32::System::Memory::{LocalAlloc, LMEM_ZEROINIT};
use windows::Win32::Security::Authorization::{
    BuildTrusteeWithSidW, ConvertSidToStringSidW, ConvertStringSidToSidW, GetNamedSecurityInfoW,
    SetEntriesInAclW, SetNamedSecurityInfoW, EXPLICIT_ACCESS_W, GRANT_ACCESS, SE_FILE_OBJECT,
    TRUSTEE_W,
};
use windows::Win32::Security::Isolation::{CreateAppContainerProfile, DeleteAppContainerProfile};
use windows::Win32::Security::{
    CopySid, FreeSid, GetLengthSid, PSID, SID_AND_ATTRIBUTES, SECURITY_CAPABILITIES,
    SUB_CONTAINERS_AND_OBJECTS_INHERIT,
};
use windows::Win32::Security::DACL_SECURITY_INFORMATION;
use windows::Win32::Storage::FileSystem::{FILE_GENERIC_READ, FILE_GENERIC_WRITE};
use windows::Win32::Security::ACL;
use windows::Win32::Security::PSECURITY_DESCRIPTOR;
use windows::Win32::Foundation::{ERROR_SUCCESS, SetHandleInformation, HANDLE_FLAG_INHERIT};
use windows::Win32::System::Console::{GetStdHandle, STD_ERROR_HANDLE, STD_INPUT_HANDLE, STD_OUTPUT_HANDLE};
use windows::Win32::System::Threading::{
    CreateProcessW, DeleteProcThreadAttributeList, GetExitCodeProcess,
    InitializeProcThreadAttributeList, LPPROC_THREAD_ATTRIBUTE_LIST, PROCESS_INFORMATION,
    STARTUPINFOEXW, STARTUPINFOW, UpdateProcThreadAttribute, WaitForSingleObject,
    EXTENDED_STARTUPINFO_PRESENT, PROC_THREAD_ATTRIBUTE_SECURITY_CAPABILITIES,
    STARTF_USESTDHANDLES,
};

/// Windows AppContainer capability
#[derive(Debug, Clone)]
pub struct Capability {
    pub name: String,
    pub sid: String,
}

/// Windows AppContainer sandbox implementation
pub struct WindowsAppContainerSandbox {
    config: SandboxConfig,
    capabilities: Vec<Capability>,
    status: SandboxStatus,
    profile_name: String,
    /// AppContainer package SID from CreateAppContainerProfile (freed in stop/drop).
    container_sid: Option<*mut std::ffi::c_void>,
    /// Firewall rule name for outbound allow (removed in stop()).
    firewall_rule_name: Option<String>,
}

// SAFETY: container_sid is only accessed from the thread that runs start/stop/spawn; we require single-threaded use for these operations.
unsafe impl Send for WindowsAppContainerSandbox {}
unsafe impl Sync for WindowsAppContainerSandbox {}

impl WindowsAppContainerSandbox {
    /// Create a new Windows AppContainer sandbox
    /// 
    /// # Arguments
    /// 
    /// * `config` - Sandbox configuration
    /// 
    /// # Returns
    /// 
    /// A new `WindowsAppContainerSandbox` instance
    /// 
    /// # Errors
    /// 
    /// Returns an error if:
    /// - The configuration is invalid
    /// - Capabilities cannot be built
    pub fn new(config: SandboxConfig) -> Result<Self> {
        let capabilities = Self::build_capabilities(&config)?;
        let profile_name = format!("SynBot.Sandbox.{}", config.sandbox_id);
        
        Ok(Self {
            status: SandboxStatus {
                sandbox_id: config.sandbox_id.clone(),
                state: SandboxState::Created,
                created_at: Utc::now(),
                started_at: None,
                stopped_at: None,
                error: None,
            },
            config,
            capabilities,
            profile_name,
            container_sid: None,
            firewall_rule_name: None,
        })
    }
    
    /// Build capabilities from configuration
    /// 
    /// Translates the sandbox configuration into Windows AppContainer capabilities.
    /// This includes:
    /// - Network capabilities (if network is enabled)
    /// - File system capabilities (based on allowed paths)
    /// 
    /// # Arguments
    /// 
    /// * `config` - Sandbox configuration
    /// 
    /// # Returns
    /// 
    /// A vector of capabilities
    fn build_capabilities(config: &SandboxConfig) -> Result<Vec<Capability>> {
        let mut capabilities = Vec::new();
        
        // Add network capability if enabled. PRIVATE_NETWORK_CLIENT_SERVER is required for
        // system DNS resolver / local network (getaddrinfo often uses local DNS path);
        // INTERNET_CLIENT alone can still yield WSAHOST_NOT_FOUND in AppContainer.
        if config.network.enabled {
            capabilities.push(Capability {
                name: "internetClient".to_string(),
                sid: "S-1-15-3-1".to_string(), // SECURITY_CAPABILITY_INTERNET_CLIENT
            });
            capabilities.push(Capability {
                name: "privateNetworkClientServer".to_string(),
                sid: "S-1-15-3-3".to_string(), // SECURITY_CAPABILITY_PRIVATE_NETWORK_CLIENT_SERVER
            });
            capabilities.push(Capability {
                name: "internetClientServer".to_string(),
                sid: "S-1-15-3-2".to_string(), // SECURITY_CAPABILITY_INTERNET_CLIENT_SERVER
            });
        }
        
        // File system access is controlled through AppContainer's file system isolation
        // We grant access to writable paths through security descriptors
        // Readonly paths are accessible by default with read-only permissions
        // Hidden paths are blocked by not granting any access
        
        // Add document library capability for file access
        if !config.filesystem.writable_paths.is_empty() || !config.filesystem.readonly_paths.is_empty() {
            capabilities.push(Capability {
                name: "documentsLibrary".to_string(),
                sid: "S-1-15-3-12".to_string(), // SECURITY_CAPABILITY_DOCUMENTS_LIBRARY
            });
        }
        
        Ok(capabilities)
    }
    
    /// Convert capabilities to Windows SID_AND_ATTRIBUTES array
    fn capabilities_to_sid_and_attributes(&self) -> Result<Vec<String>> {
        // Simplified implementation that returns capability names
        // In a full implementation, this would convert to actual Windows SID structures
        Ok(self.capabilities.iter().map(|c| c.name.clone()).collect())
    }
}

/// Convert a Rust string to null-terminated wide string (caller must free with drop or LocalFree).
fn to_wide_null(s: &str) -> Vec<u16> {
    OsStr::new(s).encode_wide().chain(std::iter::once(0)).collect()
}

/// Convert a Windows PSID to string form S-R-I-S-S using the system API (for firewall LocalAppPackageId).
unsafe fn sid_to_string(sid: *mut std::ffi::c_void) -> Option<String> {
    if sid.is_null() {
        return None;
    }
    use windows::core::PWSTR;
    let mut ptr = PWSTR::null();
    ConvertSidToStringSidW(PSID(sid), &mut ptr).ok()?;
    if ptr.0.is_null() {
        return None;
    }
    let mut len = 0usize;
    while len < 256 && *ptr.0.add(len) != 0 {
        len += 1;
    }
    let slice = std::slice::from_raw_parts(ptr.0, len);
    let s = String::from_utf16_lossy(slice);
    let _ = LocalFree(HLOCAL(ptr.0 as *mut _));
    Some(s)
}

/// Log current process token's Integrity Level and AppContainer SID to stderr (for diagnostics when running inside AppContainer).
/// Call this from the child process after "daemon starting" when SYNBOT_IN_APP_SANDBOX is set.
/// Layout matches TOKEN_MANDATORY_LABEL for TokenIntegrityLevel.
#[repr(C)]
struct TokenMandatoryLabelLayout {
    label: SID_AND_ATTRIBUTES,
}

pub fn log_process_token_appcontainer_diagnostic() {
    use std::io::Write;
    use windows::Win32::Foundation::HANDLE;
    use windows::Win32::Security::{GetTokenInformation, GetSidSubAuthority, GetSidSubAuthorityCount, TokenIntegrityLevel, TOKEN_QUERY};
    use windows::Win32::System::Threading::{GetCurrentProcess, OpenProcessToken};

    let _ = writeln!(std::io::stderr(), "[synbot] AppContainer diagnostic: checking process token...");
    let _ = std::io::stderr().flush();

    unsafe {
        let process = GetCurrentProcess();
        let mut token = HANDLE::default();
        if OpenProcessToken(process, TOKEN_QUERY, &mut token).is_err() {
            let _ = writeln!(std::io::stderr(), "[synbot] AppContainer diagnostic: OpenProcessToken failed");
            let _ = std::io::stderr().flush();
            return;
        }

        // TokenIntegrityLevel -> TOKEN_MANDATORY_LABEL
        let mut ret_len = 0u32;
        let _ = GetTokenInformation(token, TokenIntegrityLevel, None, 0, &mut ret_len);
        let mut buf = vec![0u8; ret_len as usize];
        if GetTokenInformation(
            token,
            TokenIntegrityLevel,
            Some(buf.as_mut_ptr() as *mut _),
            ret_len,
            &mut ret_len,
        )
        .is_ok()
        {
            let label = &*(buf.as_ptr() as *const TokenMandatoryLabelLayout);
            let sid = label.label.Sid;
            if !sid.0.is_null() {
                let auth_count = *GetSidSubAuthorityCount(sid);
                let count = auth_count as usize;
                if count >= 1 {
                    let last = *GetSidSubAuthority(sid, (count - 1) as u32);
                    // Windows mandatory label uses 0x1000=Low, 0x2000=Medium, 0x3000=High, 0x4000=System (and 0=Untrusted).
                    let level_name = match last {
                        0 => "Untrusted",
                        0x1000 => "Low",
                        0x2000 => "Medium",
                        0x3000 => "High",
                        0x4000 => "System",
                        _ => "Unknown",
                    };
                    let _ = writeln!(std::io::stderr(), "[synbot] AppContainer diagnostic: IntegrityLevel={} (0x{:X}, {})", last, last, level_name);
                }
            }
        } else {
            let _ = writeln!(std::io::stderr(), "[synbot] AppContainer diagnostic: GetTokenInformation(TokenIntegrityLevel) failed");
        }

        // TokenAppContainerSid -> TOKEN_APPCONTAINER_INFORMATION (TokenAppContainer: PSID only in Win8+; layout is single PSID)
        // Windows: TOKEN_APPCONTAINER_INFORMATION has TokenAppContainer (PSID). So we get a pointer to a struct with one PSID.
        let mut ret_len = 0u32;
        let _ = GetTokenInformation(
            token,
            windows::Win32::Security::TokenAppContainerSid,
            None,
            0,
            &mut ret_len,
        );
        let mut buf = vec![0u8; ret_len as usize];
        if GetTokenInformation(
            token,
            windows::Win32::Security::TokenAppContainerSid,
            Some(buf.as_mut_ptr() as *mut _),
            ret_len,
            &mut ret_len,
        )
        .is_ok()
        {
            // TOKEN_APPCONTAINER_INFORMATION: TokenAppContainer: PSID (one pointer)
            #[repr(C)]
            struct TokenAppContainerInfo {
                token_app_container_sid: *mut std::ffi::c_void,
            }
            let info = &*(buf.as_ptr() as *const TokenAppContainerInfo);
            let sid_ptr = info.token_app_container_sid;
            let is_app_container = !sid_ptr.is_null();
            let _ = writeln!(std::io::stderr(), "[synbot] AppContainer diagnostic: IsAppContainer={}", is_app_container);
            if is_app_container {
                if let Some(sid_str) = sid_to_string(sid_ptr) {
                    let _ = writeln!(std::io::stderr(), "[synbot] AppContainer diagnostic: AppContainerSid={}", sid_str);
                } else {
                    let _ = writeln!(std::io::stderr(), "[synbot] AppContainer diagnostic: AppContainerSid=(non-null but ConvertSidToStringSid failed)");
                }
            }
        } else {
            let _ = writeln!(std::io::stderr(), "[synbot] AppContainer diagnostic: GetTokenInformation(TokenAppContainerSid) failed");
        }

        let _ = CloseHandle(token);
        let _ = std::io::stderr().flush();
    }
}

/// Add a Windows Firewall outbound allow rule for the given AppContainer SID so that
/// the container can make outbound TCP/UDP (e.g. HTTPS) connections.
fn add_firewall_outbound_rule_for_appcontainer(sid_string: &str, rule_name: &str) -> Result<()> {
    use windows::core::BSTR;
    use windows::Win32::Foundation::VARIANT_TRUE;
    use windows::Win32::NetworkManagement::WindowsFirewall::{
        INetFwPolicy2, INetFwRule3, NetFwPolicy2, NetFwRule, NET_FW_ACTION_ALLOW, NET_FW_PROFILE2_ALL,
        NET_FW_RULE_DIR_OUT,
    };
    use windows::Win32::System::Com::{CoCreateInstance, CoInitializeEx, CLSCTX_INPROC_SERVER, COINIT_APARTMENTTHREADED};

    unsafe {
        let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
        let policy: INetFwPolicy2 = CoCreateInstance(&NetFwPolicy2, None, CLSCTX_INPROC_SERVER)
            .map_err(|e| SandboxError::CreationFailed(format!("CoCreateInstance(NetFwPolicy2): {}", e)))?;
        let rules = policy.Rules()
            .map_err(|e| SandboxError::CreationFailed(format!("INetFwPolicy2::Rules: {}", e)))?;
        let rule: INetFwRule3 = CoCreateInstance(&NetFwRule, None, CLSCTX_INPROC_SERVER)
            .map_err(|e| SandboxError::CreationFailed(format!("CoCreateInstance(NetFwRule): {}", e)))?;
        let name_bstr = BSTR::from(rule_name);
        rule.SetName(&name_bstr)
            .map_err(|e| SandboxError::CreationFailed(format!("SetName: {}", e)))?;
        rule.SetDirection(NET_FW_RULE_DIR_OUT)
            .map_err(|e| SandboxError::CreationFailed(format!("SetDirection: {}", e)))?;
        rule.SetAction(NET_FW_ACTION_ALLOW)
            .map_err(|e| SandboxError::CreationFailed(format!("SetAction: {}", e)))?;
        let sid_bstr = BSTR::from(sid_string);
        rule.SetLocalAppPackageId(&sid_bstr)
            .map_err(|e| SandboxError::CreationFailed(format!("SetLocalAppPackageId: {}", e)))?;
        rule.SetEnabled(VARIANT_TRUE)
            .map_err(|e| SandboxError::CreationFailed(format!("SetEnabled: {}", e)))?;
        // Apply rule to all firewall profiles (domain, private, public) so it is active.
        rule.SetProfiles(NET_FW_PROFILE2_ALL.0)
            .map_err(|e| SandboxError::CreationFailed(format!("SetProfiles: {}", e)))?;
        rules.Add(&rule)
            .map_err(|e| SandboxError::CreationFailed(format!("Rules.Add: {}", e)))?;
    }
    Ok(())
}

/// Maps Win32 error code from FwpmEngineOpen0 to a short description for logging/troubleshooting.
fn wfp_engine_open_error_message(err: u32) -> String {
    // 5 = ERROR_ACCESS_DENIED, 50 = ERROR_NOT_SUPPORTED (often when not elevated)
    match err {
        5 => "Access denied (5). Run as Administrator.".to_string(),
        50 => "Not supported (50). Usually means not running as Administrator.".to_string(),
        other => format!("Error code {}. Try running as Administrator.", other),
    }
}

/// WFP error: object with this key already exists (e.g. from a previous run that did not clean up).
const FWP_E_ALREADY_EXISTS: u32 = 0x8032_0009;

/// Frees HLOCAL on drop (used for SID copy buffer in add_wfp_permit_for_appcontainer).
struct SidCopyGuard(HLOCAL);
impl Drop for SidCopyGuard {
    fn drop(&mut self) {
        if !self.0.0.is_null() {
            unsafe { let _ = LocalFree(self.0); }
        }
    }
}

/// Add WFP permit filters for the AppContainer SID so outbound TCP is allowed (high-priority sublayer + CLEAR_ACTION_RIGHT).
/// Idempotent: if provider/sublayer/filters already exist (e.g. leftover from crash), treats as success.
/// Cleanup in stop() uses delete-by-key (no stored IDs needed).
fn add_wfp_permit_for_appcontainer(container_sid: *mut std::ffi::c_void) -> Result<()> {
    use windows::Win32::NetworkManagement::WindowsFilteringPlatform::{
        FwpmEngineClose0, FwpmEngineOpen0, FwpmFilterAdd0, FwpmFilterDeleteById0,
        FwpmProviderAdd0, FwpmProviderDeleteByKey0, FwpmSubLayerAdd0, FwpmSubLayerDeleteByKey0,
        FWPM_LAYER_ALE_AUTH_CONNECT_V4, FWPM_LAYER_ALE_AUTH_CONNECT_V6,
        FWPM_LAYER_ALE_FLOW_ESTABLISHED_V4, FWPM_LAYER_ALE_FLOW_ESTABLISHED_V6,
        FWPM_CONDITION_ALE_PACKAGE_ID, FWPM_FILTER0, FWPM_FILTER_CONDITION0, FWPM_FILTER_FLAG_CLEAR_ACTION_RIGHT,
        FWPM_PROVIDER0, FWPM_SUBLAYER0, FWP_ACTION_PERMIT, FWP_CONDITION_VALUE0, FWP_CONDITION_VALUE0_0,
        FWP_MATCH_EQUAL, FWP_SID, FWP_VALUE0, FWP_VALUE0_0, FWP_EMPTY, FWPM_ACTION0, FWPM_ACTION0_0,
        FWPM_DISPLAY_DATA0, FWP_BYTE_BLOB,
    };
    use windows::Win32::Foundation::HANDLE;
    use windows::core::GUID;

    const ERROR_SUCCESS: u32 = 0;
    // FwpmEngineOpen0 only accepts RPC_C_AUTHN_WINNT (10) or RPC_C_AUTHN_DEFAULT (0xffffffff). Using 0 (RPC_C_AUTHN_NONE) returns ERROR_NOT_SUPPORTED (50).
    const RPC_C_AUTHN_WINNT: u32 = 10;

    // Fixed GUIDs for our provider and sublayer (weight 0 = highest priority).
    let provider_key = GUID::from_u128(0x5b8a1c2d_3e4f_5a6b_7c8d_9e0f1a2b3c4d);
    let sublayer_key = GUID::from_u128(0x6c9b2d3e_4f50_6b7c_8d9e_0f1a2b3c4d5e);
    let filter_key_v4 = GUID::from_u128(0x7d0c3e4f_5061_7c8d_9e0f_1a2b3c4d5e6f);
    let filter_key_v6 = GUID::from_u128(0x8e1d4f50_6172_8d9e_0f1a_2b3c4d5e6f70);
    let filter_flow_v4_key = GUID::from_u128(0x9f2e5f61_7283_9e0f_1a2b_3c4d5e6f7081);
    let filter_flow_v6_key = GUID::from_u128(0xa03f6072_8394_0f1a_2b3c_4d5e6f708192);
    // Inbound accept layers (allow AppContainer to receive connections).
    let filter_recv_v4_key = GUID::from_u128(0xb14f7183_9405_1f2b_3c4d_5e6f70819203);
    let filter_recv_v6_key = GUID::from_u128(0xc25f8294_a516_2f3c_4d5e_6f7081920314);

    unsafe {
        // WFP requires non-null displayData.name (FWP_E_NULL_DISPLAY_NAME); keep buffers alive for the whole block.
        let provider_name = to_wide_null("SynBot WFP Provider");
        let sublayer_name = to_wide_null("SynBot WFP Sublayer");
        let filter_v4_name = to_wide_null("SynBot AppContainer Outbound V4");
        let filter_v6_name = to_wide_null("SynBot AppContainer Outbound V6");
        let filter_flow_v4_name = to_wide_null("SynBot AppContainer Flow V4");
        let filter_flow_v6_name = to_wide_null("SynBot AppContainer Flow V6");
        let filter_recv_v4_name = to_wide_null("SynBot AppContainer Inbound V4");
        let filter_recv_v6_name = to_wide_null("SynBot AppContainer Inbound V6");

        let mut engine: HANDLE = HANDLE::default();
        let err = FwpmEngineOpen0(
            None,
            RPC_C_AUTHN_WINNT,
            None,
            None,
            &mut engine,
        );
        if err != ERROR_SUCCESS {
            return Err(SandboxError::CreationFailed(format!(
                "FwpmEngineOpen0: {}",
                wfp_engine_open_error_message(err)
            )));
        }

        let provider = FWPM_PROVIDER0 {
            providerKey: provider_key,
            displayData: FWPM_DISPLAY_DATA0 {
                name: windows::core::PWSTR(provider_name.as_ptr() as *mut u16),
                description: windows::core::PWSTR::null(),
            },
            flags: 0,
            providerData: FWP_BYTE_BLOB::default(),
            serviceName: windows::core::PWSTR::null(),
        };
        let prov_err = FwpmProviderAdd0(engine, &provider, None);
        let provider_added_this_run = prov_err == ERROR_SUCCESS;
        if prov_err != ERROR_SUCCESS && prov_err != FWP_E_ALREADY_EXISTS {
            let _ = FwpmEngineClose0(engine);
            return Err(SandboxError::CreationFailed(format!(
                "FwpmProviderAdd0 failed (error {})",
                prov_err
            )));
        }

        let sublayer = FWPM_SUBLAYER0 {
            subLayerKey: sublayer_key,
            displayData: FWPM_DISPLAY_DATA0 {
                name: windows::core::PWSTR(sublayer_name.as_ptr() as *mut u16),
                description: windows::core::PWSTR::null(),
            },
            flags: 0,
            providerKey: &provider_key as *const _ as *mut _,
            providerData: FWP_BYTE_BLOB::default(),
            weight: 0,
        };
        let sub_err = FwpmSubLayerAdd0(engine, &sublayer, None);
        if sub_err != ERROR_SUCCESS && sub_err != FWP_E_ALREADY_EXISTS {
            if provider_added_this_run {
                let _ = FwpmProviderDeleteByKey0(engine, &provider_key);
            }
            let _ = FwpmEngineClose0(engine);
            return Err(SandboxError::CreationFailed(format!(
                "FwpmSubLayerAdd0 failed (error {})",
                sub_err
            )));
        }

        // Copy SID to a buffer so BFE can read it during FwpmFilterAdd0 RPC (pointer in condition may not be marshalled).
        let sid_len = GetLengthSid(PSID(container_sid));
        let sid_copy = match LocalAlloc(LMEM_ZEROINIT, sid_len as usize) {
            Ok(h) if !h.0.is_null() => h,
            _ => {
                if provider_added_this_run {
                    let _ = FwpmSubLayerDeleteByKey0(engine, &sublayer_key);
                    let _ = FwpmProviderDeleteByKey0(engine, &provider_key);
                }
                let _ = FwpmEngineClose0(engine);
                return Err(SandboxError::CreationFailed("LocalAlloc for SID copy failed".to_string()));
            }
        };
        if CopySid(sid_len, PSID(sid_copy.0 as *mut _), PSID(container_sid)).is_err() {
            let _ = LocalFree(sid_copy);
            if provider_added_this_run {
                let _ = FwpmSubLayerDeleteByKey0(engine, &sublayer_key);
                let _ = FwpmProviderDeleteByKey0(engine, &provider_key);
            }
            let _ = FwpmEngineClose0(engine);
            return Err(SandboxError::CreationFailed("CopySid failed".to_string()));
        }
        let sid_ptr = sid_copy.0 as *mut _;
        // Free SID copy after all FwpmFilterAdd0 calls (buffer must stay valid during RPC so BFE can read it).
        let _sid_guard = SidCopyGuard(sid_copy);

        let cond_value = FWP_CONDITION_VALUE0 {
            r#type: FWP_SID,
            Anonymous: FWP_CONDITION_VALUE0_0 { sid: sid_ptr },
        };
        let mut condition = FWPM_FILTER_CONDITION0 {
            fieldKey: FWPM_CONDITION_ALE_PACKAGE_ID,
            matchType: FWP_MATCH_EQUAL,
            conditionValue: cond_value,
        };
        let weight = FWP_VALUE0 { r#type: FWP_EMPTY, Anonymous: FWP_VALUE0_0::default() };
        let action = FWPM_ACTION0 { r#type: FWP_ACTION_PERMIT, Anonymous: FWPM_ACTION0_0::default() };

        let mut filter_v4 = FWPM_FILTER0 {
            filterKey: filter_key_v4,
            displayData: FWPM_DISPLAY_DATA0 {
                name: windows::core::PWSTR(filter_v4_name.as_ptr() as *mut u16),
                description: windows::core::PWSTR::null(),
            },
            flags: FWPM_FILTER_FLAG_CLEAR_ACTION_RIGHT,
            providerKey: std::ptr::null_mut(),
            providerData: FWP_BYTE_BLOB::default(),
            layerKey: FWPM_LAYER_ALE_AUTH_CONNECT_V4,
            subLayerKey: sublayer_key,
            weight,
            numFilterConditions: 1,
            filterCondition: &mut condition as *mut _,
            action,
            Anonymous: windows::Win32::NetworkManagement::WindowsFilteringPlatform::FWPM_FILTER0_0::default(),
            reserved: std::ptr::null_mut(),
            filterId: 0,
            effectiveWeight: FWP_VALUE0::default(),
        };

        let mut id_v4: u64 = 0;
        let add4 = FwpmFilterAdd0(engine, &filter_v4, None, Some(&mut id_v4));
        if add4 != ERROR_SUCCESS && add4 != FWP_E_ALREADY_EXISTS {
            if provider_added_this_run {
                let _ = FwpmSubLayerDeleteByKey0(engine, &sublayer_key);
                let _ = FwpmProviderDeleteByKey0(engine, &provider_key);
            }
            let _ = FwpmEngineClose0(engine);
            return Err(SandboxError::CreationFailed(format!(
                "FwpmFilterAdd0 (V4) failed (error {})",
                add4
            )));
        }
        if add4 == FWP_E_ALREADY_EXISTS {
            // Filter already present (e.g. leftover); id_v4 not set. We'll clean by key in remove_wfp_permit_filters.
            id_v4 = 0;
        }

        let mut filter_v6 = FWPM_FILTER0 {
            filterKey: filter_key_v6,
            layerKey: FWPM_LAYER_ALE_AUTH_CONNECT_V6,
            ..filter_v4
        };
        filter_v6.displayData.name = windows::core::PWSTR(filter_v6_name.as_ptr() as *mut u16);
        filter_v6.filterCondition = &mut condition as *mut _;
        let mut id_v6: u64 = 0;
        let add6 = FwpmFilterAdd0(engine, &filter_v6, None, Some(&mut id_v6));
        if add6 != ERROR_SUCCESS && add6 != FWP_E_ALREADY_EXISTS {
            if id_v4 != 0 {
                let _ = FwpmFilterDeleteById0(engine, id_v4);
            }
            if provider_added_this_run {
                let _ = FwpmSubLayerDeleteByKey0(engine, &sublayer_key);
                let _ = FwpmProviderDeleteByKey0(engine, &provider_key);
            }
            let _ = FwpmEngineClose0(engine);
            return Err(SandboxError::CreationFailed(format!(
                "FwpmFilterAdd0 (V6) failed (error {})",
                add6
            )));
        }
        if add6 == FWP_E_ALREADY_EXISTS {
            id_v6 = 0;
        }

        // Also permit at ALE_FLOW_ESTABLISHED so reauthorization / flow tracking allows the connection.
        let mut filter_flow_v4 = FWPM_FILTER0 {
            filterKey: filter_flow_v4_key,
            displayData: FWPM_DISPLAY_DATA0 {
                name: windows::core::PWSTR(filter_flow_v4_name.as_ptr() as *mut u16),
                description: windows::core::PWSTR::null(),
            },
            layerKey: FWPM_LAYER_ALE_FLOW_ESTABLISHED_V4,
            ..filter_v4
        };
        filter_flow_v4.filterCondition = &mut condition as *mut _;
        let _ = FwpmFilterAdd0(engine, &filter_flow_v4, None, None as Option<*mut u64>);

        let mut filter_flow_v6 = FWPM_FILTER0 {
            filterKey: filter_flow_v6_key,
            displayData: FWPM_DISPLAY_DATA0 {
                name: windows::core::PWSTR(filter_flow_v6_name.as_ptr() as *mut u16),
                description: windows::core::PWSTR::null(),
            },
            layerKey: FWPM_LAYER_ALE_FLOW_ESTABLISHED_V6,
            ..filter_v4
        };
        filter_flow_v6.filterCondition = &mut condition as *mut _;
        let _ = FwpmFilterAdd0(engine, &filter_flow_v6, None, None as Option<*mut u64>);

        // Permit inbound connections to the AppContainer (ALE_AUTH_RECV_ACCEPT).
        // Without this, Windows blocks all inbound connections to AppContainer processes
        // even when a firewall allow rule exists.
        use windows::Win32::NetworkManagement::WindowsFilteringPlatform::{
            FWPM_LAYER_ALE_AUTH_RECV_ACCEPT_V4, FWPM_LAYER_ALE_AUTH_RECV_ACCEPT_V6,
        };
        let mut filter_recv_v4 = FWPM_FILTER0 {
            filterKey: filter_recv_v4_key,
            displayData: FWPM_DISPLAY_DATA0 {
                name: windows::core::PWSTR(filter_recv_v4_name.as_ptr() as *mut u16),
                description: windows::core::PWSTR::null(),
            },
            layerKey: FWPM_LAYER_ALE_AUTH_RECV_ACCEPT_V4,
            ..filter_v4
        };
        filter_recv_v4.filterCondition = &mut condition as *mut _;
        let _ = FwpmFilterAdd0(engine, &filter_recv_v4, None, None as Option<*mut u64>);

        let mut filter_recv_v6 = FWPM_FILTER0 {
            filterKey: filter_recv_v6_key,
            displayData: FWPM_DISPLAY_DATA0 {
                name: windows::core::PWSTR(filter_recv_v6_name.as_ptr() as *mut u16),
                description: windows::core::PWSTR::null(),
            },
            layerKey: FWPM_LAYER_ALE_AUTH_RECV_ACCEPT_V6,
            ..filter_v4
        };
        filter_recv_v6.filterCondition = &mut condition as *mut _;
        let _ = FwpmFilterAdd0(engine, &filter_recv_v6, None, None as Option<*mut u64>);

        let _ = FwpmEngineClose0(engine);
        Ok(())
    }
}

/// Remove WFP permit filters and our sublayer/provider by key (idempotent; works even when filter IDs were not stored).
fn remove_wfp_permit_filters() {
    use windows::Win32::NetworkManagement::WindowsFilteringPlatform::{
        FwpmEngineClose0, FwpmEngineOpen0, FwpmFilterDeleteByKey0,
        FwpmProviderDeleteByKey0, FwpmSubLayerDeleteByKey0,
    };
    use windows::Win32::Foundation::HANDLE;
    use windows::core::GUID;

    const ERROR_SUCCESS: u32 = 0;
    const RPC_C_AUTHN_WINNT: u32 = 10;
    let provider_key = GUID::from_u128(0x5b8a1c2d_3e4f_5a6b_7c8d_9e0f1a2b3c4d);
    let sublayer_key = GUID::from_u128(0x6c9b2d3e_4f50_6b7c_8d9e_0f1a2b3c4d5e);
    let filter_key_v4 = GUID::from_u128(0x7d0c3e4f_5061_7c8d_9e0f_1a2b3c4d5e6f);
    let filter_key_v6 = GUID::from_u128(0x8e1d4f50_6172_8d9e_0f1a_2b3c4d5e6f70);
    let filter_flow_v4_key = GUID::from_u128(0x9f2e5f61_7283_9e0f_1a2b_3c4d5e6f7081);
    let filter_flow_v6_key = GUID::from_u128(0xa03f6072_8394_0f1a_2b3c_4d5e6f708192);
    let filter_recv_v4_key = GUID::from_u128(0xb14f7183_9405_1f2b_3c4d_5e6f70819203);
    let filter_recv_v6_key = GUID::from_u128(0xc25f8294_a516_2f3c_4d5e_6f7081920314);

    unsafe {
        let mut engine = HANDLE::default();
        if FwpmEngineOpen0(None, RPC_C_AUTHN_WINNT, None, None, &mut engine) != ERROR_SUCCESS {
            return;
        }
        let _ = FwpmFilterDeleteByKey0(engine, &filter_key_v4);
        let _ = FwpmFilterDeleteByKey0(engine, &filter_key_v6);
        let _ = FwpmFilterDeleteByKey0(engine, &filter_flow_v4_key);
        let _ = FwpmFilterDeleteByKey0(engine, &filter_flow_v6_key);
        let _ = FwpmFilterDeleteByKey0(engine, &filter_recv_v4_key);
        let _ = FwpmFilterDeleteByKey0(engine, &filter_recv_v6_key);
        let _ = FwpmSubLayerDeleteByKey0(engine, &sublayer_key);
        let _ = FwpmProviderDeleteByKey0(engine, &provider_key);
        let _ = FwpmEngineClose0(engine);
    }
}

/// Remove a Windows Firewall rule by name.
fn remove_firewall_rule_by_name(rule_name: &str) -> Result<()> {
    use windows::core::BSTR;
    use windows::Win32::NetworkManagement::WindowsFirewall::{INetFwPolicy2, NetFwPolicy2};
    use windows::Win32::System::Com::{CoCreateInstance, CoInitializeEx, CLSCTX_INPROC_SERVER, COINIT_APARTMENTTHREADED};

    unsafe {
        let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
        let policy: INetFwPolicy2 = CoCreateInstance(&NetFwPolicy2, None, CLSCTX_INPROC_SERVER)
            .map_err(|e| SandboxError::CreationFailed(format!("CoCreateInstance(NetFwPolicy2): {}", e)))?;
        let rules = policy.Rules()
            .map_err(|e| SandboxError::CreationFailed(format!("INetFwPolicy2::Rules: {}", e)))?;
        let name_bstr = BSTR::from(rule_name);
        rules.Remove(&name_bstr)
            .map_err(|e| SandboxError::CreationFailed(format!("Rules.Remove: {}", e)))?;
    }
    Ok(())
}

/// Add a Windows Firewall inbound allow rule for a specific port (TCP), so LAN clients can
/// reach the web server running inside the AppContainer.
/// Note: inbound rules must NOT set LocalAppPackageId — that field only applies to outbound
/// rules. A plain port-based inbound rule is sufficient; the AppContainer process binds the
/// port and the OS routes inbound packets to it normally.
fn add_firewall_inbound_rule_for_port(rule_name: &str, port: u16) -> Result<()> {
    use windows::core::BSTR;
    use windows::Win32::Foundation::VARIANT_TRUE;
    use windows::Win32::NetworkManagement::WindowsFirewall::{
        INetFwPolicy2, INetFwRule, NetFwPolicy2, NetFwRule,
        NET_FW_ACTION_ALLOW, NET_FW_IP_PROTOCOL_TCP, NET_FW_PROFILE2_ALL, NET_FW_RULE_DIR_IN,
    };
    use windows::Win32::System::Com::{
        CoCreateInstance, CoInitializeEx, CLSCTX_INPROC_SERVER, COINIT_APARTMENTTHREADED,
    };

    unsafe {
        let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
        let policy: INetFwPolicy2 =
            CoCreateInstance(&NetFwPolicy2, None, CLSCTX_INPROC_SERVER).map_err(|e| {
                SandboxError::CreationFailed(format!("CoCreateInstance(NetFwPolicy2): {}", e))
            })?;
        let rules = policy.Rules().map_err(|e| {
            SandboxError::CreationFailed(format!("INetFwPolicy2::Rules: {}", e))
        })?;
        let rule: INetFwRule = CoCreateInstance(&NetFwRule, None, CLSCTX_INPROC_SERVER)
            .map_err(|e| SandboxError::CreationFailed(format!("CoCreateInstance(NetFwRule): {}", e)))?;
        rule.SetName(&BSTR::from(rule_name))
            .map_err(|e| SandboxError::CreationFailed(format!("SetName: {}", e)))?;
        rule.SetDirection(NET_FW_RULE_DIR_IN)
            .map_err(|e| SandboxError::CreationFailed(format!("SetDirection: {}", e)))?;
        rule.SetAction(NET_FW_ACTION_ALLOW)
            .map_err(|e| SandboxError::CreationFailed(format!("SetAction: {}", e)))?;
        rule.SetProtocol(NET_FW_IP_PROTOCOL_TCP.0 as i32)
            .map_err(|e| SandboxError::CreationFailed(format!("SetProtocol: {}", e)))?;
        rule.SetLocalPorts(&BSTR::from(port.to_string()))
            .map_err(|e| SandboxError::CreationFailed(format!("SetLocalPorts: {}", e)))?;
        rule.SetEnabled(VARIANT_TRUE)
            .map_err(|e| SandboxError::CreationFailed(format!("SetEnabled: {}", e)))?;
        rule.SetProfiles(NET_FW_PROFILE2_ALL.0)
            .map_err(|e| SandboxError::CreationFailed(format!("SetProfiles: {}", e)))?;
        rules.Add(&rule)
            .map_err(|e| SandboxError::CreationFailed(format!("Rules.Add: {}", e)))?;
    }
    Ok(())
}

/// Grant the AppContainer loopback exemption so processes on the same machine can connect
/// to ports bound inside the AppContainer (e.g. the web server on 127.0.0.1).
/// Equivalent to: CheckNetIsolation.exe LoopbackExempt -a -n="<profile_name>"
fn add_loopback_exemption_for_appcontainer(container_sid: *mut std::ffi::c_void) -> Result<()> {
    use windows::Win32::NetworkManagement::WindowsFirewall::NetworkIsolationSetAppContainerConfig;
    use windows::Win32::Security::SID_AND_ATTRIBUTES;

    let sid_attr = SID_AND_ATTRIBUTES {
        Sid: windows::Win32::Security::PSID(container_sid),
        Attributes: 0,
    };
    unsafe {
        let err = NetworkIsolationSetAppContainerConfig(&[sid_attr]);
        if err != 0 {
            return Err(SandboxError::CreationFailed(format!(
                "NetworkIsolationSetAppContainerConfig failed (error {})",
                err
            )));
        }
    }
    Ok(())
}

/// Grant the AppContainer SID read-only or read+write access to a path (file or directory).
/// Caller must run with sufficient privileges to modify the path's DACL.
fn grant_appcontainer_path_access(path: &Path, container_sid: *mut std::ffi::c_void, read_only: bool) -> Result<()> {
    if container_sid.is_null() {
        return Err(SandboxError::CreationFailed("container_sid is null".to_string()));
    }
    let path_wide = to_wide_null(&path.to_string_lossy());
    // Writable needs DELETE (0x10000) so that rename(tmp, target) can overwrite existing
    // session files (MoveFileEx overwrite requires DELETE on the target in AppContainer).
    const FILE_DELETE: u32 = 0x0001_0000;
    let perms = if read_only {
        FILE_GENERIC_READ.0
    } else {
        FILE_GENERIC_READ.0 | FILE_GENERIC_WRITE.0 | FILE_DELETE
    };
    unsafe {
        let mut sd: PSECURITY_DESCRIPTOR = PSECURITY_DESCRIPTOR(null_mut());
        let mut dacl: *mut ACL = null_mut();
        let err = GetNamedSecurityInfoW(
            PCWSTR::from_raw(path_wide.as_ptr()),
            SE_FILE_OBJECT,
            DACL_SECURITY_INFORMATION,
            None,
            None,
            Some(&mut dacl),
            None,
            &mut sd,
        );
        if err != ERROR_SUCCESS {
            return Err(SandboxError::CreationFailed(format!(
                "GetNamedSecurityInfoW({}) failed: {:?}",
                path.display(),
                err
            )));
        }
        let mut trustee = TRUSTEE_W::default();
        BuildTrusteeWithSidW(&mut trustee, PSID(container_sid));
        let explicit_access = EXPLICIT_ACCESS_W {
            grfAccessPermissions: perms,
            grfAccessMode: GRANT_ACCESS,
            grfInheritance: SUB_CONTAINERS_AND_OBJECTS_INHERIT,
            Trustee: trustee,
        };
        let mut new_acl: *mut ACL = null_mut();
        let err2 = SetEntriesInAclW(Some(&[explicit_access]), Some(dacl), &mut new_acl);
        if err2 != ERROR_SUCCESS {
            let _ = LocalFree(HLOCAL(sd.0));
            return Err(SandboxError::CreationFailed(format!(
                "SetEntriesInAclW({}) failed: {:?}",
                path.display(),
                err2
            )));
        }
        let err3 = SetNamedSecurityInfoW(
            PCWSTR::from_raw(path_wide.as_ptr()),
            SE_FILE_OBJECT,
            DACL_SECURITY_INFORMATION,
            PSID(null_mut()),
            PSID(null_mut()),
            Some(new_acl),
            None,
        );
        let _ = LocalFree(HLOCAL(new_acl as *mut _));
        let _ = LocalFree(HLOCAL(sd.0));
        if err3 != ERROR_SUCCESS {
            return Err(SandboxError::CreationFailed(format!(
                "SetNamedSecurityInfoW({}) failed: {:?}",
                path.display(),
                err3
            )));
        }
    }
    Ok(())
}

const SE_GROUP_ENABLED: u32 = 0x20;

/// HRESULT for "cannot create when file already exists" (AppContainer profile already exists).
const HRESULT_ALREADY_EXISTS: HRESULT = HRESULT(0x800700B7_u32 as i32);

impl WindowsAppContainerSandbox {
    /// Spawn a child process inside the AppContainer and wait for it to exit.
    /// Call after start(). Returns the process exit code.
    /// When network is enabled, WFP permit filters (added in start()) allow outbound traffic.
    pub fn spawn_child_in_container(&self, exe: &Path, args: &[String]) -> Result<i32> {
        let container_sid = self
            .container_sid
            .ok_or_else(|| SandboxError::CreationFailed("AppContainer not started".to_string()))?;

        // Build command line: "exe" arg1 arg2 ...
        let cmd_line: String = std::iter::once(exe.as_os_str().to_string_lossy().into_owned())
            .chain(args.iter().cloned())
            .map(|s| {
                if s.contains(' ') || s.is_empty() {
                    format!("\"{}\"", s.replace('\"', "\\\""))
                } else {
                    s
                }
            })
            .collect::<Vec<_>>()
            .join(" ");
        let cmd_wide = to_wide_null(&cmd_line);
        let exe_wide = to_wide_null(&exe.to_string_lossy());

        let is_system_path = |path: &Path| {
            let s = path.to_string_lossy().replace('\\', "/").to_lowercase();
            s == "c:/windows"
                || s.starts_with("c:/windows/")
                || s == "c:/program files"
                || s.starts_with("c:/program files/")
                || s == "c:/program files (x86)"
                || s.starts_with("c:/program files (x86)/")
        };

        // Grant AppContainer SID access to exe directory and config paths.
        if let Some(parent) = exe.parent() {
            let _ = writeln!(std::io::stderr(), "[synbot sandbox] Grant access: exe dir {}...", parent.display());
            let _ = std::io::stderr().flush();
            if let Err(e) = grant_appcontainer_path_access(parent, container_sid, false) {
                log::warn!("Grant access to exe dir {}: {} (child may fail to start)", parent.display(), e);
            }
        }
        for p in &self.config.filesystem.writable_paths {
            let path = Path::new(p);
            let _ = writeln!(std::io::stderr(), "[synbot sandbox] Grant write: {}...", path.display());
            let _ = std::io::stderr().flush();
            if let Err(e) = grant_appcontainer_path_access(path, container_sid, false) {
                log::warn!("Grant write access to {}: {}", path.display(), e);
            }
        }
        for p in &self.config.filesystem.readonly_paths {
            let path = Path::new(p);
            if is_system_path(path) {
                let _ = writeln!(std::io::stderr(), "[synbot sandbox] Skip ACL for system path: {} (would block)", path.display());
                let _ = std::io::stderr().flush();
                continue;
            }
            let _ = writeln!(std::io::stderr(), "[synbot sandbox] Grant read: {}...", path.display());
            let _ = std::io::stderr().flush();
            if let Err(e) = grant_appcontainer_path_access(path, container_sid, true) {
                log::warn!("Grant read access to {}: {}", path.display(), e);
            }
        }

        // Working directory: use config child_work_dir if set (e.g. "~" or "C:\Users\you"), else first writable or exe dir.
        let work_dir: Option<PathBuf> = self
            .config
            .child_work_dir
            .as_ref()
            .map(|s| Path::new(s).to_path_buf())
            .or_else(|| {
                self.config
                    .filesystem
                    .writable_paths
                    .first()
                    .map(|s| Path::new(s).to_path_buf())
            })
            .or_else(|| exe.parent().map(|p| p.to_path_buf()));
        let work_dir_wide = work_dir.as_ref().map(|p| to_wide_null(&p.to_string_lossy()));

        let (h_stdin, h_stdout, h_stderr) = unsafe {
            let hin = GetStdHandle(STD_INPUT_HANDLE).unwrap_or_default();
            let hout = GetStdHandle(STD_OUTPUT_HANDLE).unwrap_or_default();
            let herr = GetStdHandle(STD_ERROR_HANDLE).unwrap_or_default();
            if let Ok(()) = SetHandleInformation(hin, 1, HANDLE_FLAG_INHERIT) {}
            if let Ok(()) = SetHandleInformation(hout, 1, HANDLE_FLAG_INHERIT) {}
            if let Ok(()) = SetHandleInformation(herr, 1, HANDLE_FLAG_INHERIT) {}
            (hin, hout, herr)
        };

        let current_dir = work_dir_wide
            .as_ref()
            .map(|w| PCWSTR::from_raw(w.as_ptr()))
            .unwrap_or(PCWSTR::from_raw(std::ptr::null()));
        std::env::set_var("SYNBOT_IN_APP_SANDBOX", "1");

        // Spawn inside AppContainer. Network requires both (1) Capability SIDs (e.g. INTERNET_CLIENT
        // S-1-15-3-1) in SECURITY_CAPABILITIES and (2) WFP/firewall allow for the container SID.
        let mut capability_sids: Vec<*mut std::ffi::c_void> = Vec::new();
        let mut sid_attrs: Vec<SID_AND_ATTRIBUTES> = Vec::new();
        for cap in &self.capabilities {
            let wide_sid = to_wide_null(&cap.sid);
            unsafe {
                let mut psid = PSID(null_mut());
                if ConvertStringSidToSidW(PCWSTR::from_raw(wide_sid.as_ptr()), &mut psid).is_ok()
                    && !psid.0.is_null()
                {
                    capability_sids.push(psid.0);
                    sid_attrs.push(SID_AND_ATTRIBUTES {
                        Sid: psid,
                        Attributes: SE_GROUP_ENABLED,
                    });
                } else {
                    log::warn!(
                        "AppContainer capability SID conversion failed: {} ({})",
                        cap.name,
                        cap.sid
                    );
                }
            }
        }
        if self.config.network.enabled && sid_attrs.is_empty() {
            log::warn!("AppContainer has no capability SIDs (network.enabled=true); outbound network may be denied");
        }
        let _ = writeln!(
            std::io::stderr(),
            "[synbot sandbox] SECURITY_CAPABILITIES: CapabilityCount={}",
            sid_attrs.len()
        );
        let _ = std::io::stderr().flush();

        let capabilities = SECURITY_CAPABILITIES {
            AppContainerSid: PSID(container_sid),
            Capabilities: sid_attrs.as_mut_ptr(),
            CapabilityCount: sid_attrs.len() as u32,
            Reserved: 0,
        };

        let _ = writeln!(std::io::stderr(), "[synbot sandbox] Creating child process...");
        let _ = std::io::stderr().flush();

        unsafe {
            let mut size = 0usize;
            let _ = InitializeProcThreadAttributeList(
                LPPROC_THREAD_ATTRIBUTE_LIST(null_mut()),
                1,
                0,
                &mut size,
            );
            let mut buf = vec![0u8; size];
            let attr_list = buf.as_mut_ptr() as *mut std::ffi::c_void;
            let attr_list_handle = LPPROC_THREAD_ATTRIBUTE_LIST(attr_list);
            InitializeProcThreadAttributeList(attr_list_handle, 1, 0, &mut size)
                .map_err(|e| SandboxError::ExecutionFailed(format!("InitializeProcThreadAttributeList: {}", e)))?;

            UpdateProcThreadAttribute(
                attr_list_handle,
                0,
                PROC_THREAD_ATTRIBUTE_SECURITY_CAPABILITIES as usize,
                Some(&capabilities as *const _ as *const std::ffi::c_void),
                std::mem::size_of::<SECURITY_CAPABILITIES>(),
                None,
                None,
            )
            .map_err(|e| SandboxError::ExecutionFailed(format!("UpdateProcThreadAttribute: {}", e)))?;

            let startup = STARTUPINFOEXW {
                StartupInfo: STARTUPINFOW {
                    cb: std::mem::size_of::<STARTUPINFOEXW>() as u32,
                    dwFlags: STARTF_USESTDHANDLES,
                    hStdInput: h_stdin,
                    hStdOutput: h_stdout,
                    hStdError: h_stderr,
                    ..Default::default()
                },
                lpAttributeList: attr_list_handle,
            };

            let mut pi = PROCESS_INFORMATION::default();
            CreateProcessW(
                PCWSTR::from_raw(exe_wide.as_ptr()),
                windows::core::PWSTR(cmd_wide.as_ptr() as *mut u16),
                None,
                None,
                true,
                EXTENDED_STARTUPINFO_PRESENT,
                None,
                current_dir,
                &startup as *const STARTUPINFOEXW as *const STARTUPINFOW,
                &mut pi,
            )
            .map_err(|e| SandboxError::ExecutionFailed(format!("CreateProcessW: {}", e)))?;

            let _ = writeln!(std::io::stderr(), "[synbot sandbox] Child process created, waiting for exit...");
            let _ = std::io::stderr().flush();

            for sid in &mut capability_sids {
                if !(*sid).is_null() {
                    let _ = LocalFree(HLOCAL(*sid));
                }
            }

            let _ = CloseHandle(pi.hThread);
            WaitForSingleObject(pi.hProcess, u32::MAX);
            let mut exit_code: u32 = 0;
            let _ = GetExitCodeProcess(pi.hProcess, &mut exit_code);
            let _ = CloseHandle(pi.hProcess);
            DeleteProcThreadAttributeList(attr_list_handle);
            Ok(exit_code as i32)
        }
    }
}

impl Sandbox for WindowsAppContainerSandbox {
    fn start(&mut self) -> Result<()> {
        self.status.state = SandboxState::Starting;

        let name_wide = to_wide_null(&self.profile_name);
        let display_wide = to_wide_null(&format!("SynBot Sandbox {}", self.config.sandbox_id));
        let desc_wide = to_wide_null("Sandbox for SynBot agent process");

        // Build capability SIDs for CreateAppContainerProfile (ConvertStringSidToSidW returns via *mut PSID; alloc with LocalAlloc, free with LocalFree)
        let mut capability_sids: Vec<*mut std::ffi::c_void> = Vec::new();
        let mut sid_attrs: Vec<SID_AND_ATTRIBUTES> = Vec::new();
        for cap in &self.capabilities {
            let wide_sid = to_wide_null(&cap.sid);
            unsafe {
                let mut psid = PSID(null_mut());
                if ConvertStringSidToSidW(PCWSTR::from_raw(wide_sid.as_ptr()), &mut psid).is_ok()
                    && !psid.0.is_null()
                {
                    capability_sids.push(psid.0);
                    sid_attrs.push(SID_AND_ATTRIBUTES {
                        Sid: psid,
                        Attributes: SE_GROUP_ENABLED,
                    });
                }
            }
        }

        let cap_slice = if sid_attrs.is_empty() {
            None
        } else {
            Some(sid_attrs.as_slice())
        };
        let _ = writeln!(
            std::io::stderr(),
            "[synbot sandbox] CreateAppContainerProfile with {} capability SIDs (e.g. INTERNET_CLIENT S-1-15-3-1 for network)",
            sid_attrs.len()
        );
        let _ = std::io::stderr().flush();

        unsafe {
            let psid = match CreateAppContainerProfile(
                PCWSTR::from_raw(name_wide.as_ptr()),
                PCWSTR::from_raw(display_wide.as_ptr()),
                PCWSTR::from_raw(desc_wide.as_ptr()),
                cap_slice,
            ) {
                Ok(sid) => sid,
                Err(e) if e.code() == HRESULT_ALREADY_EXISTS => {
                    // Profile left from a previous run; delete and retry once.
                    let _ = DeleteAppContainerProfile(PCWSTR::from_raw(name_wide.as_ptr()));
                    CreateAppContainerProfile(
                        PCWSTR::from_raw(name_wide.as_ptr()),
                        PCWSTR::from_raw(display_wide.as_ptr()),
                        PCWSTR::from_raw(desc_wide.as_ptr()),
                        cap_slice,
                    )
                    .map_err(|e| SandboxError::CreationFailed(format!("CreateAppContainerProfile (after delete): {}", e)))?
                }
                Err(e) => {
                    return Err(SandboxError::CreationFailed(format!(
                        "CreateAppContainerProfile: {}",
                        e
                    )))
                }
            };

            for sid in &mut capability_sids {
                if !(*sid).is_null() {
                    let _ = LocalFree(HLOCAL(*sid));
                    *sid = null_mut();
                }
            }

            self.container_sid = Some(psid.0);
        }

        // Allow outbound network: firewall rule + WFP permit (high-priority, CLEAR_ACTION_RIGHT) so AppContainer can reach internet.
        // Rules are persistent (not removed on stop); add is idempotent (remove-then-add for firewall when we have admin).
        if self.config.network.enabled {
            if let Some(sid_str) = unsafe { sid_to_string(self.container_sid.unwrap()) } {
                let rule_name = format!("SynBot Sandbox - {}", self.config.sandbox_id);
                let _ = remove_firewall_rule_by_name(&rule_name); // idempotent: remove existing so Add succeeds when we have admin
                match add_firewall_outbound_rule_for_appcontainer(&sid_str, &rule_name) {
                    Ok(()) => {
                        self.firewall_rule_name = Some(rule_name);
                        let _ = writeln!(std::io::stderr(), "[synbot sandbox] Firewall outbound rule added for AppContainer");
                        let _ = std::io::stderr().flush();
                    }
                    Err(e) => {
                        log::warn!("Firewall outbound rule: {} (rules persist; if already added by Administrator, network may work)", e);
                    }
                }
            }
            if let Some(sid_str) = unsafe { sid_to_string(self.container_sid.unwrap()) } {
                let _ = writeln!(std::io::stderr(), "[synbot sandbox] WFP permit will use AppContainer SID: {}", sid_str);
                let _ = std::io::stderr().flush();
            }
            match add_wfp_permit_for_appcontainer(self.container_sid.unwrap()) {
                Ok(_) => {
                    let _ = writeln!(std::io::stderr(), "[synbot sandbox] WFP permit filters added for AppContainer outbound (child SID should match above)");
                    let _ = writeln!(std::io::stderr(), "[synbot sandbox] If outbound HTTPS still fails, see docs/getting-started/appcontainer-network-troubleshooting.md (WFP audit)");
                    let _ = std::io::stderr().flush();
                }
                Err(e) => {
                    let _ = writeln!(std::io::stderr(), "[synbot sandbox] WARNING: Could not add WFP permit: {}", e);
                    let _ = writeln!(std::io::stderr(), "[synbot sandbox] Troubleshooting: Run PowerShell or CMD as Administrator (e.g. right-click -> Run as administrator), then run synbot sandbox. If it still fails, ensure Base Filtering Engine (BFE) service is running.");
                    let _ = std::io::stderr().flush();
                    log::warn!("WFP permit for AppContainer: {} (network may be blocked)", e);
                }
            }

            // Loopback exemption: allow processes on the same machine to connect to ports
            // bound inside the AppContainer (e.g. web UI on 127.0.0.1).
            // Equivalent to: CheckNetIsolation.exe LoopbackExempt -a -n="<profile>"
            match add_loopback_exemption_for_appcontainer(self.container_sid.unwrap()) {
                Ok(()) => {
                    let _ = writeln!(std::io::stderr(), "[synbot sandbox] Loopback exemption granted for AppContainer (localhost access enabled)");
                    let _ = std::io::stderr().flush();
                }
                Err(e) => {
                    log::warn!("Loopback exemption failed: {} (localhost access to web UI may not work)", e);
                }
            }

            // Inbound firewall rule: allow LAN clients to reach the web server port.
            for port in &self.config.network.allowed_ports {
                let inbound_rule_name = format!("SynBot Sandbox Inbound - {} port {}", self.config.sandbox_id, port);
                let _ = remove_firewall_rule_by_name(&inbound_rule_name); // idempotent: remove existing so Add succeeds when we have admin
                match add_firewall_inbound_rule_for_port(&inbound_rule_name, *port) {
                    Ok(()) => {
                        let _ = writeln!(std::io::stderr(), "[synbot sandbox] Firewall inbound rule added for port {}", port);
                        let _ = std::io::stderr().flush();
                    }
                    Err(e) => {
                        log::warn!("Firewall inbound rule for port {}: {}", port, e);
                    }
                }
            }
        }

        log::info!(
            "AppContainer sandbox starting: id={}, network_enabled={}, writable_paths={}, readonly_paths={}, hidden_paths={}",
            self.config.sandbox_id,
            self.config.network.enabled,
            self.config.filesystem.writable_paths.len(),
            self.config.filesystem.readonly_paths.len(),
            self.config.filesystem.hidden_paths.len()
        );

        self.status.state = SandboxState::Running;
        self.status.started_at = Some(Utc::now());

        Ok(())
    }

    fn stop(&mut self) -> Result<()> {
        self.status.state = SandboxState::Stopping;

        // Firewall and WFP rules are left in place (persistent) so that after one admin run,
        // normal users can start the sandbox without needing Administrator again. Rules
        // are keyed by sandbox_id/AppContainer SID; start() is idempotent when they already exist.
        // To remove rules manually: delete firewall rules named "SynBot Sandbox - *" and
        // "SynBot Sandbox Inbound - *", and remove WFP provider/sublayer by key (see remove_wfp_permit_filters).

        if let Some(sid) = self.container_sid.take() {
            let name_wide = to_wide_null(&self.profile_name);
            unsafe {
                let _ = DeleteAppContainerProfile(PCWSTR::from_raw(name_wide.as_ptr()));
                FreeSid(PSID(sid));
            }
        }

        log::info!("AppContainer sandbox stopped: id={}", self.config.sandbox_id);

        self.status.state = SandboxState::Stopped;
        self.status.stopped_at = Some(Utc::now());

        Ok(())
    }
    
    fn execute(
        &self,
        command: &str,
        args: &[String],
        timeout: Duration,
        _working_dir: Option<&str>,
    ) -> Result<ExecutionResult> {
        use std::process::Command;
        use std::time::Instant;
        
        // For now, we'll use a basic implementation that runs the command
        // In a production system, we would:
        // 1. Get the AppContainer SID
        // 2. Create a process token with AppContainer restrictions
        // 3. Use CreateProcessAsUser with the restricted token
        // 4. Apply resource limits via Job Objects
        
        // Build command with arguments
        let mut cmd = Command::new(command);
        cmd.args(args);
        
        // Set timeout using a simple approach
        let start = Instant::now();
        
        // Execute command
        let output = cmd.output().map_err(|e| {
            SandboxError::ExecutionFailed(format!("Failed to execute command: {}", e))
        })?;
        
        let duration = start.elapsed();
        
        // Check timeout
        if duration > timeout {
            return Err(SandboxError::Timeout);
        }
        
        Ok(ExecutionResult {
            exit_code: output.status.code().unwrap_or(-1),
            stdout: output.stdout,
            stderr: output.stderr,
            duration,
            error: None,
        })
    }
    
    fn get_status(&self) -> SandboxStatus {
        self.status.clone()
    }
    
    fn health_check(&self) -> HealthStatus {
        let mut checks = HashMap::new();
        
        // Check if container is running
        let is_running = self.status.state == SandboxState::Running;
        checks.insert("running".to_string(), is_running);
        
        // Check if profile is configured
        let has_profile = !self.profile_name.is_empty();
        checks.insert("profile_configured".to_string(), has_profile);
        
        let healthy = is_running && has_profile;
        let message = if healthy {
            "Sandbox is healthy".to_string()
        } else {
            format!("Sandbox is not healthy: state={:?}, has_profile={}", 
                    self.status.state, has_profile)
        };
        
        HealthStatus {
            healthy,
            checks,
            message,
        }
    }
    
    fn get_info(&self) -> SandboxInfo {
        SandboxInfo {
            sandbox_id: self.config.sandbox_id.clone(),
            platform: "windows".to_string(),
            sandbox_type: "appcontainer".to_string(),
        }
    }
}

impl Drop for WindowsAppContainerSandbox {
    fn drop(&mut self) {
        // Ensure cleanup on drop
        let _ = self.stop();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sandbox::types::*;
    
    fn create_test_config() -> SandboxConfig {
        SandboxConfig {
            sandbox_id: "test-windows-001".to_string(),
            platform: "windows".to_string(),
            filesystem: FilesystemConfig {
                readonly_paths: vec!["C:\\Windows\\System32".to_string()],
                writable_paths: vec!["C:\\Temp".to_string()],
                hidden_paths: vec!["C:\\Windows\\System32\\config".to_string()],
                ..Default::default()
            },
            network: NetworkConfig {
                enabled: true,
                allowed_hosts: vec!["api.example.com".to_string()],
                allowed_ports: vec![80, 443],
            },
            resources: ResourceConfig {
                max_memory: 1024 * 1024 * 1024, // 1GB
                max_cpu: 1.0,
                max_disk: 5 * 1024 * 1024 * 1024, // 5GB
            },
            process: ProcessConfig {
                allow_fork: false,
                max_processes: 10,
            },
            child_work_dir: None,
            monitoring: MonitoringConfig::default(),
            delete_on_start: false,
        }
    }
    
    #[test]
    fn test_new_appcontainer_sandbox() {
        let config = create_test_config();
        let sandbox = WindowsAppContainerSandbox::new(config.clone());
        
        assert!(sandbox.is_ok());
        let sandbox = sandbox.unwrap();
        assert_eq!(sandbox.config.sandbox_id, "test-windows-001");
        assert_eq!(sandbox.status.state, SandboxState::Created);
    }
    
    #[test]
    fn test_build_capabilities_with_network() {
        let config = create_test_config();
        let capabilities = WindowsAppContainerSandbox::build_capabilities(&config).unwrap();
        
        // Should have network capabilities
        assert!(!capabilities.is_empty());
        assert!(capabilities.iter().any(|c| c.name == "internetClient"));
        assert!(capabilities.iter().any(|c| c.name == "internetClientServer"));
    }
    
    #[test]
    fn test_build_capabilities_without_network() {
        let mut config = create_test_config();
        config.network.enabled = false;
        
        let capabilities = WindowsAppContainerSandbox::build_capabilities(&config).unwrap();
        
        // Should have no network capabilities
        assert!(capabilities.iter().all(|c| c.name != "internetClient"));
        assert!(capabilities.iter().all(|c| c.name != "internetClientServer"));
    }
    
    #[test]
    fn test_build_capabilities_with_file_access() {
        let config = create_test_config();
        let capabilities = WindowsAppContainerSandbox::build_capabilities(&config).unwrap();
        
        // Should have document library capability for file access
        assert!(capabilities.iter().any(|c| c.name == "documentsLibrary"));
    }
    
    #[test]
    fn test_build_capabilities_with_allowed_hosts() {
        let config = create_test_config();
        let capabilities = WindowsAppContainerSandbox::build_capabilities(&config).unwrap();
        
        // Should have private network capability when specific hosts are allowed
        assert!(capabilities.iter().any(|c| c.name == "privateNetworkClientServer"));
    }
    
    #[test]
    fn test_get_info() {
        let config = create_test_config();
        let sandbox = WindowsAppContainerSandbox::new(config).unwrap();
        
        let info = sandbox.get_info();
        assert_eq!(info.platform, "windows");
        assert_eq!(info.sandbox_type, "appcontainer");
        assert_eq!(info.sandbox_id, "test-windows-001");
    }
    
    #[test]
    fn test_health_check_created_state() {
        let config = create_test_config();
        let sandbox = WindowsAppContainerSandbox::new(config).unwrap();
        
        let health = sandbox.health_check();
        assert!(!health.healthy); // Not healthy until started
        assert_eq!(health.checks.get("running"), Some(&false));
    }
}
