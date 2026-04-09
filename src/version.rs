/// Parse `git describe --long` output into a PEP 440 version string.
///
/// Replicates setuptools-scm's "guess-next-dev" scheme:
///   - On an exact tag v0.0.6:        "0.0.6"
///   - 17 commits past v0.0.6:        "0.0.7.dev17"
///   - On an exact tag v1.0.0-rc1:    "1.0.0rc1"
///   - 3 commits past v1.0.0-rc1:     "1.0.0rc2.dev3"
///
/// Format: `v{tag}-{distance}-g{hash}` or `v{tag}-{distance}-g{hash}-dirty`
///
/// The tag itself may contain dashes (e.g., `v1.0.0-rc1`), so we parse from
/// the right: the last component is optionally "dirty", then the hash (g...),
/// then the distance (a number), and everything before that is the tag.
pub fn parse_git_describe(desc: &str) -> Option<String> {
    let parts: Vec<&str> = desc.split('-').collect();
    if parts.len() < 3 {
        return None;
    }

    // Parse from the right: [..., distance, gHASH] or [..., distance, gHASH, dirty]
    let (tag_parts, distance_str) = if parts.last() == Some(&"dirty") {
        if parts.len() < 4 {
            return None;
        }
        (&parts[..parts.len() - 3], parts[parts.len() - 3])
    } else {
        (&parts[..parts.len() - 2], parts[parts.len() - 2])
    };

    let tag = tag_parts.join("-");
    let distance: u32 = distance_str.parse().ok()?;

    // Strip 'v' prefix
    let version_str = tag.strip_prefix('v')?;

    // Check for rc suffix: "1.0.0-rc1" or "1.0.0"
    let (base, rc) = if let Some((base, rc_str)) = version_str.rsplit_once("-rc") {
        let rc_num: u32 = rc_str.parse().ok()?;
        (base, Some(rc_num))
    } else {
        (version_str, None)
    };

    // Parse major.minor.patch
    let semver: Vec<&str> = base.split('.').collect();
    if semver.len() != 3 {
        return None;
    }
    let major: u32 = semver[0].parse().ok()?;
    let minor: u32 = semver[1].parse().ok()?;
    let patch: u32 = semver[2].parse().ok()?;

    if distance == 0 {
        // Exact tag match
        match rc {
            Some(rc_num) => Some(format!("{}.{}.{}rc{}", major, minor, patch, rc_num)),
            None => Some(format!("{}.{}.{}", major, minor, patch)),
        }
    } else {
        // Dev version: bump the next component
        match rc {
            Some(rc_num) => Some(format!(
                "{}.{}.{}rc{}.dev{}",
                major,
                minor,
                patch,
                rc_num + 1,
                distance
            )),
            None => Some(format!("{}.{}.{}.dev{}", major, minor, patch + 1, distance)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_exact_release_tag() {
        assert_eq!(
            parse_git_describe("v0.0.6-0-gabcdef1"),
            Some("0.0.6".to_string())
        );
    }

    #[test]
    fn test_dev_version() {
        assert_eq!(
            parse_git_describe("v0.0.6-17-g8be46bf0"),
            Some("0.0.7.dev17".to_string())
        );
    }

    #[test]
    fn test_exact_rc_tag() {
        assert_eq!(
            parse_git_describe("v1.0.0-rc1-0-gabcdef1"),
            Some("1.0.0rc1".to_string())
        );
    }

    #[test]
    fn test_dev_past_rc_tag() {
        assert_eq!(
            parse_git_describe("v1.0.0-rc1-3-gabcdef1"),
            Some("1.0.0rc2.dev3".to_string())
        );
    }

    #[test]
    fn test_dirty_ignored() {
        assert_eq!(
            parse_git_describe("v0.0.6-17-g8be46bf0-dirty"),
            Some("0.0.7.dev17".to_string())
        );
    }

    #[test]
    fn test_dirty_exact_tag() {
        assert_eq!(
            parse_git_describe("v1.2.3-0-gabcdef1-dirty"),
            Some("1.2.3".to_string())
        );
    }

    #[test]
    fn test_major_minor_bump() {
        assert_eq!(
            parse_git_describe("v2.5.9-1-g1234567"),
            Some("2.5.10.dev1".to_string())
        );
    }

    #[test]
    fn test_invalid_input() {
        assert_eq!(parse_git_describe("not-a-version"), None);
        assert_eq!(parse_git_describe("v1.2-5-gabcdef1"), None);
    }
}
