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
use std::collections::{HashMap, HashSet};
use std::ffi::OsStr;
use std::os::windows::ffi::OsStrExt;
use std::io::{Read, Write};
use std::os::windows::io::{FromRawHandle, RawHandle};
use std::path::{Path, PathBuf};
use std::ptr::null_mut;
use std::time::{Duration, Instant};
use windows::core::{HRESULT, PCWSTR};
use windows::Win32::Foundation::{
    CloseHandle, HANDLE, HLOCAL, LocalFree, WAIT_OBJECT_0, WAIT_TIMEOUT,
};
use windows::Win32::System::Memory::{LocalAlloc, LMEM_ZEROINIT};
use windows::Win32::Security::Authorization::{
    BuildTrusteeWithSidW, ConvertSidToStringSidW, ConvertStringSidToSidW, GetNamedSecurityInfoW,
    SetEntriesInAclW, SetNamedSecurityInfoW, EXPLICIT_ACCESS_W, GRANT_ACCESS, SE_FILE_OBJECT,
    TRUSTEE_W,
};
use windows::Win32::Security::Isolation::{CreateAppContainerProfile, DeleteAppContainerProfile};
use windows::Win32::Security::{
    AclSizeInformation, CopySid, EqualSid, FreeSid, GetAce, GetAclInformation, GetLengthSid,
    ACCESS_ALLOWED_ACE, ACCESS_DENIED_ACE, ACE_HEADER, ACL, ACL_SIZE_INFORMATION, PSID,
    PSECURITY_DESCRIPTOR, SID_AND_ATTRIBUTES, SECURITY_ATTRIBUTES, SECURITY_CAPABILITIES,
    SUB_CONTAINERS_AND_OBJECTS_INHERIT,
};
// ACCESS_ALLOWED_ACE_TYPE = 0, ACCESS_DENIED_ACE_TYPE = 1 (avoid extra `windows` crate feature)
const ACCESS_ALLOWED_ACE_TYPE_U8: u8 = 0;
const ACCESS_DENIED_ACE_TYPE_U8: u8 = 1;
use windows::Win32::Security::DACL_SECURITY_INFORMATION;
use windows::Win32::Storage::FileSystem::{
    CreateFileW, QueryDosDeviceW, FILE_FLAG_BACKUP_SEMANTICS, FILE_GENERIC_EXECUTE, FILE_GENERIC_READ,
    FILE_GENERIC_WRITE, FILE_SHARE_DELETE, FILE_SHARE_READ, FILE_SHARE_WRITE, OPEN_EXISTING,
};
use windows::Win32::Storage::FileSystem::GetVolumeInformationW;
use windows::Win32::Foundation::{ERROR_SUCCESS, SetHandleInformation, HANDLE_FLAG_INHERIT, HANDLE_FLAGS};
use windows::Win32::System::Console::{GetStdHandle, STD_ERROR_HANDLE, STD_INPUT_HANDLE, STD_OUTPUT_HANDLE};
use windows::Win32::System::Pipes::CreatePipe;
use windows::Win32::System::Threading::{
    CreateProcessW, DeleteProcThreadAttributeList, GetExitCodeProcess,
    InitializeProcThreadAttributeList, LPPROC_THREAD_ATTRIBUTE_LIST, PROCESS_INFORMATION,
    STARTUPINFOEXW, STARTUPINFOW, TerminateProcess, UpdateProcThreadAttribute,
    WaitForSingleObject, EXTENDED_STARTUPINFO_PRESENT, PROC_THREAD_ATTRIBUTE_SECURITY_CAPABILITIES,
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

/// Resolve `X:\...` paths that are DOS device mappings (e.g. SUBST drives).
///
/// AppContainer file access can fail when a SUBST drive letter is used (e.g. `S:\...`), even if
/// the underlying target path (e.g. `C:\...`) was granted in ACL setup. Resolve `X:` via
/// `QueryDosDeviceW` and, when it is of the form `\??\C:\...`, rewrite to the underlying target.
fn resolve_windows_dos_device_path(p: &Path) -> PathBuf {
    let s = p.to_string_lossy();
    let bytes = s.as_bytes();
    if bytes.len() < 3 || bytes[1] != b':' || (bytes[2] != b'\\' && bytes[2] != b'/') {
        return p.to_path_buf();
    }
    let drive = &s[0..2]; // "C:"
    let drive_w = to_wide_null(drive);
    let mut buf: Vec<u16> = vec![0; 32 * 1024];
    let n = unsafe { QueryDosDeviceW(PCWSTR::from_raw(drive_w.as_ptr()), Some(&mut buf)) };
    if n == 0 {
        return p.to_path_buf();
    }
    let end = buf.iter().position(|c| *c == 0).unwrap_or(n as usize);
    let target = String::from_utf16_lossy(&buf[..end]).replace('/', "\\");
    let mapped_root = target
        .strip_prefix(r"\\??\\")
        .or_else(|| target.strip_prefix(r"\??\"));
    let Some(mapped_root) = mapped_root else {
        return p.to_path_buf();
    };
    let suffix = &s[3..]; // after "X:\"
    let mut out = PathBuf::from(mapped_root);
    if !suffix.is_empty() {
        out.push(suffix);
    }
    out
}

fn normalize_host_path_for_appcontainer(p: &Path) -> PathBuf {
    crate::config::normalize_workspace_path(&resolve_windows_dos_device_path(p))
}

fn windows_volume_root_for_path(path: &Path) -> Option<PathBuf> {
    use std::path::Component;
    let mut comps = path.components();
    match (comps.next(), comps.next()) {
        (Some(Component::Prefix(prefix)), Some(Component::RootDir)) => {
            Some(PathBuf::from(prefix.as_os_str()).join(r"\"))
        }
        _ => None,
    }
}

fn log_windows_volume_acl_capability(path: &Path) {
    let Some(root) = windows_volume_root_for_path(path) else {
        return;
    };
    let root_s = root.to_string_lossy();
    let root_w = to_wide_null(&root_s);
    let mut fs_name_buf: Vec<u16> = vec![0; 64];
    let mut flags: u32 = 0;
    unsafe {
        let ok = GetVolumeInformationW(
            PCWSTR::from_raw(root_w.as_ptr()),
            None,
            None,
            None,
            Some(&mut flags),
            Some(&mut fs_name_buf),
        )
        .is_ok();
        if !ok {
            log::warn!("Volume info query failed for {}", root.display());
            return;
        }
    }
    let fs_name_end = fs_name_buf.iter().position(|c| *c == 0).unwrap_or(fs_name_buf.len());
    let fs_name = String::from_utf16_lossy(&fs_name_buf[..fs_name_end]);
    // From Win32 `GetVolumeInformationW` docs: FILE_PERSISTENT_ACLS = 0x00000008
    const FILE_PERSISTENT_ACLS: u32 = 0x0000_0008;
    let has_persistent_acls = (flags & FILE_PERSISTENT_ACLS) != 0;
    log::info!(
        "Volume capability: root={} fs={} flags=0x{:08x} persistent_acls={}",
        root.display(),
        fs_name,
        flags,
        has_persistent_acls
    );
    if !has_persistent_acls {
        log::warn!(
            "Volume {} filesystem {} does not report FILE_PERSISTENT_ACLS; AppContainer ACL grants may not work on this drive",
            root.display(),
            fs_name
        );
    }
}

fn host_preflight_open_dir(path: &Path) -> std::result::Result<(), u32> {
    let wide = to_wide_null(&path.to_string_lossy());
    unsafe {
        // Open directory handle (requires FILE_FLAG_BACKUP_SEMANTICS).
        let h = CreateFileW(
            PCWSTR::from_raw(wide.as_ptr()),
            (FILE_GENERIC_READ | FILE_GENERIC_EXECUTE).0,
            FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE,
            None,
            OPEN_EXISTING,
            FILE_FLAG_BACKUP_SEMANTICS,
            None,
        );
        match h {
            Ok(handle) => {
                let _ = CloseHandle(handle);
                Ok(())
            }
            Err(e) => Err(e.code().0 as u32),
        }
    }
}

/// Returns true if the current process is running with elevated privileges (Administrator).
/// Used to skip firewall/WFP/loopback add when running as normal user (rules should already exist from `synbot sandbox setup`).
fn is_process_elevated() -> bool {
    use windows::Win32::Foundation::HANDLE;
    use windows::Win32::Security::{GetTokenInformation, TokenElevation, TOKEN_QUERY};
    use windows::Win32::System::Threading::{GetCurrentProcess, OpenProcessToken};

    #[repr(C)]
    struct TokenElevationLayout {
        token_is_elevated: u32,
    }

    unsafe {
        let process = GetCurrentProcess();
        let mut token = HANDLE::default();
        if OpenProcessToken(process, TOKEN_QUERY, &mut token).is_err() {
            return false;
        }
        let mut ret_len = 0u32;
        let _ = GetTokenInformation(token, TokenElevation, None, 0, &mut ret_len);
        let mut buf = vec![0u8; ret_len as usize];
        if GetTokenInformation(
            token,
            TokenElevation,
            Some(buf.as_mut_ptr() as *mut _),
            ret_len,
            &mut ret_len,
        )
        .is_err()
        {
            let _ = CloseHandle(token);
            return false;
        }
        let _ = CloseHandle(token);
        let layout = &*(buf.as_ptr() as *const TokenElevationLayout);
        layout.token_is_elevated != 0
    }
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
/// WFP error: filter is persistent but referenced provider/sublayer are dynamic (lifetime mismatch).
const FWP_E_LIFETIME_MISMATCH: u32 = 0x8032_0016;

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
/// Filters are added with FWPM_FILTER_FLAG_PERSISTENT so they survive reboot (BFE restores them from persistent store).
/// Idempotent: if provider/sublayer/filters already exist (e.g. leftover from crash), treats as success.
/// Cleanup in stop() uses delete-by-key (no stored IDs needed).
fn add_wfp_permit_for_appcontainer(container_sid: *mut std::ffi::c_void) -> Result<()> {
    use windows::Win32::NetworkManagement::WindowsFilteringPlatform::{
        FwpmEngineClose0, FwpmEngineOpen0, FwpmFilterAdd0, FwpmFilterDeleteById0, FwpmFilterDeleteByKey0,
        FwpmProviderAdd0, FwpmProviderDeleteByKey0, FwpmSubLayerAdd0, FwpmSubLayerDeleteByKey0,
        FWPM_LAYER_ALE_AUTH_CONNECT_V4, FWPM_LAYER_ALE_AUTH_CONNECT_V6,
        FWPM_LAYER_ALE_FLOW_ESTABLISHED_V4, FWPM_LAYER_ALE_FLOW_ESTABLISHED_V6,
        FWPM_CONDITION_ALE_PACKAGE_ID, FWPM_FILTER0, FWPM_FILTER_CONDITION0,
        FWPM_FILTER_FLAG_CLEAR_ACTION_RIGHT, FWPM_FILTER_FLAG_PERSISTENT,
        FWPM_PROVIDER0, FWPM_PROVIDER_FLAG_PERSISTENT, FWPM_SUBLAYER0, FWPM_SUBLAYER_FLAG_PERSISTENT,
        FWP_ACTION_PERMIT, FWP_CONDITION_VALUE0, FWP_CONDITION_VALUE0_0,
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
            flags: FWPM_PROVIDER_FLAG_PERSISTENT,
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
            flags: FWPM_SUBLAYER_FLAG_PERSISTENT,
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
            flags: FWPM_FILTER_FLAG_CLEAR_ACTION_RIGHT | FWPM_FILTER_FLAG_PERSISTENT,
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
        let mut add4 = FwpmFilterAdd0(engine, &filter_v4, None, Some(&mut id_v4));
        // If existing provider/sublayer were added without PERSISTENT (e.g. by older code), we get LIFETIME_MISMATCH.
        // Remove our WFP objects and re-add provider/sublayer as persistent, then retry.
        if add4 == FWP_E_LIFETIME_MISMATCH {
            let _ = FwpmFilterDeleteByKey0(engine, &filter_key_v4);
            let _ = FwpmFilterDeleteByKey0(engine, &filter_key_v6);
            let _ = FwpmFilterDeleteByKey0(engine, &filter_flow_v4_key);
            let _ = FwpmFilterDeleteByKey0(engine, &filter_flow_v6_key);
            let _ = FwpmFilterDeleteByKey0(engine, &filter_recv_v4_key);
            let _ = FwpmFilterDeleteByKey0(engine, &filter_recv_v6_key);
            let _ = FwpmSubLayerDeleteByKey0(engine, &sublayer_key);
            let _ = FwpmProviderDeleteByKey0(engine, &provider_key);
            let provider2 = FWPM_PROVIDER0 {
                providerKey: provider_key,
                displayData: FWPM_DISPLAY_DATA0 {
                    name: windows::core::PWSTR(provider_name.as_ptr() as *mut u16),
                    description: windows::core::PWSTR::null(),
                },
                flags: FWPM_PROVIDER_FLAG_PERSISTENT,
                providerData: FWP_BYTE_BLOB::default(),
                serviceName: windows::core::PWSTR::null(),
            };
            let _ = FwpmProviderAdd0(engine, &provider2, None);
            let sublayer2 = FWPM_SUBLAYER0 {
                subLayerKey: sublayer_key,
                displayData: FWPM_DISPLAY_DATA0 {
                    name: windows::core::PWSTR(sublayer_name.as_ptr() as *mut u16),
                    description: windows::core::PWSTR::null(),
                },
                flags: FWPM_SUBLAYER_FLAG_PERSISTENT,
                providerKey: &provider_key as *const _ as *mut _,
                providerData: FWP_BYTE_BLOB::default(),
                weight: 0,
            };
            let _ = FwpmSubLayerAdd0(engine, &sublayer2, None);
            add4 = FwpmFilterAdd0(engine, &filter_v4, None, Some(&mut id_v4));
        }
        if add4 != ERROR_SUCCESS && add4 != FWP_E_ALREADY_EXISTS {
            if provider_added_this_run {
                let _ = FwpmSubLayerDeleteByKey0(engine, &sublayer_key);
                let _ = FwpmProviderDeleteByKey0(engine, &provider_key);
            }
            let _ = FwpmEngineClose0(engine);
            return Err(SandboxError::CreationFailed(format!(
                "FwpmFilterAdd0 (V4) failed (error {}). If error is FWP_E_LIFETIME_MISMATCH (0x80320016), run 'synbot sandbox setup' as Administrator to recreate persistent WFP objects.",
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

/// `CreateProcessW` with a non-null application name only resolves partial names against the
/// **current directory**, not `PATH`. The exec tool passes bare `cmd`.
///
/// On Windows, `Path::new("cmd").parent()` is `Some("")` (empty), which breaks ACL APIs with
/// `ERROR_INVALID_NAME` (123).
fn resolve_windows_command_for_create_process(command: &str) -> PathBuf {
    let cmd = command.trim();
    if cmd.is_empty() {
        return PathBuf::from(cmd);
    }
    let path = Path::new(cmd);
    if path.is_absolute() {
        return path.to_path_buf();
    }
    if cmd.contains('\\') || cmd.contains('/') {
        return path.to_path_buf();
    }
    let lower = cmd.to_ascii_lowercase();
    if lower == "cmd" || lower == "cmd.exe" {
        if let Ok(c) = std::env::var("ComSpec") {
            let p = PathBuf::from(c.trim());
            if !p.as_os_str().is_empty() {
                return p;
            }
        }
        return PathBuf::from(r"C:\Windows\System32\cmd.exe");
    }
    PathBuf::from(cmd)
}

/// Skip DACL edits under well-known Windows install roots (often `ERROR_ACCESS_DENIED` for
/// non-elevated callers; AppContainer can still run system binaries per policy).
fn is_windows_skip_acl_dir(path: &Path) -> bool {
    let s = path.to_string_lossy().replace('\\', "/").to_lowercase();
    s == "c:/windows"
        || s.starts_with("c:/windows/")
        || s == "c:/program files"
        || s.starts_with("c:/program files/")
        || s == "c:/program files (x86)"
        || s.starts_with("c:/program files (x86)/")
}

fn normalize_windows_path_cmp_key(path: &Path) -> String {
    let mut s = path.to_string_lossy().replace('\\', "/").to_lowercase();
    while s.len() > 1 && s.ends_with('/') {
        s.pop();
    }
    s
}

/// `desc` is `root` or a subdirectory of `root` (compares normalized path keys).
fn path_is_under_or_equal_directory(desc: &Path, root: &Path) -> bool {
    let root_key = normalize_windows_path_cmp_key(root);
    if root_key.is_empty() {
        return false;
    }
    let mut cur = Some(desc);
    while let Some(p) = cur {
        if normalize_windows_path_cmp_key(p) == root_key {
            return true;
        }
        cur = p.parent();
    }
    false
}

/// Keep only outermost paths. Inheritable `SetNamedSecurityInfoW` on each nested folder can make
/// Windows propagate ACLs through the **entire** subtree each time — extremely slow when
/// `C:\Users\…\.synbot` already covers `…\.synbot\workspace`, logs, workflows, etc.
fn collapse_nested_paths_outermost(paths: &[String]) -> Vec<String> {
    if paths.is_empty() {
        return vec![];
    }
    let parsed: Vec<PathBuf> = paths.iter().map(PathBuf::from).collect();
    let mut order: Vec<usize> = (0..parsed.len()).collect();
    order.sort_by_key(|&i| parsed[i].components().count());
    let mut kept: Vec<usize> = Vec::new();
    for &i in &order {
        if kept
            .iter()
            .any(|&j| path_is_under_or_equal_directory(&parsed[i], &parsed[j]))
        {
            continue;
        }
        kept.retain(|&j| !path_is_under_or_equal_directory(&parsed[j], &parsed[i]));
        kept.push(i);
    }
    kept.into_iter().map(|i| paths[i].clone()).collect()
}

/// `C:\` style path — stop ancestor traverse before the volume root.
fn is_windows_volume_root(path: &Path) -> bool {
    use std::path::Component;
    let mut it = path.components();
    match (it.next(), it.next(), it.next()) {
        (Some(Component::Prefix(_)), Some(Component::RootDir), None) => true,
        _ => false,
    }
}

/// `SetNamedSecurityInfoW` with inheritable ACE on the profile root propagates through the entire tree.
fn windows_path_is_user_profile_root(path: &Path) -> bool {
    let Some(prof) = std::env::var_os("USERPROFILE") else {
        return false;
    };
    let pb = Path::new(&prof);
    if pb.as_os_str().is_empty() {
        return false;
    }
    normalize_windows_path_cmp_key(path) == normalize_windows_path_cmp_key(pb)
}

fn normalized_work_dir_path(s: &str) -> PathBuf {
    normalize_host_path_for_appcontainer(Path::new(s.trim()))
}

/// Directory safe to pass as `CreateProcessW` `lpCurrentDirectory` when the desired user folder
/// may be rejected for the AppContainer token (PowerShell reports “current directory invalid”).
fn windows_host_create_process_cwd() -> PathBuf {
    std::env::var_os("SystemRoot")
        .map(|root| PathBuf::from(root).join("System32"))
        .filter(|p| p.is_dir())
        .unwrap_or_else(|| PathBuf::from(r"C:\Windows\System32"))
}

fn join_args_for_cmd_c_tail(parts: &[String]) -> String {
    match parts.len() {
        0 => String::new(),
        1 => parts[0].clone(),
        _ => parts.join(" "),
    }
}

/// `cd /d` target for `cmd /c`: avoid quoting the path when possible.
///
/// If we emit `cd /d "C:\path"` and the whole `/C` argument is later wrapped in `"..."` for
/// `CreateProcessW`, we end up with embedded `\"`, which `cmd` often parses as **invalid path
/// syntax** (Win32 123 — 文件名、目录名或卷标语法不正确).
fn cmd_cd_path_segment_for_c(user_workdir: &Path) -> String {
    let s = user_workdir.to_string_lossy();
    let s = s.trim();
    let needs_quotes = s.contains(' ')
        || s.contains('&')
        || s.contains('(')
        || s.contains(')')
        || s.contains('^')
        || s.contains('%')
        || s.contains('\t');
    if needs_quotes {
        let inner = s.replace('"', "");
        format!("\"{inner}\"")
    } else {
        s.to_string()
    }
}

fn cmd_pushd_path_segment_for_c(user_workdir: &Path) -> String {
    // pushd has similar parsing pitfalls to cd; reuse the same quoting rules.
    cmd_cd_path_segment_for_c(user_workdir)
}

/// Use host System32 as the process current directory, then `cd` / `Set-Location` into
/// `user_workdir` inside the shell so listing and tools see the intended folder.
fn wrap_spawn_args_for_user_workdir(exe: &Path, args: &[String], user_workdir: &Path) -> Vec<String> {
    let stem = exe
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    let wd_lit = user_workdir.to_string_lossy();

    if stem == "cmd" {
        if args.len() >= 2 && args[0].eq_ignore_ascii_case("/c") {
            let user_cmd = join_args_for_cmd_c_tail(&args[1..]);
            // Prefer `pushd` over `cd /d` for AppContainer: it tends to behave better with drive changes
            // and avoids some "current directory invalid" cases when the process cwd is System32.
            let target = cmd_pushd_path_segment_for_c(user_workdir);
            let combined = if user_cmd.is_empty() {
                format!("pushd {target}")
            } else {
                format!("pushd {target} && {user_cmd}")
            };
            return vec!["/C".to_string(), combined];
        }
    }
    if stem == "powershell" || stem == "pwsh" {
        for i in 0..args.len().saturating_sub(1) {
            let fl = args[i].to_ascii_lowercase();
            if fl == "-command" || fl == "-c" {
                let wd_ps = wd_lit.replace('\'', "''");
                let script = args[i + 1].clone();
                let combined = format!("Set-Location -LiteralPath '{}'; {}", wd_ps, script);
                let mut out = args.to_vec();
                out[i + 1] = combined;
                return out;
            }
        }
    }
    args.to_vec()
}

/// Main synbot runs inside the app sandbox (`SYNBOT_IN_APP_SANDBOX`): the token cannot change
/// host folder DACLs (`SetNamedSecurityInfoW` → access denied). Grants must be done once from
/// `synbot sandbox setup` (Administrator, not under app sandbox).
fn skip_runtime_appcontainer_acl_grants() -> bool {
    std::env::var_os("SYNBOT_IN_APP_SANDBOX").is_some()
}

/// Runtime `SetNamedSecurityInfoW` from a **non-elevated** process can be extremely slow or appear
/// to hang (AV / policy / contention on system folders). `synbot sandbox setup` (Administrator)
/// already applies DACLs; skip duplicate edits on `synbot sandbox start` as a normal user.
fn should_apply_runtime_appcontainer_acl_grants() -> bool {
    !skip_runtime_appcontainer_acl_grants() && is_process_elevated()
}

/// Read-only NTFS check for diagnostics: does the object's DACL enumerate an explicit
/// `ACCESS_ALLOWED` / `ACCESS_DENIED` ACE whose trustee equals the tool AppContainer SID?
/// This does **not** compute effective access; it confirms whether `synbot sandbox setup`
/// (or another tool) applied an ACE for **this** profile SID on each path component.
unsafe fn dacl_explicit_ace_flags_for_sid(
    path: &Path,
    container_sid: PSID,
) -> std::result::Result<(bool, bool), u32> {
    let path_wide = to_wide_null(&path.to_string_lossy());
    let mut dacl: *mut ACL = null_mut();
    let mut sd = PSECURITY_DESCRIPTOR::default();
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
    if err.0 != 0 {
        return Err(err.0);
    }
    let mut si = ACL_SIZE_INFORMATION::default();
    if let Err(e) = GetAclInformation(
        dacl as *const ACL,
        &mut si as *mut ACL_SIZE_INFORMATION as *mut std::ffi::c_void,
        std::mem::size_of::<ACL_SIZE_INFORMATION>() as u32,
        AclSizeInformation,
    ) {
        if !sd.0.is_null() {
            let _ = LocalFree(HLOCAL(sd.0));
        }
        return Err(e.code().0 as u32);
    }
    let mut allow = false;
    let mut deny = false;
    for i in 0..si.AceCount {
        let mut pace: *mut std::ffi::c_void = null_mut();
        if GetAce(dacl as *const ACL, i, &mut pace).is_err() {
            continue;
        }
        let header = &*(pace as *const ACE_HEADER);
        if header.AceType == ACCESS_ALLOWED_ACE_TYPE_U8 {
            let ace = &*(pace as *const ACCESS_ALLOWED_ACE);
            let sid_ptr = PSID(core::ptr::addr_of!(ace.SidStart) as *mut _);
            if EqualSid(sid_ptr, container_sid).is_ok() {
                allow = true;
            }
        } else if header.AceType == ACCESS_DENIED_ACE_TYPE_U8 {
            let ace = &*(pace as *const ACCESS_DENIED_ACE);
            let sid_ptr = PSID(core::ptr::addr_of!(ace.SidStart) as *mut _);
            if EqualSid(sid_ptr, container_sid).is_ok() {
                deny = true;
            }
        }
    }
    if !sd.0.is_null() {
        let _ = LocalFree(HLOCAL(sd.0));
    }
    Ok((allow, deny))
}

fn log_tool_sandbox_ntfs_dacl_probe_after_denied(
    work_dir: &Path,
    container_sid: *mut std::ffi::c_void,
    sandbox_id: &str,
) {
    let sid_str = unsafe { sid_to_string(container_sid) }.unwrap_or_else(|| "<unknown>".to_string());
    let profile_name = format!("SynBot.Sandbox.{}", sandbox_id);
    let psid = PSID(container_sid);
    let mut parts: Vec<String> = Vec::new();
    let mut cur = Some(work_dir.to_path_buf());
    let mut steps = 0u32;
    while let Some(p) = cur.take() {
        if steps >= 16 {
            parts.push("... (truncated at 16 levels)".to_string());
            break;
        }
        if p.as_os_str().is_empty() {
            break;
        }
        let label = p.display().to_string();
        let r = unsafe { dacl_explicit_ace_flags_for_sid(&p, psid) };
        match r {
            Ok((a, d)) => {
                parts.push(format!("{}:allow={}:deny={}", label, a, d));
            }
            Err(code) => {
                parts.push(format!("{}:dacl_read_error={}", label, code));
                break;
            }
        }
        cur = p.parent().map(PathBuf::from);
        if cur.as_ref().map(|x| x.as_os_str().is_empty()).unwrap_or(true) {
            break;
        }
        steps += 1;
    }
    let chain = parts.join(" | ");
    // Use `tracing` (not `log`): this crate initializes tracing only; `log::` events are invisible in normal runs.
    tracing::warn!(
        target: "synbot::sandbox::ntfs_verify",
        tool_appcontainer_sid = %sid_str,
        tool_sandbox_profile_name = %profile_name,
        chain = %chain,
        "tool sandbox: NTFS DACL read-only probe after child access denied"
    );
    // Always mirror a short line to helper stderr so it appears even when log filtering hides the target.
    let summary = format!(
        "sid={} profile={}",
        sid_str, profile_name
    );
    let chain_tail = if chain.len() > 600 {
        format!("{} ... (truncated, full chain in WARN synbot::sandbox::ntfs_verify)", &chain[..600])
    } else {
        chain
    };
    let _ = writeln!(
        std::io::stderr(),
        "[synbot tool-sandbox] NTFS verify: {} | {}",
        summary, chain_tail
    );
}

/// Non-inheriting read on each parent directory so the AppContainer SID can **traverse** to the target.
/// Without this, ACEs only on e.g. `...\yangxb\.synbot\workspace` do not grant traverse on `...\yangxb`.
/// Deduplicated across all paths in one setup run (cheap: O(depth) per path, no subtree propagation).
fn grant_appcontainer_path_ancestors_traverse(
    path: &Path,
    container_sid: *mut std::ffi::c_void,
    ancestor_seen: &mut HashSet<String>,
) {
    if container_sid.is_null() {
        return;
    }
    let phase_start = Instant::now();
    let target = path.display().to_string();
    let mut new_ancestor_grants: u32 = 0;
    let _ = writeln!(
        std::io::stderr(),
        "[synbot sandbox]   Ancestors for {} — each line below is one DACL edit (slow on large ACLs / AV hooks)...",
        target
    );
    let _ = std::io::stderr().flush();
    let path = normalize_host_path_for_appcontainer(path);
    let mut current = path.parent();
    while let Some(dir) = current {
        if dir.as_os_str().is_empty() {
            break;
        }
        if is_windows_volume_root(dir) {
            // Traverse into e.g. C:\Users\… requires rights on the volume root (e.g. C:\).
            log_windows_volume_acl_capability(dir);
            let key = dir.to_string_lossy().to_lowercase();
            if ancestor_seen.insert(key) {
                new_ancestor_grants += 1;
                let _ = writeln!(
                    std::io::stderr(),
                    "[synbot sandbox]   Ancestor: {} (volume root)...",
                    dir.display()
                );
                let _ = std::io::stderr().flush();
                let t = Instant::now();
                let _ = writeln!(
                    std::io::stderr(),
                    "[synbot sandbox]     ACL {} (read, this folder only) …",
                    dir.display()
                );
                let _ = std::io::stderr().flush();
                if let Err(e) = grant_appcontainer_path_access(dir, container_sid, true, false) {
                    let wall_ms = t.elapsed().as_secs_f64() * 1000.0;
                    let _ = writeln!(
                        std::io::stderr(),
                        "[synbot sandbox]     ACL ancestor failed after {:.0}ms: {}",
                        wall_ms, e
                    );
                    let _ = std::io::stderr().flush();
                    log::warn!(
                        "sandbox setup: ancestor traverse ACL grant failed for {}: {}",
                        dir.display(),
                        e
                    );
                } else {
                    let wall_ms = t.elapsed().as_secs_f64() * 1000.0;
                    let _ = writeln!(
                        std::io::stderr(),
                        "[synbot sandbox]     ACL ancestor finished in {:.0}ms (details: RUST_LOG=info)",
                        wall_ms
                    );
                    let _ = std::io::stderr().flush();
                }
            }
            break;
        }
        if is_windows_skip_acl_dir(dir) {
            break;
        }
        let key = dir.to_string_lossy().to_lowercase();
        if !ancestor_seen.insert(key) {
            current = dir.parent();
            continue;
        }
        new_ancestor_grants += 1;
        let _ = writeln!(
            std::io::stderr(),
            "[synbot sandbox]   Ancestor: {} ...",
            dir.display()
        );
        let _ = std::io::stderr().flush();
        let t = Instant::now();
        let _ = writeln!(
            std::io::stderr(),
            "[synbot sandbox]     ACL {} (read, this folder only) …",
            dir.display()
        );
        let _ = std::io::stderr().flush();
        if let Err(e) = grant_appcontainer_path_access(dir, container_sid, true, false) {
            let wall_ms = t.elapsed().as_secs_f64() * 1000.0;
            let _ = writeln!(
                std::io::stderr(),
                "[synbot sandbox]     ACL ancestor failed after {:.0}ms: {}",
                wall_ms, e
            );
            let _ = std::io::stderr().flush();
            log::warn!(
                "sandbox setup: ancestor traverse ACL grant failed for {}: {}",
                dir.display(),
                e
            );
        } else {
            let wall_ms = t.elapsed().as_secs_f64() * 1000.0;
            let _ = writeln!(
                std::io::stderr(),
                "[synbot sandbox]     ACL ancestor finished in {:.0}ms (details: RUST_LOG=info)",
                wall_ms
            );
            let _ = std::io::stderr().flush();
        }
        current = dir.parent();
    }
    let phase_ms = phase_start.elapsed().as_secs_f64() * 1000.0;
    let _ = writeln!(
        std::io::stderr(),
        "[synbot sandbox]   Ancestors finished for {}: {} new grant(s) in {:.0}ms",
        target, new_ancestor_grants, phase_ms
    );
    let _ = std::io::stderr().flush();
    if new_ancestor_grants > 0 {
        log::info!(
            "sandbox ACL ancestors for target={}: {} new directory grant(s), phase_total_ms={:.1}",
            target,
            new_ancestor_grants,
            phase_ms
        );
    } else {
        log::debug!(
            "sandbox ACL ancestors for target={}: all ancestors already granted (deduped), phase_total_ms={:.1}",
            target,
            phase_ms
        );
    }
}

/// One-time grants for `synbot sandbox setup` (elevated host process). Same AppContainer SID as runtime.
fn grant_config_paths_for_appcontainer_sid(
    config: &super::types::SandboxConfig,
    container_sid: *mut std::ffi::c_void,
    grant_parent_traverse_acl: bool,
) {
    if container_sid.is_null() {
        return;
    }
    let setup_acl_start = Instant::now();
    let mut seen: HashSet<String> = HashSet::new();
    let mut ancestor_seen: HashSet<String> = HashSet::new();
    let mut try_one = |raw: &str, read_only: bool, inherit_requested: bool| {
        let raw = raw.trim();
        if raw.is_empty() {
            return;
        }
        let path = normalize_host_path_for_appcontainer(Path::new(raw));
        if path.as_os_str().is_empty() {
            return;
        }
        let key = path.to_string_lossy().to_lowercase();
        if !seen.insert(key) {
            return;
        }
        if is_windows_skip_acl_dir(&path) {
            return;
        }

        let inherit = inherit_requested && !windows_path_is_user_profile_root(&path);

        let _ = writeln!(
            std::io::stderr(),
            "[synbot sandbox]   Path: {} ({}{}) — walking parent directories next (each may take seconds)...",
            path.display(),
            if read_only { "read" } else { "read/write" },
            if inherit {
                ", propagate to children"
            } else {
                ", this folder only"
            }
        );
        let _ = std::io::stderr().flush();

        if grant_parent_traverse_acl {
            grant_appcontainer_path_ancestors_traverse(&path, container_sid, &mut ancestor_seen);
        }

        let _ = writeln!(
            std::io::stderr(),
            "[synbot sandbox]   ACL {} ({}{}) …",
            path.display(),
            if read_only { "read" } else { "read/write" },
            if inherit {
                ", propagate to children"
            } else {
                ", this folder only"
            }
        );
        let _ = std::io::stderr().flush();

        let t_path = Instant::now();
        match grant_appcontainer_path_access(&path, container_sid, read_only, inherit) {
            Ok(()) => {
                let wall_ms = t_path.elapsed().as_secs_f64() * 1000.0;
                let _ = writeln!(
                    std::io::stderr(),
                    "[synbot sandbox]   ACL target finished in {:.0}ms (details: RUST_LOG=info)",
                    wall_ms
                );
                let _ = std::io::stderr().flush();
            }
            Err(e) => {
                let wall_ms = t_path.elapsed().as_secs_f64() * 1000.0;
                let _ = writeln!(
                    std::io::stderr(),
                    "[synbot sandbox]   ACL target failed after {:.0}ms: {}",
                    wall_ms, e
                );
                let _ = std::io::stderr().flush();
                log::warn!(
                    "sandbox setup: ACL grant failed for {}: {}",
                    path.display(),
                    e
                );
            }
        }
    };

    // Process writable/readonly before `child_work_dir`. If work dir was processed first with
    // object-only ACE, the same path in `writable_paths` was skipped (`seen`) and never got an
    // inheritable ACE — AppContainer could not access files under e.g. `~/.synbot`.
    let writ_collapsed = collapse_nested_paths_outermost(&config.filesystem.writable_paths);
    let read_not_under_writ: Vec<String> = config
        .filesystem
        .readonly_paths
        .iter()
        .filter(|r| {
            let rp = Path::new(r.trim());
            !writ_collapsed
                .iter()
                .any(|w| path_is_under_or_equal_directory(rp, Path::new(w.trim())))
        })
        .cloned()
        .collect();
    let read_collapsed = collapse_nested_paths_outermost(&read_not_under_writ);

    let skipped_writ = config.filesystem.writable_paths.len().saturating_sub(writ_collapsed.len());
    let skipped_read_ro = config
        .filesystem
        .readonly_paths
        .len()
        .saturating_sub(read_not_under_writ.len());
    let skipped_read_nested =
        read_not_under_writ.len().saturating_sub(read_collapsed.len());
    if skipped_writ + skipped_read_ro + skipped_read_nested > 0 {
        let _ = writeln!(
            std::io::stderr(),
            "[synbot sandbox]   Skipping {} redundant path(s): one inheritable ACL on the parent folder covers nested dirs (avoids very slow setup when the profile tree is large).",
            skipped_writ + skipped_read_ro + skipped_read_nested
        );
        let _ = std::io::stderr().flush();
    }

    for p in &writ_collapsed {
        try_one(p, false, true);
    }
    for p in &read_collapsed {
        try_one(p, true, true);
    }
    if let Some(ref wd) = config.child_work_dir {
        // Request inherit; `windows_path_is_user_profile_root` forces object-only on `%USERPROFILE%`.
        try_one(wd, false, true);
    }

    log::info!(
        "sandbox ACL setup complete sandbox_id={} total_elapsed_ms={:.1}",
        config.sandbox_id,
        setup_acl_start.elapsed().as_secs_f64() * 1000.0
    );
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
/// When `inherit` is true, the ACE is inheritable by children (used for workspace roots).
/// When `inherit` is false, the ACE applies to this object only (no inheritance to children).
/// Caller must run with sufficient privileges to modify the path's DACL.
fn grant_appcontainer_path_access(
    path: &Path,
    container_sid: *mut std::ffi::c_void,
    read_only: bool,
    inherit: bool,
) -> Result<()> {
    if container_sid.is_null() {
        return Err(SandboxError::CreationFailed("container_sid is null".to_string()));
    }
    if path.as_os_str().is_empty() {
        return Err(SandboxError::CreationFailed(
            "Grant ACL: path is empty".to_string(),
        ));
    }
    let path_disp = path.display().to_string();
    let t_total = Instant::now();
    let path_wide = to_wide_null(&path.to_string_lossy());
    // Writable needs DELETE (0x10000) so that rename(tmp, target) can overwrite existing
    // session files (MoveFileEx overwrite requires DELETE on the target in AppContainer).
    const FILE_DELETE: u32 = 0x0001_0000;
    // NOTE: For *directories*, AppContainer needs "execute/traverse" rights to `cd` / `Set-Location`
    // and to traverse ancestors. `FILE_GENERIC_EXECUTE` maps to `FILE_TRAVERSE` on directories.
    let perms = if read_only {
        FILE_GENERIC_READ.0 | FILE_GENERIC_EXECUTE.0
    } else {
        FILE_GENERIC_READ.0 | FILE_GENERIC_WRITE.0 | FILE_GENERIC_EXECUTE.0 | FILE_DELETE
    };
    let mut ms_get = 0.0f64;
    let mut ms_entries = 0.0f64;
    let mut ms_set = 0.0f64;
    unsafe {
        let t_get = Instant::now();
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
        ms_get = t_get.elapsed().as_secs_f64() * 1000.0;
        if err != ERROR_SUCCESS {
            log::warn!(
                "sandbox ACL GetNamedSecurityInfoW failed path={} after {:.1}ms err={:?}",
                path_disp, ms_get, err
            );
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
            grfInheritance: if inherit {
                SUB_CONTAINERS_AND_OBJECTS_INHERIT
            } else {
                windows::Win32::Security::ACE_FLAGS(0)
            },
            Trustee: trustee,
        };
        let mut new_acl: *mut ACL = null_mut();
        let t_entries = Instant::now();
        let err2 = SetEntriesInAclW(Some(&[explicit_access]), Some(dacl), &mut new_acl);
        ms_entries = t_entries.elapsed().as_secs_f64() * 1000.0;
        if err2 != ERROR_SUCCESS {
            let _ = LocalFree(HLOCAL(sd.0));
            log::warn!(
                "sandbox ACL SetEntriesInAclW failed path={} after get={:.1}ms entries={:.1}ms err={:?}",
                path_disp, ms_get, ms_entries, err2
            );
            return Err(SandboxError::CreationFailed(format!(
                "SetEntriesInAclW({}) failed: {:?}",
                path.display(),
                err2
            )));
        }
        let t_set = Instant::now();
        let err3 = SetNamedSecurityInfoW(
            PCWSTR::from_raw(path_wide.as_ptr()),
            SE_FILE_OBJECT,
            DACL_SECURITY_INFORMATION,
            PSID(null_mut()),
            PSID(null_mut()),
            Some(new_acl),
            None,
        );
        ms_set = t_set.elapsed().as_secs_f64() * 1000.0;
        let _ = LocalFree(HLOCAL(new_acl as *mut _));
        let _ = LocalFree(HLOCAL(sd.0));
        if err3 != ERROR_SUCCESS {
            log::warn!(
                "sandbox ACL SetNamedSecurityInfoW failed path={} after get={:.1}ms entries={:.1}ms set={:.1}ms err={:?}",
                path_disp, ms_get, ms_entries, ms_set, err3
            );
            return Err(SandboxError::CreationFailed(format!(
                "SetNamedSecurityInfoW({}) failed: {:?}",
                path.display(),
                err3
            )));
        }
    }
    let ms_total = t_total.elapsed().as_secs_f64() * 1000.0;
    log::info!(
        "sandbox ACL grant ok path={} read_only={} inherit={} ms: get_dacl={:.1} set_entries={:.1} set_dacl={:.1} wall_total={:.1}",
        path_disp,
        read_only,
        inherit,
        ms_get,
        ms_entries,
        ms_set,
        ms_total
    );
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

        // Grant AppContainer SID access to exe directory and config paths (host only; see skip_runtime_*).
        if skip_runtime_appcontainer_acl_grants() {
            log::debug!(
                "Skipping per-exec AppContainer ACL grants (main process is in app sandbox). \
                 Run `synbot sandbox setup` as Administrator on the host so DACLs are applied once."
            );
        } else if should_apply_runtime_appcontainer_acl_grants() {
            if let Some(parent) = exe.parent() {
                if !parent.as_os_str().is_empty() && !is_windows_skip_acl_dir(parent) {
                    let _ = writeln!(std::io::stderr(), "[synbot sandbox] Grant access: exe dir {}...", parent.display());
                    let _ = std::io::stderr().flush();
                    if let Err(e) = grant_appcontainer_path_access(parent, container_sid, false, true) {
                        log::warn!("Grant access to exe dir {}: {} (child may fail to start)", parent.display(), e);
                    }
                }
            }
            for p in &self.config.filesystem.writable_paths {
                let path = Path::new(p);
                let _ = writeln!(std::io::stderr(), "[synbot sandbox] Grant write: {}...", path.display());
                let _ = std::io::stderr().flush();
                if let Err(e) = grant_appcontainer_path_access(path, container_sid, false, true) {
                    log::warn!("Grant write access to {}: {}", path.display(), e);
                }
            }
            for p in &self.config.filesystem.readonly_paths {
                let path = Path::new(p);
                if is_windows_skip_acl_dir(path) {
                    let _ = writeln!(std::io::stderr(), "[synbot sandbox] Skip ACL for system path: {} (would block)", path.display());
                    let _ = std::io::stderr().flush();
                    continue;
                }
                let _ = writeln!(std::io::stderr(), "[synbot sandbox] Grant read: {}...", path.display());
                let _ = std::io::stderr().flush();
                if let Err(e) = grant_appcontainer_path_access(path, container_sid, true, true) {
                    log::warn!("Grant read access to {}: {}", path.display(), e);
                }
            }
        } else {
            let _ = writeln!(
                std::io::stderr(),
                "[synbot sandbox] Skipping runtime ACL edits (not elevated). \
                 DACLs should already exist from `synbot sandbox setup`; if the child fails with access denied, run setup again as Administrator."
            );
            let _ = std::io::stderr().flush();
        }

        // Working directory: use config child_work_dir if set (e.g. "~" or "C:\Users\you"), else first writable or exe dir.
        let work_dir: Option<PathBuf> = self
            .config
            .child_work_dir
            .as_ref()
            .map(|s| normalized_work_dir_path(s))
            .or_else(|| {
                self.config
                    .filesystem
                    .writable_paths
                    .first()
                    .map(|s| normalized_work_dir_path(s))
            })
            .or_else(|| {
                exe.parent()
                    .filter(|p| !p.as_os_str().is_empty())
                    .map(|p| crate::config::normalize_workspace_path(p))
            });
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

    /// Spawn a child in the AppContainer with piped stdout/stderr and a wait timeout.
    pub fn spawn_child_in_container_piped(
        &self,
        exe: &Path,
        args: &[String],
        working_dir_override: Option<&Path>,
        timeout: Duration,
    ) -> Result<(i32, Vec<u8>, Vec<u8>)> {
        use std::thread;

        let container_sid = self
            .container_sid
            .ok_or_else(|| SandboxError::CreationFailed("AppContainer not started".to_string()))?;
        let sid_str = unsafe { sid_to_string(container_sid) }.unwrap_or_else(|| "<unknown>".to_string());

        if skip_runtime_appcontainer_acl_grants() {
            log::debug!(
                "Skipping per-exec AppContainer ACL grants (main process is in app sandbox). \
                 Run `synbot sandbox setup` as Administrator on the host so DACLs are applied once."
            );
        } else if should_apply_runtime_appcontainer_acl_grants() {
            if let Some(parent) = exe.parent() {
                if !parent.as_os_str().is_empty() && !is_windows_skip_acl_dir(parent) {
                    if let Err(e) = grant_appcontainer_path_access(parent, container_sid, false, true) {
                        log::warn!(
                            "Grant access to exe dir {}: {} (child may fail to start)",
                            parent.display(),
                            e
                        );
                    }
                }
            }
            for p in &self.config.filesystem.writable_paths {
                let path = normalize_host_path_for_appcontainer(Path::new(p));
                if let Err(e) = grant_appcontainer_path_access(&path, container_sid, false, true) {
                    log::warn!("Grant write access to {}: {}", path.display(), e);
                }
            }
            for p in &self.config.filesystem.readonly_paths {
                let path = normalize_host_path_for_appcontainer(Path::new(p));
                if is_windows_skip_acl_dir(&path) {
                    continue;
                }
                if let Err(e) = grant_appcontainer_path_access(&path, container_sid, true, true) {
                    log::warn!("Grant read access to {}: {}", path.display(), e);
                }
            }
        } else {
            log::debug!(
                "Skipping runtime ACL edits for piped spawn (not elevated); rely on `synbot sandbox setup` DACLs"
            );
        }

        let work_dir: Option<PathBuf> = working_dir_override
            .map(|p| normalize_host_path_for_appcontainer(p))
            .or_else(|| {
                self.config
                    .child_work_dir
                    .as_ref()
                    .map(|s| normalized_work_dir_path(s))
            })
            .or_else(|| {
                self.config
                    .filesystem
                    .writable_paths
                    .first()
                    .map(|s| normalized_work_dir_path(s))
            })
            .or_else(|| {
                exe.parent()
                    .filter(|p| !p.as_os_str().is_empty())
                    .map(|p| normalize_host_path_for_appcontainer(p))
            });

        log::debug!(
            "AppContainer spawn request: sandbox_id={}, container_sid={}, exe={}, args={:?}, working_dir_override={:?}, resolved_work_dir={:?}",
            self.config.sandbox_id,
            sid_str,
            exe.display(),
            args,
            working_dir_override.map(|p| p.display().to_string()),
            work_dir.as_ref().map(|p| p.display().to_string())
        );

        let mut work_dir_preflight_ok = false;
        if let Some(wd) = work_dir.as_ref() {
            // Log volume capabilities for the workdir volume on each spawn. This helps diagnose
            // "ACL setup looks fine but AppContainer still denies access" cases on non-NTFS volumes
            // (e.g. exFAT / FAT / some virtual volumes).
            log_windows_volume_acl_capability(wd);
            match host_preflight_open_dir(wd) {
                Ok(()) => {
                    work_dir_preflight_ok = true;
                    log::debug!("Host preflight: working_dir open ok: {}", wd.display());
                }
                Err(code) => {
                    log::warn!(
                        "Host preflight: working_dir open FAILED: {} (win32_error={})",
                        wd.display(),
                        code
                    );
                }
            }
        }

        // Prefer setting `lpCurrentDirectory` directly when the directory is accessible to the
        // AppContainer token. This avoids `cmd.exe cd /d ...` failures even when ACLs look fine.
        // If current dir is rejected, fall back to a safe System32 cwd and do an in-shell `cd`.
        let stem = exe
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("")
            .to_ascii_lowercase();
        let cmd_c_payload = if stem == "cmd" && args.len() >= 2 && args[0].eq_ignore_ascii_case("/c") {
            join_args_for_cmd_c_tail(&args[1..]).to_ascii_lowercase()
        } else {
            String::new()
        };
        let cmd_payload_mentions_powershell =
            stem == "cmd" && (cmd_c_payload.contains("powershell") || cmd_c_payload.contains("pwsh"));

        let parent_in_app_sandbox = std::env::var_os("SYNBOT_IN_APP_SANDBOX").is_some();

        let (spawn_args, create_process_cwd_wide): (Vec<String>, Option<Vec<u16>>) = match work_dir.as_ref() {
            // PowerShell is known to emit “当前目录无效” when inheriting some AppContainer cwd values.
            // When cmd.exe is being used to launch PowerShell, prefer System32 + in-shell cd.
            Some(wd) if cmd_payload_mentions_powershell => (
                wrap_spawn_args_for_user_workdir(exe, args, wd),
                Some(to_wide_null(&windows_host_create_process_cwd().to_string_lossy())),
            ),
            // cmd from **inside** the app-container daemon: historical pushd workaround.
            Some(wd) if stem == "cmd" && parent_in_app_sandbox => (
                wrap_spawn_args_for_user_workdir(exe, args, wd),
                Some(to_wide_null(&windows_host_create_process_cwd().to_string_lossy())),
            ),
            // cmd from host (e.g. `tool-sandbox serve`): workspace as `lpCurrentDirectory`.
            Some(wd) if stem == "cmd" => (
                args.to_vec(),
                Some(to_wide_null(&wd.to_string_lossy())),
            ),
            Some(wd) if work_dir_preflight_ok => (args.to_vec(), Some(to_wide_null(&wd.to_string_lossy()))),
            Some(wd) => (
                wrap_spawn_args_for_user_workdir(exe, args, wd),
                Some(to_wide_null(&windows_host_create_process_cwd().to_string_lossy())),
            ),
            None => (args.to_vec(), None),
        };
        log::debug!(
            "AppContainer CreateProcessW cwd choice: sandbox_id={}, cwd_mode={}, lpCurrentDirectory={}",
            self.config.sandbox_id,
            if work_dir.is_some() && cmd_payload_mentions_powershell {
                "system32+cd(powershell)"
            } else if work_dir.is_some() && stem == "cmd" && parent_in_app_sandbox {
                "system32+pushd(cmd)"
            } else if work_dir.is_some() && stem == "cmd" {
                "direct(cmd, host parent)"
            } else if work_dir.as_ref().is_some_and(|_| work_dir_preflight_ok) {
                "direct"
            } else if work_dir.is_some() {
                "system32+cd"
            } else {
                "none"
            },
            create_process_cwd_wide
                .as_ref()
                .map(|w| String::from_utf16_lossy(&w[..w.len().saturating_sub(1)]))
                .unwrap_or_else(|| "<null>".to_string())
        );

        let cmd_line: String = std::iter::once(exe.as_os_str().to_string_lossy().into_owned())
            .chain(spawn_args.iter().cloned())
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

        let sa = SECURITY_ATTRIBUTES {
            nLength: std::mem::size_of::<SECURITY_ATTRIBUTES>() as u32,
            lpSecurityDescriptor: null_mut(),
            bInheritHandle: true.into(),
        };

        let mut stdout_r = HANDLE::default();
        let mut stdout_w = HANDLE::default();
        let mut stderr_r = HANDLE::default();
        let mut stderr_w = HANDLE::default();

        unsafe {
            CreatePipe(
                &mut stdout_r,
                &mut stdout_w,
                Some(std::ptr::addr_of!(sa)),
                0,
            )
            .map_err(|e| SandboxError::ExecutionFailed(format!("CreatePipe stdout: {}", e)))?;
            if let Err(e) = CreatePipe(
                &mut stderr_r,
                &mut stderr_w,
                Some(std::ptr::addr_of!(sa)),
                0,
            )
            {
                let _ = CloseHandle(stdout_r);
                let _ = CloseHandle(stdout_w);
                return Err(SandboxError::ExecutionFailed(format!(
                    "CreatePipe stderr: {}",
                    e
                )));
            }

            let _ = SetHandleInformation(stdout_r, 1, HANDLE_FLAGS(0));
            let _ = SetHandleInformation(stderr_r, 1, HANDLE_FLAGS(0));

            let h_stdin = GetStdHandle(STD_INPUT_HANDLE).unwrap_or_default();
            let _ = SetHandleInformation(h_stdin, 1, HANDLE_FLAG_INHERIT);

            let current_dir = create_process_cwd_wide
                .as_ref()
                .map(|w| PCWSTR::from_raw(w.as_ptr()))
                .unwrap_or(PCWSTR::from_raw(std::ptr::null()));

            let mut capability_sids: Vec<*mut std::ffi::c_void> = Vec::new();
            let mut sid_attrs: Vec<SID_AND_ATTRIBUTES> = Vec::new();
            for cap in &self.capabilities {
                let wide_sid = to_wide_null(&cap.sid);
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

            let capabilities = SECURITY_CAPABILITIES {
                AppContainerSid: PSID(container_sid),
                Capabilities: sid_attrs.as_mut_ptr(),
                CapabilityCount: sid_attrs.len() as u32,
                Reserved: 0,
            };

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
            if let Err(e) = InitializeProcThreadAttributeList(attr_list_handle, 1, 0, &mut size) {
                let _ = CloseHandle(stdout_r);
                let _ = CloseHandle(stdout_w);
                let _ = CloseHandle(stderr_r);
                let _ = CloseHandle(stderr_w);
                return Err(SandboxError::ExecutionFailed(format!(
                    "InitializeProcThreadAttributeList: {}",
                    e
                )));
            }

            if let Err(e) = UpdateProcThreadAttribute(
                attr_list_handle,
                0,
                PROC_THREAD_ATTRIBUTE_SECURITY_CAPABILITIES as usize,
                Some(&capabilities as *const _ as *const std::ffi::c_void),
                std::mem::size_of::<SECURITY_CAPABILITIES>(),
                None,
                None,
            ) {
                DeleteProcThreadAttributeList(attr_list_handle);
                let _ = CloseHandle(stdout_r);
                let _ = CloseHandle(stdout_w);
                let _ = CloseHandle(stderr_r);
                let _ = CloseHandle(stderr_w);
                return Err(SandboxError::ExecutionFailed(format!(
                    "UpdateProcThreadAttribute: {}",
                    e
                )));
            }

            let startup = STARTUPINFOEXW {
                StartupInfo: STARTUPINFOW {
                    cb: std::mem::size_of::<STARTUPINFOEXW>() as u32,
                    dwFlags: STARTF_USESTDHANDLES,
                    hStdInput: h_stdin,
                    hStdOutput: stdout_w,
                    hStdError: stderr_w,
                    ..Default::default()
                },
                lpAttributeList: attr_list_handle,
            };

            let mut pi = PROCESS_INFORMATION::default();
            let cp = CreateProcessW(
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
            );

            if let Err(e) = cp {
                for sid in &mut capability_sids {
                    if !(*sid).is_null() {
                        let _ = LocalFree(HLOCAL(*sid));
                    }
                }
                DeleteProcThreadAttributeList(attr_list_handle);
                let _ = CloseHandle(stdout_r);
                let _ = CloseHandle(stdout_w);
                let _ = CloseHandle(stderr_r);
                let _ = CloseHandle(stderr_w);
                return Err(SandboxError::ExecutionFailed(format!("CreateProcessW: {}", e)));
            }

            for sid in &mut capability_sids {
                if !(*sid).is_null() {
                    let _ = LocalFree(HLOCAL(*sid));
                }
            }

            let _ = CloseHandle(stdout_w);
            let _ = CloseHandle(stderr_w);
            let _ = CloseHandle(pi.hThread);

            let out_h = stdout_r.0 as usize;
            let err_h = stderr_r.0 as usize;
            let stdout_j = thread::spawn(move || {
                let mut v = Vec::new();
                unsafe {
                    let mut f =
                        std::fs::File::from_raw_handle(out_h as RawHandle);
                    let _ = f.read_to_end(&mut v);
                }
                v
            });
            let stderr_j = thread::spawn(move || {
                let mut v = Vec::new();
                unsafe {
                    let mut f =
                        std::fs::File::from_raw_handle(err_h as RawHandle);
                    let _ = f.read_to_end(&mut v);
                }
                v
            });

            let timeout_ms = u32::try_from(timeout.as_millis()).unwrap_or(u32::MAX).max(1);
            let wait = WaitForSingleObject(pi.hProcess, timeout_ms);

            let (exit_u32, timed_out) = if wait == WAIT_TIMEOUT {
                let _ = TerminateProcess(pi.hProcess, 1);
                let _ = WaitForSingleObject(pi.hProcess, 10_000);
                (1u32, true)
            } else if wait == WAIT_OBJECT_0 {
                let mut code = 0u32;
                let _ = GetExitCodeProcess(pi.hProcess, &mut code);
                (code, false)
            } else {
                let _ = TerminateProcess(pi.hProcess, 1);
                let _ = WaitForSingleObject(pi.hProcess, 10_000);
                let _ = CloseHandle(pi.hProcess);
                DeleteProcThreadAttributeList(attr_list_handle);
                let _ = stdout_j.join();
                let _ = stderr_j.join();
                return Err(SandboxError::ExecutionFailed(format!(
                    "WaitForSingleObject returned unexpected {:?}",
                    wait
                )));
            };

            let _ = CloseHandle(pi.hProcess);
            DeleteProcThreadAttributeList(attr_list_handle);

            let stdout = stdout_j.join().unwrap_or_default();
            let stderr = stderr_j.join().unwrap_or_default();

            if timed_out {
                return Err(SandboxError::Timeout);
            }

            if std::env::var_os(crate::sandbox::tool_sandbox_ipc::ENV_TOOL_SANDBOX_HELPER).is_some()
                && exit_u32 != 0
            {
                let err_utf = String::from_utf8_lossy(&stderr);
                let low = err_utf.to_ascii_lowercase();
                let looks_denied = err_utf.contains("拒绝访问")
                    || low.contains("access is denied")
                    || low.contains("access denied");
                if looks_denied {
                    if let Some(wd) = work_dir.as_ref() {
                        log_tool_sandbox_ntfs_dacl_probe_after_denied(
                            wd,
                            container_sid,
                            &self.config.sandbox_id,
                        );
                    }
                }
            }

            Ok((exit_u32 as i32, stdout, stderr))
        }
    }
}

impl WindowsAppContainerSandbox {
    /// Creates the AppContainer profile and adds firewall/WFP/loopback rules.
    /// Used by both start() and install_windows_sandbox_network_rules().
    /// Does not set sandbox state to Running.
    fn create_profile_and_add_network_rules(&mut self) -> Result<()> {
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

        // Allow outbound network: firewall rule + WFP permit. Only add when running elevated (Administrator);
        // otherwise rules should already exist from `synbot sandbox setup` and we skip to avoid "access denied" warnings.
        if self.config.network.enabled {
            if is_process_elevated() {
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
                            let _ = writeln!(std::io::stderr(), "[synbot sandbox] WARNING: Could not add firewall rule: {}", e);
                            let _ = writeln!(std::io::stderr(), "[synbot sandbox] Run once as Administrator: synbot sandbox setup");
                            let _ = std::io::stderr().flush();
                            log::warn!("Firewall outbound rule: {} (run as Administrator: synbot sandbox setup)", e);
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
                        let _ = writeln!(std::io::stderr(), "[synbot sandbox] Run once as Administrator: synbot sandbox setup");
                        let _ = writeln!(std::io::stderr(), "[synbot sandbox] Then you can start the sandbox as a normal user. WFP filters are persistent (survive reboot).");
                        let _ = std::io::stderr().flush();
                        log::warn!("WFP permit for AppContainer: {} (network may be blocked)", e);
                    }
                }

                // Loopback exemption: allow processes on the same machine to connect to ports
                // bound inside the AppContainer (e.g. web UI on 127.0.0.1).
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
                    let _ = remove_firewall_rule_by_name(&inbound_rule_name);
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
            } else {
                let _ = writeln!(std::io::stderr(), "[synbot sandbox] Running as normal user; using existing network rules (run 'synbot sandbox setup' as Administrator if network fails).");
                let _ = std::io::stderr().flush();
            }
        }

        Ok(())
    }
}

/// One-time setup of firewall and WFP rules for the AppContainer sandbox (Windows only).
/// Run this **once as Administrator** after install or after each reboot so that normal users
/// can start the sandbox without admin. The rules are keyed by AppContainer SID (deterministic
/// from profile name); the profile is created then deleted so the next `synbot sandbox start`
/// will recreate the same profile and reuse the rules.
///
/// `grant_parent_traverse_acl`: when `true`, also walks **parent directories** of each configured
/// path and adds traverse ACEs for the AppContainer SID (needed for deep paths under e.g.
/// `%USERPROFILE%`, but can be very slow). `synbot sandbox setup` passes **`false`** so only
/// configured paths receive DACL edits; tool AppContainer runs in a host helper process, so
/// setup no longer couples expensive ancestor grants to the dual-AppContainer case.
pub fn install_windows_sandbox_network_rules(
    config: SandboxConfig,
    grant_parent_traverse_acl: bool,
) -> Result<()> {
    let mut sandbox = WindowsAppContainerSandbox::new(config)?;
    sandbox.create_profile_and_add_network_rules()?;
    if let Some(sid) = sandbox.container_sid.take() {
        if is_process_elevated() {
            let _ = writeln!(
                std::io::stderr(),
                "[synbot sandbox] AppContainer profile: {}",
                sandbox.profile_name
            );
            if let Some(sid_str) = unsafe { sid_to_string(sid) } {
                let _ = writeln!(
                    std::io::stderr(),
                    "[synbot sandbox] AppContainer SID: {}",
                    sid_str
                );
            }
            let _ = writeln!(
                std::io::stderr(),
                "[synbot sandbox] Sandbox filesystem config: writable_paths={}, readonly_paths={}, hidden_paths={}, child_work_dir={}",
                sandbox.config.filesystem.writable_paths.len(),
                sandbox.config.filesystem.readonly_paths.len(),
                sandbox.config.filesystem.hidden_paths.len(),
                sandbox
                    .config
                    .child_work_dir
                    .as_ref()
                    .map(|s| s.as_str())
                    .unwrap_or("<none>")
            );
            for p in &sandbox.config.filesystem.writable_paths {
                let _ = writeln!(std::io::stderr(), "[synbot sandbox]   writable: {}", p);
            }
            for p in &sandbox.config.filesystem.readonly_paths {
                let _ = writeln!(std::io::stderr(), "[synbot sandbox]   readonly: {}", p);
            }
            let _ = std::io::stderr().flush();

            let _ = writeln!(
                std::io::stderr(),
                "[synbot sandbox] Granting filesystem ACLs for this AppContainer on configured paths..."
            );
            let _ = writeln!(
                std::io::stderr(),
                "[synbot sandbox]   (Nested dirs share one inheritable grant per root; very large trees can still take a few minutes.)"
            );
            let _ = std::io::stderr().flush();
            grant_config_paths_for_appcontainer_sid(&sandbox.config, sid, grant_parent_traverse_acl);
            let _ = writeln!(
                std::io::stderr(),
                "[synbot sandbox] Filesystem ACL grants finished (check logs if any path failed)."
            );
            let _ = std::io::stderr().flush();
        }
        let name_wide = to_wide_null(&sandbox.profile_name);
        unsafe {
            let _ = DeleteAppContainerProfile(PCWSTR::from_raw(name_wide.as_ptr()));
            FreeSid(PSID(sid));
        }
    }
    Ok(())
}

impl Sandbox for WindowsAppContainerSandbox {
    fn start(&mut self) -> Result<()> {
        self.status.state = SandboxState::Starting;
        self.create_profile_and_add_network_rules()?;

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
        working_dir: Option<&str>,
    ) -> Result<ExecutionResult> {
        use std::time::Instant;

        if self.container_sid.is_none() || self.status.state != SandboxState::Running {
            return Err(SandboxError::NotStarted);
        }

        let start = Instant::now();
        let exe = resolve_windows_command_for_create_process(command);
        let wd = working_dir.filter(|s| !s.is_empty()).map(Path::new);
        let (code, stdout, stderr) =
            self.spawn_child_in_container_piped(&exe, args, wd, timeout)?;
        let duration = start.elapsed();

        Ok(ExecutionResult {
            exit_code: code,
            stdout,
            stderr,
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
        let sandbox_type =
            if matches!(self.config.requested_tool_sandbox_type.as_deref(), Some("appcontainer")) {
                "appcontainer-tool".to_string()
            } else {
                "appcontainer".to_string()
            };
        SandboxInfo {
            sandbox_id: self.config.sandbox_id.clone(),
            platform: "windows".to_string(),
            sandbox_type,
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
            requested_tool_sandbox_type: None,
            image: None,
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
