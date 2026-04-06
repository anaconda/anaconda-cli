//! Platform identification for the user-agent string.
//!
//! Produces: `{kernel}/{release} {os}/{version} rattler/{version}`

use std::sync::LazyLock;

const RATTLER_VERSION: &str = env!("RATTLER_VERSION");

/// Cached platform string (computed once per process).
static PLATFORM_STRING: LazyLock<String> = LazyLock::new(build_platform_string);

/// Return the platform identification string.
///
/// Examples:
///   macOS:   `Darwin/25.2.0 OSX/26.2 rattler/0.40.3`
///   Linux:   `Linux/6.5.0 Ubuntu/22.04 rattler/0.40.3`
///   Windows: `Windows/10.0.22631 rattler/0.40.3`
pub fn platform_string() -> &'static str {
    &PLATFORM_STRING
}

fn build_platform_string() -> String {
    let mut parts = Vec::new();

    let (system, release) = system_release();
    parts.push(format!("{}/{}", system, release));

    if let Some((name, version)) = os_distribution() {
        parts.push(format!("{}/{}", name, version));
    }

    parts.push(format!("rattler/{}", RATTLER_VERSION));

    parts.join(" ")
}

/// Get the kernel name and release version via libc::uname.
#[cfg(unix)]
fn system_release() -> (String, String) {
    unsafe {
        let mut info: libc::utsname = std::mem::zeroed();
        if libc::uname(&mut info) == 0 {
            let system = std::ffi::CStr::from_ptr(info.sysname.as_ptr())
                .to_string_lossy()
                .into_owned();
            let release = std::ffi::CStr::from_ptr(info.release.as_ptr())
                .to_string_lossy()
                .into_owned();
            return (system, release);
        }
    }
    (std::env::consts::OS.to_string(), String::from("unknown"))
}

/// Get the Windows version via RtlGetVersion (ntdll.dll FFI, no crate needed).
///
/// Unlike GetVersionEx, RtlGetVersion is not subject to the compatibility
/// shim that lies about the version on Windows 8.1+.
#[cfg(not(unix))]
fn system_release() -> (String, String) {
    #[repr(C)]
    struct OsVersionInfoExW {
        os_version_info_size: u32,
        major_version: u32,
        minor_version: u32,
        build_number: u32,
        platform_id: u32,
        csd_version: [u16; 128],
        service_pack_major: u16,
        service_pack_minor: u16,
        suite_mask: u16,
        product_type: u8,
        reserved: u8,
    }

    unsafe {
        #[link(name = "ntdll")]
        extern "system" {
            fn RtlGetVersion(lp_version_information: *mut OsVersionInfoExW) -> i32;
        }

        let mut info: OsVersionInfoExW = std::mem::zeroed();
        info.os_version_info_size = std::mem::size_of::<OsVersionInfoExW>() as u32;

        if RtlGetVersion(&mut info) == 0 {
            let release = format!(
                "{}.{}.{}",
                info.major_version, info.minor_version, info.build_number
            );
            return ("Windows".to_string(), release);
        }
    }

    ("Windows".to_string(), "unknown".to_string())
}

/// Get the OS distribution name and version.
///
/// On macOS: returns ("OSX", version) via SystemVersion.plist
/// On Linux: returns distro info via /etc/os-release
/// On Windows: returns None (system_release already covers it)
fn os_distribution() -> Option<(String, String)> {
    #[cfg(target_os = "macos")]
    {
        macos_version()
    }

    #[cfg(target_os = "linux")]
    {
        linux_distribution()
    }

    #[cfg(not(any(target_os = "macos", target_os = "linux")))]
    {
        None
    }
}

#[cfg(target_os = "macos")]
fn macos_version() -> Option<(String, String)> {
    // Read from SystemVersion.plist instead of shelling out to sw_vers.
    let content =
        std::fs::read_to_string("/System/Library/CoreServices/SystemVersion.plist").ok()?;
    let version = parse_plist_key(&content, "ProductVersion")?;
    Some(("OSX".to_string(), version))
}

/// Extract a string value for `key` from a simple XML plist.
#[cfg(target_os = "macos")]
fn parse_plist_key(xml: &str, key: &str) -> Option<String> {
    let mut lines = xml.lines();
    while let Some(line) = lines.next() {
        if line.trim() == format!("<key>{}</key>", key) {
            let val_line = lines.next()?.trim().to_string();
            return val_line
                .strip_prefix("<string>")
                .and_then(|s| s.strip_suffix("</string>"))
                .map(|s| s.to_string());
        }
    }
    None
}

#[cfg(target_os = "linux")]
fn linux_distribution() -> Option<(String, String)> {
    let content = std::fs::read_to_string("/etc/os-release").ok()?;
    let mut name = None;
    let mut version = None;
    for line in content.lines() {
        if let Some(val) = line.strip_prefix("NAME=") {
            name = Some(val.trim_matches('"').to_string());
        } else if let Some(val) = line.strip_prefix("VERSION_ID=") {
            version = Some(val.trim_matches('"').to_string());
        }
    }
    Some((name?, version.unwrap_or_default()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_platform_string_not_empty() {
        let s = platform_string();
        assert!(!s.is_empty());
    }

    #[test]
    fn test_system_release_reasonable() {
        let (system, release) = system_release();
        assert!(!system.is_empty());
        assert!(!release.is_empty());
        assert_ne!(release, "unknown");
    }

    #[test]
    fn test_platform_string_contains_rattler() {
        let s = platform_string();
        assert!(
            s.contains("rattler/"),
            "expected rattler/ in platform string, got: {}",
            s
        );
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn test_macos_includes_osx() {
        let s = platform_string();
        assert!(s.contains("Darwin/"), "expected Darwin/, got: {}", s);
        assert!(s.contains("OSX/"), "expected OSX/, got: {}", s);
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn test_linux_includes_distro() {
        let s = platform_string();
        assert!(s.contains("Linux/"), "expected Linux/, got: {}", s);
    }
}
