// Platform detection and unified API for cross-platform sandbox management
//
// This module provides automatic platform detection and selects the appropriate
// sandbox implementation based on the current operating system.

use super::error::{Result, SandboxError};
use super::types::SandboxConfig;
use super::sandbox_trait::Sandbox;

/// Detected platform information
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlatformInfo {
    /// Operating system name (windows, linux, macos)
    pub os: String,
    
    /// OS version
    pub version: String,
    
    /// Architecture (x86_64, aarch64, etc.)
    pub arch: String,
    
    /// Whether the platform is supported
    pub supported: bool,
    
    /// Recommended sandbox type for applications
    pub recommended_app_sandbox: String,
    
    /// Recommended sandbox type for tools
    pub recommended_tool_sandbox: String,
}

/// Platform detector
pub struct PlatformDetector;

impl PlatformDetector {
    /// Detect the current platform
    /// 
    /// Automatically detects the operating system, version, and architecture,
    /// and determines the appropriate sandbox implementations.
    /// 
    /// # Returns
    /// 
    /// A `PlatformInfo` structure with detected platform details
    pub fn detect() -> PlatformInfo {
        let os = std::env::consts::OS.to_string();
        let arch = std::env::consts::ARCH.to_string();
        let version = Self::detect_os_version();
        
        let (supported, recommended_app_sandbox, recommended_tool_sandbox) = 
            Self::get_recommendations(&os);
        
        PlatformInfo {
            os,
            version,
            arch,
            supported,
            recommended_app_sandbox,
            recommended_tool_sandbox,
        }
    }
    
    /// Detect OS version
    fn detect_os_version() -> String {
        #[cfg(target_os = "windows")]
        {
            Self::detect_windows_version()
        }
        
        #[cfg(target_os = "linux")]
        {
            Self::detect_linux_version()
        }
        
        #[cfg(target_os = "macos")]
        {
            Self::detect_macos_version()
        }
        
        #[cfg(not(any(target_os = "windows", target_os = "linux", target_os = "macos")))]
        {
            "unknown".to_string()
        }
    }
    
    #[cfg(target_os = "windows")]
    fn detect_windows_version() -> String {
        // Try to get Windows version from registry or system info
        // For now, return a placeholder
        "10+".to_string()
    }
    
    #[cfg(target_os = "linux")]
    fn detect_linux_version() -> String {
        // Try to read /etc/os-release
        if let Ok(content) = std::fs::read_to_string("/etc/os-release") {
            for line in content.lines() {
                if line.starts_with("VERSION_ID=") {
                    return line.split('=').nth(1)
                        .unwrap_or("unknown")
                        .trim_matches('"')
                        .to_string();
                }
            }
        }
        "unknown".to_string()
    }
    
    #[cfg(target_os = "macos")]
    fn detect_macos_version() -> String {
        // Try to get macOS version from system_profiler or sw_vers
        // For now, return a placeholder
        "10.15+".to_string()
    }
    
    /// Get sandbox recommendations for a platform
    fn get_recommendations(os: &str) -> (bool, String, String) {
        match os {
            "windows" => (
                true,
                "appcontainer".to_string(),
                "wsl2-gvisor".to_string(),
            ),
            "linux" => (
                true,
                "nono".to_string(),
                "gvisor-docker".to_string(),
            ),
            "macos" => (
                true,
                "nono".to_string(),
                "gvisor-docker".to_string(),
            ),
            _ => (
                false,
                "unsupported".to_string(),
                "unsupported".to_string(),
            ),
        }
    }
    
    /// Check if the current platform is supported
    pub fn is_supported() -> bool {
        matches!(std::env::consts::OS, "windows" | "linux" | "macos")
    }
    
    /// Get the current platform name
    pub fn current_platform() -> String {
        std::env::consts::OS.to_string()
    }
    
    /// Get the current architecture
    pub fn current_arch() -> String {
        std::env::consts::ARCH.to_string()
    }
}

/// Sandbox factory for creating platform-appropriate sandbox instances
pub struct SandboxFactory;

impl SandboxFactory {
    /// Create an application sandbox for the current platform
    /// 
    /// Automatically selects the appropriate sandbox implementation based on
    /// the detected platform. If the config specifies "auto" as the platform,
    /// it will be replaced with the detected platform.
    /// 
    /// # Arguments
    /// 
    /// * `config` - Sandbox configuration (platform field can be "auto")
    /// 
    /// # Returns
    /// 
    /// A boxed sandbox implementation suitable for the current platform
    /// 
    /// # Errors
    /// 
    /// Returns an error if:
    /// - The platform is not supported
    /// - The sandbox implementation cannot be created
    pub fn create_app_sandbox(mut config: SandboxConfig) -> Result<Box<dyn Sandbox>> {
        // Auto-detect platform if needed
        if config.platform == "auto" {
            config.platform = PlatformDetector::current_platform();
        }
        
        // Validate platform support
        if !PlatformDetector::is_supported() {
            return Err(SandboxError::UnsupportedPlatform);
        }
        
        // Create platform-specific sandbox
        #[cfg(target_os = "windows")]
        {
            use crate::sandbox::WindowsAppContainerSandbox;
            Ok(Box::new(WindowsAppContainerSandbox::new(config)?))
        }
        
        #[cfg(any(target_os = "linux", target_os = "macos"))]
        {
            use crate::sandbox::NonoSandbox;
            Ok(Box::new(NonoSandbox::new(config)?))
        }
        
        #[cfg(not(any(target_os = "windows", target_os = "linux", target_os = "macos")))]
        {
            Err(SandboxError::UnsupportedPlatform)
        }
    }
    
