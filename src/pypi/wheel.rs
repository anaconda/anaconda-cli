//! Wheel filename parsing (PEP 427) and metadata extraction.
//!
//! Upstream candidate: part of `rattler_pypi_installer`

use pep440_rs::Version;

/// Parsed wheel filename per PEP 427.
#[derive(Debug, Clone)]
pub struct WheelFilename {
    #[allow(dead_code)] // Used in dist_info_dir; will be used for install paths
    pub distribution: String,
    pub version: Version,
    #[allow(dead_code)] // Parsed for completeness; build tags used in wheel selection
    pub build_tag: Option<String>,
    pub python_tags: Vec<String>,
    pub abi_tags: Vec<String>,
    pub platform_tags: Vec<String>,
}

impl WheelFilename {
    /// Parse a wheel filename.
    ///
    /// Format: `{dist}-{version}(-{build})?-{python}-{abi}-{platform}.whl`
    pub fn parse(filename: &str) -> Result<Self, String> {
        let stem = filename
            .strip_suffix(".whl")
            .ok_or_else(|| format!("Not a wheel filename: {}", filename))?;

        let parts: Vec<&str> = stem.split('-').collect();

        // Minimum: dist-version-python-abi-platform (5 parts)
        // With build tag: dist-version-build-python-abi-platform (6 parts)
        if parts.len() < 5 || parts.len() > 6 {
            return Err(format!(
                "Invalid wheel filename (expected 5-6 dash-separated parts): {}",
                filename
            ));
        }

        let (distribution, version_str, build_tag, python, abi, platform) = if parts.len() == 6 {
            (
                parts[0],
                parts[1],
                Some(parts[2]),
                parts[3],
                parts[4],
                parts[5],
            )
        } else {
            (parts[0], parts[1], None, parts[2], parts[3], parts[4])
        };

        let version = version_str
            .parse::<Version>()
            .map_err(|e| format!("Invalid version '{}' in wheel filename: {}", version_str, e))?;

        Ok(Self {
            distribution: distribution.to_string(),
            version,
            build_tag: build_tag.map(String::from),
            python_tags: python.split('.').map(String::from).collect(),
            abi_tags: abi.split('.').map(String::from).collect(),
            platform_tags: platform.split('.').map(String::from).collect(),
        })
    }

    /// Check if this wheel is a pure Python wheel (platform-independent).
    pub fn is_pure_python(&self) -> bool {
        self.platform_tags.iter().all(|t| t == "any") && self.abi_tags.iter().all(|t| t == "none")
    }

    /// Check if this wheel is compatible with the given Python version and platform.
    pub fn is_compatible(&self, python_version: (u32, u32), platform_tags: &[&str]) -> bool {
        let python_ok = self.python_tags.iter().any(|tag| {
            // Accept "py3", "py2.py3", or specific "cpXY"
            tag == "py3"
                || tag == "py2.py3"
                || tag == &format!("cp{}{}", python_version.0, python_version.1)
                || tag == &format!("cp{}", python_version.0)
        });

        let platform_ok = self
            .platform_tags
            .iter()
            .any(|tag| tag == "any" || platform_tags.iter().any(|pt| tag == *pt));

        python_ok && platform_ok
    }

    /// The dist-info directory name inside the wheel.
    #[allow(dead_code)] // Will be used for wheel installation
    pub fn dist_info_dir(&self) -> String {
        format!("{}-{}.dist-info", self.distribution, self.version)
    }
}

/// Check if a filename is a wheel (vs source distribution).
pub fn is_wheel(filename: &str) -> bool {
    filename.ends_with(".whl")
}

/// Check if a filename is a source distribution.
#[allow(dead_code)] // Will be used for sdist filtering during resolution
pub fn is_sdist(filename: &str) -> bool {
    filename.ends_with(".tar.gz") || filename.ends_with(".zip")
}