    /// Create a tool sandbox for the current platform
    /// 
    /// Automatically selects the appropriate sandbox implementation based on
    /// the detected platform. If the config specifies "auto" as the platform,
    /// it will be replaced with the detected platform.
    /// 
    /// # Arguments
    /// 
    /// * `config` - Sandbox configuration (platform field can be "auto")
    /// 
    /// # Returns
    /// 
    /// A boxed sandbox implementation suitable for the current platform
    /// 
    /// # Errors
    /// 
    /// Returns an error if:
    /// - The platform is not supported
    /// - Docker is not available
    /// - gVisor runtime is not installed
    /// - On Windows: WSL2 is not available
    pub fn create_tool_sandbox(mut config: SandboxConfig) -> Result<Box<dyn Sandbox>> {
        // Auto-detect platform if needed
        if config.platform == "auto" {
            config.platform = PlatformDetector::current_platform();
        }
        
        // Validate platform support
        if !PlatformDetector::is_supported() {
            return Err(SandboxError::UnsupportedPlatform);
        }
        
        // Create platform-specific tool sandbox
        #[cfg(target_os = "windows")]
        {
            use crate::sandbox::{GVisorDockerSandbox, Wsl2GVisorSandbox};
            match Wsl2GVisorSandbox::new(config.clone()) {
                Ok(sb) => Ok(Box::new(sb)),
                Err(e) => {
                    let msg = e.to_string();
                    if msg.contains("WSL2") || msg.contains("Docker is not accessible") {
                        GVisorDockerSandbox::new(config).map(|sb| Box::new(sb) as Box<dyn Sandbox>)
                    } else {
                        Err(e)
                    }
                }
            }
        }
        
        #[cfg(any(target_os = "linux", target_os = "macos"))]
        {
            use crate::sandbox::GVisorDockerSandbox;
            Ok(Box::new(GVisorDockerSandbox::new(config)?))
        }
        
        #[cfg(not(any(target_os = "windows", target_os = "linux", target_os = "macos")))]
        {
            Err(SandboxError::UnsupportedPlatform)
        }
    }
    
    /// Get recommended sandbox type for applications on current platform
    pub fn recommended_app_sandbox_type() -> String {
        let platform_info = PlatformDetector::detect();
        platform_info.recommended_app_sandbox
    }
    
    /// Get recommended sandbox type for tools on current platform
    pub fn recommended_tool_sandbox_type() -> String {
        let platform_info = PlatformDetector::detect();
        platform_info.recommended_tool_sandbox
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_platform_detection() {
        let platform_info = PlatformDetector::detect();
        
        // Should detect one of the supported platforms
        assert!(
            platform_info.os == "windows" 
            || platform_info.os == "linux" 
            || platform_info.os == "macos"
        );
        
        // Should be marked as supported
        assert!(platform_info.supported);
        
        // Should have recommendations
        assert!(!platform_info.recommended_app_sandbox.is_empty());
        assert!(!platform_info.recommended_tool_sandbox.is_empty());
    }
    
    #[test]
    fn test_is_supported() {
        // Current platform should be supported (since we're running tests)
        assert!(PlatformDetector::is_supported());
    }
    
    #[test]
    fn test_current_platform() {
        let platform = PlatformDetector::current_platform();
        assert!(!platform.is_empty());
        assert!(platform == "windows" || platform == "linux" || platform == "macos");
    }
    
    #[test]
    fn test_current_arch() {
        let arch = PlatformDetector::current_arch();
        assert!(!arch.is_empty());
    }
    
    #[test]
    fn test_platform_recommendations() {
        let platform_info = PlatformDetector::detect();
        
        match platform_info.os.as_str() {
            "windows" => {
                assert_eq!(platform_info.recommended_app_sandbox, "appcontainer");
                assert_eq!(platform_info.recommended_tool_sandbox, "wsl2-gvisor");
            }
            "linux" | "macos" => {
                assert_eq!(platform_info.recommended_app_sandbox, "nono");
                assert_eq!(platform_info.recommended_tool_sandbox, "gvisor-docker");
            }
            _ => {}
        }
    }
    
    #[test]
    fn test_recommended_sandbox_types() {
        let app_type = SandboxFactory::recommended_app_sandbox_type();
        let tool_type = SandboxFactory::recommended_tool_sandbox_type();
        
        assert!(!app_type.is_empty());
        assert!(!tool_type.is_empty());
        
        // Verify recommendations match platform
        let platform = PlatformDetector::current_platform();
        match platform.as_str() {
            "windows" => {
                assert_eq!(app_type, "appcontainer");
                assert_eq!(tool_type, "wsl2-gvisor");
            }
            "linux" | "macos" => {
                assert_eq!(app_type, "nono");
                assert_eq!(tool_type, "gvisor-docker");
            }
            _ => {}
        }
    }
}