/// Extract METADATA from a wheel file (ZIP).
#[allow(dead_code)] // Will be used when PEP 658 metadata is unavailable
pub fn extract_metadata(wheel_bytes: &[u8]) -> Result<String, String> {
    let reader = std::io::Cursor::new(wheel_bytes);
    let mut archive =
        zip::ZipArchive::new(reader).map_err(|e| format!("Failed to open wheel: {}", e))?;

    // Find the METADATA file in the dist-info directory
    let metadata_path = (0..archive.len())
        .filter_map(|i| {
            let name = archive.by_index(i).ok()?.name().to_string();
            if name.ends_with(".dist-info/METADATA") {
                Some(name)
            } else {
                None
            }
        })
        .next()
        .ok_or("No METADATA file found in wheel")?;

    let mut file = archive
        .by_name(&metadata_path)
        .map_err(|e| format!("Failed to read METADATA: {}", e))?;

    let mut content = String::new();
    std::io::Read::read_to_string(&mut file, &mut content)
        .map_err(|e| format!("Failed to read METADATA content: {}", e))?;

    Ok(content)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_wheel_simple() {
        let whl = WheelFilename::parse("requests-2.31.0-py3-none-any.whl").unwrap();
        assert_eq!(whl.distribution, "requests");
        assert_eq!(whl.version.to_string(), "2.31.0");
        assert!(whl.build_tag.is_none());
        assert_eq!(whl.python_tags, vec!["py3"]);
        assert_eq!(whl.abi_tags, vec!["none"]);
        assert_eq!(whl.platform_tags, vec!["any"]);
        assert!(whl.is_pure_python());
    }

    #[test]
    fn test_parse_wheel_native() {
        let whl = WheelFilename::parse(
            "numpy-1.26.4-cp312-cp312-manylinux_2_17_x86_64.manylinux2014_x86_64.whl",
        )
        .unwrap();
        assert_eq!(whl.distribution, "numpy");
        assert_eq!(whl.version.to_string(), "1.26.4");
        assert_eq!(whl.python_tags, vec!["cp312"]);
        assert_eq!(whl.abi_tags, vec!["cp312"]);
        assert_eq!(whl.platform_tags.len(), 2);
        assert!(!whl.is_pure_python());
    }

    #[test]
    fn test_parse_wheel_with_build_tag() {
        let whl = WheelFilename::parse("package-1.0.0-1-py3-none-any.whl").unwrap();
        assert_eq!(whl.build_tag, Some("1".to_string()));
    }

    #[test]
    fn test_parse_wheel_invalid_extension() {
        assert!(WheelFilename::parse("requests-2.31.0.tar.gz").is_err());
    }

    #[test]
    fn test_is_wheel() {
        assert!(is_wheel("requests-2.31.0-py3-none-any.whl"));
        assert!(!is_wheel("requests-2.31.0.tar.gz"));
    }

    #[test]
    fn test_is_sdist() {
        assert!(is_sdist("requests-2.31.0.tar.gz"));
        assert!(is_sdist("requests-2.31.0.zip"));
        assert!(!is_sdist("requests-2.31.0-py3-none-any.whl"));
    }

    #[test]
    fn test_is_compatible_pure_python() {
        let whl = WheelFilename::parse("requests-2.31.0-py3-none-any.whl").unwrap();
        assert!(whl.is_compatible((3, 12), &["manylinux_2_17_x86_64"]));
        assert!(whl.is_compatible((3, 8), &["macosx_11_0_arm64"]));
    }

    #[test]
    fn test_is_compatible_native() {
        let whl = WheelFilename::parse("numpy-1.26.4-cp312-cp312-macosx_11_0_arm64.whl").unwrap();
        assert!(whl.is_compatible((3, 12), &["macosx_11_0_arm64"]));
        assert!(!whl.is_compatible((3, 11), &["macosx_11_0_arm64"]));
        assert!(!whl.is_compatible((3, 12), &["manylinux_2_17_x86_64"]));
    }

    #[test]
    fn test_dist_info_dir() {
        let whl = WheelFilename::parse("requests-2.31.0-py3-none-any.whl").unwrap();
        assert_eq!(whl.dist_info_dir(), "requests-2.31.0.dist-info");
    }

    // -- parse edge cases --

    #[test]
    fn test_parse_wheel_too_few_parts() {
        let result = WheelFilename::parse("bad-name.whl");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("5-6 dash-separated"));
    }

    #[test]
    fn test_parse_wheel_too_many_parts() {
        let result = WheelFilename::parse("a-b-c-d-e-f-g.whl");
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_wheel_not_whl() {
        let result = WheelFilename::parse("package-1.0.0-py3-none-any.zip");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Not a wheel"));
    }

    #[test]
    fn test_parse_wheel_post_release() {
        let whl = WheelFilename::parse("package-1.0.0.post1-py3-none-any.whl").unwrap();
        assert_eq!(whl.version.to_string(), "1.0.0.post1");
    }

    #[test]
    fn test_parse_wheel_multiple_platform_tags() {
        let whl = WheelFilename::parse(
            "numpy-1.26.4-cp312-cp312-manylinux_2_17_x86_64.manylinux2014_x86_64.whl",
        )
        .unwrap();
        assert_eq!(whl.platform_tags.len(), 2);
        assert!(
            whl.platform_tags
                .contains(&"manylinux_2_17_x86_64".to_string())
        );
        assert!(
            whl.platform_tags
                .contains(&"manylinux2014_x86_64".to_string())
        );
    }

    // -- is_compatible edge cases --

    #[test]
    fn test_is_compatible_py2_py3_tag() {
        let whl = WheelFilename::parse("six-1.16.0-py2.py3-none-any.whl").unwrap();
        assert!(whl.is_compatible((3, 12), &["manylinux_2_17_x86_64"]));
        assert!(whl.is_compatible((3, 8), &["macosx_11_0_arm64"]));
    }

    #[test]
    fn test_is_compatible_cp_major_only() {
        // "cp3" tag should match any Python 3.x
        let whl = WheelFilename {
            distribution: "test".to_string(),
            version: "1.0.0".parse().unwrap(),
            build_tag: None,
            python_tags: vec!["cp3".to_string()],
            abi_tags: vec!["none".to_string()],
            platform_tags: vec!["any".to_string()],
        };
        assert!(whl.is_compatible((3, 12), &["any"]));
        assert!(whl.is_compatible((3, 8), &["any"]));
    }

    #[test]
    fn test_is_not_compatible_wrong_python() {
        let whl = WheelFilename::parse("package-1.0.0-cp311-cp311-macosx_11_0_arm64.whl").unwrap();
        assert!(!whl.is_compatible((3, 12), &["macosx_11_0_arm64"]));
    }

    #[test]
    fn test_is_not_compatible_wrong_platform() {
        let whl =
            WheelFilename::parse("package-1.0.0-cp312-cp312-manylinux_2_17_x86_64.whl").unwrap();
        assert!(!whl.is_compatible((3, 12), &["macosx_11_0_arm64"]));
    }

    // -- is_pure_python --

    #[test]
    fn test_is_pure_python_false_with_abi() {
        let whl = WheelFilename::parse("numpy-1.26.4-cp312-cp312-macosx_11_0_arm64.whl").unwrap();
        assert!(!whl.is_pure_python());
    }

    // -- is_wheel / is_sdist --

    #[test]
    fn test_is_sdist_zip() {
        assert!(is_sdist("package-1.0.0.zip"));
    }

    #[test]
    fn test_is_not_sdist_whl() {
        assert!(!is_sdist("package-1.0.0-py3-none-any.whl"));
    }
}
