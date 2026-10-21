//! Org resolution and subscriber-path HTTP for the package intelligence
//! commands. Ported from bigbend `apps/remote-mcp/remote_mcp/services/
//! org_channels.py` and `mcp/tools/org_context.py`.
//!
//! The subscriber (business-tier) path routes package lookups through
//! `GET /api/organizations/{org}/channels/{anaconda|org}/{channel}/artifacts…`,
//! which returns policy-filtered results. Resolution reads the same
//! `whoami` passport the CLI already uses for `has_premium_subscription`.

use serde::Deserialize;
use serde_json::Value;

use super::REQUEST_TIMEOUT;
use super::errors::ToolError;
use super::package_info::{SEGMENT_RULE, is_safe_segment};
use super::schemas::OrgChoiceRequired;
use crate::context::CommandContext;

/// Channels that live under the shared `anaconda` prefix rather than an org.
pub const ANACONDA_CHANNELS: &[&str] = &["main", "msys2", "r", "main-x"];

/// Business-tier product-code prefixes. An org with an active subscription
/// attribute whose id starts with one of these gets org channel access.
const BUSINESS_SUBSCRIPTION_PREFIXES: &[&str] =
    &["security_subscription", "business_subscription"];

// ---------------------------------------------------------------------------
// Passport model (whoami)
// ---------------------------------------------------------------------------

/// Passport section of the whoami response. Tolerantly parsed: only the
/// fields org resolution needs are declared, and a bad response fails the
/// whole deserialization, which callers treat as "no org" (graceful).
#[derive(Debug, Deserialize)]
pub struct WhoamiResponse {
    pub passport: Passport,
}

#[derive(Debug, Deserialize, Default)]
pub struct Passport {
    #[serde(default)]
    pub organizations: Vec<PassportOrganization>,
}

#[derive(Debug, Deserialize)]
pub struct PassportOrganization {
    pub name: String,
    #[serde(default)]
    pub attributes: Vec<PassportAttribute>,
}

#[derive(Debug, Deserialize)]
pub struct PassportAttribute {
    pub id: String,
    pub group: String,
    pub data: Option<Value>,
}

/// A resolved organization, as needed to build request paths and scope
/// channel lookups.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Org {
    pub name: String,
}

/// Outcome of org resolution.
#[derive(Debug, Clone)]
pub enum OrgResolution {
    /// The org to use.
    Resolved(Org),
    /// No business org; caller decides (free path, or an error).
    None,
    /// Several business orgs; the caller must name one.
    Choice(OrgChoiceRequired),
    /// `access_denied` or `no_subscription`.
    Error(ToolError),
}

/// True when `attribute` is an active business-tier subscription.
fn is_active_business_subscription(attr: &PassportAttribute) -> bool {
    if attr.group != "subscriptions"
        || !BUSINESS_SUBSCRIPTION_PREFIXES
            .iter()
            .any(|prefix| attr.id.starts_with(prefix))
    {
        return false;
    }
    // Active: no expiry recorded, or an expiry in the future. An unparseable
    // expiry is treated as inactive (cannot be trusted).
    match attr
        .data
        .as_ref()
        .and_then(|d| d.get("expires_at"))
        .and_then(Value::as_str)
    {
        None => true,
        Some(expires) => chrono::DateTime::parse_from_rfc3339(expires)
            .is_ok_and(|expires| expires >= chrono::Utc::now()),
    }
}

fn has_business_subscription(org: &PassportOrganization) -> bool {
    org.attributes
        .iter()
        .any(is_active_business_subscription)
}

/// Port of `resolve_org`: pick the org to scope channel lookups to.
///
/// - `org_name` given: the named org must be one of the caller's orgs
///   (`access_denied` otherwise) and have a business subscription
///   (`no_subscription` otherwise).
/// - `org_name` absent: one business org is used; several yield
///   `OrgChoiceRequired`; none yield `None`.
pub fn resolve_org(passport: &Passport, org_name: Option<&str>) -> OrgResolution {
    let business_orgs: Vec<&PassportOrganization> = passport
        .organizations
        .iter()
        .filter(|org| has_business_subscription(org))
        .collect();

    if let Some(name) = org_name {
        if !passport.organizations.iter().any(|org| org.name == name) {
            let available: Vec<String> = passport
                .organizations
                .iter()
                .map(|org| org.name.clone())
                .collect();
            return OrgResolution::Error(ToolError::new(
                "access_denied",
                format!(
                    "You do not have access to '{name}'. Your organizations: {}",
                    available.join(", ")
                ),
            ));
        }
        return match business_orgs.iter().find(|org| org.name == name) {
            Some(org) => OrgResolution::Resolved(Org {
                name: org.name.clone(),
            }),
            None => OrgResolution::Error(ToolError::new(
                "no_subscription",
                format!("Organization '{name}' does not have a business subscription."),
            )),
        };
    }

    match business_orgs.as_slice() {
        [] => OrgResolution::None,
        [org] => OrgResolution::Resolved(Org {
            name: org.name.clone(),
        }),
        many => OrgResolution::Choice(OrgChoiceRequired {
            error: "org_choice_required",
            message: "You belong to multiple organizations with channel access. \
                      Specify org_name."
                .into(),
            available_orgs: many.iter().map(|org| org.name.clone()).collect(),
        }),
    }
}

// ---------------------------------------------------------------------------
// Path helpers
// ---------------------------------------------------------------------------

/// Guard a value before it is interpolated into a request path.
fn check_segment(value: &str, field: &str) -> Result<(), ToolError> {
    if is_safe_segment(value) {
        Ok(())
    } else {
        Err(ToolError::invalid_request(format!("{field} {SEGMENT_RULE}")))
    }
}

/// Build a subscriber-path artifacts URL. Anaconda channels live under the
/// shared `anaconda` prefix; everything else under the org.
pub fn build_artifacts_path(org_name: &str, channel: &str, suffix: &str) -> String {
    if ANACONDA_CHANNELS.contains(&channel) {
        format!("/api/organizations/{org_name}/channels/anaconda/{channel}/artifacts{suffix}")
    } else {
        format!("/api/organizations/{org_name}/channels/{org_name}/{channel}/artifacts{suffix}")
    }
}

// ---------------------------------------------------------------------------
// HTTP
// ---------------------------------------------------------------------------

/// Fetch the caller's passport. `None` on any failure: callers degrade to the
/// free path or report an auth error.
pub async fn fetch_whoami(ctx: &CommandContext) -> Option<Passport> {
    let response = ctx
        .client()
        .get("/api/auth/sessions/whoami")
        .timeout(REQUEST_TIMEOUT)
        .send()
        .await
        .ok()?;
    if !response.status().is_success() {
        return None;
    }
    response.json::<WhoamiResponse>().await.ok().map(|r| r.passport)
}

/// A channel reported by `/api/organizations/{org}/channels`.
#[derive(Debug, Clone)]
pub struct OrgChannel {
    pub name: String,
}

/// Fetch the org's configured channels. `None` on failure, matching the
/// fail-open behavior of the remote service.
pub async fn fetch_org_channels(ctx: &CommandContext, org_name: &str) -> Option<Vec<OrgChannel>> {
    let path = format!("/api/organizations/{org_name}/channels");
    let response = ctx
        .client()
        .get(&path)
        .timeout(REQUEST_TIMEOUT)
        .send()
        .await
        .ok()?;
    if response.status().as_u16() != 200 {
        return None;
    }
    let data: Value = response.json().await.ok()?;
    let channels = data.as_array()?;
    Some(
        channels
            .iter()
            .filter_map(|ch| {
                let name = ch.get("name")?.as_str()?.to_string();
                Some(OrgChannel { name })
            })
            .collect(),
    )
}

/// Strip the `{org}/` prefix from an org-channel name, matching
/// `org_context.strip_prefix`.
pub fn strip_org_prefix(org_name: &str, channel_name: &str) -> String {
    let prefix = format!("{org_name}/");
    channel_name
        .strip_prefix(&prefix)
        .unwrap_or(channel_name)
        .to_string()
}

/// Step 1 of the subscriber path: artifact metadata, policy-adjusted.
pub async fn fetch_artifacts_listing(
    ctx: &CommandContext,
    org_name: &str,
    channel: &str,
    package_name: &str,
) -> Option<Value> {
    let path = build_artifacts_path(org_name, channel, "");
    let response = ctx
        .client()
        .get(&path)
        .query(&[("only_names", package_name)])
        .timeout(REQUEST_TIMEOUT)
        .send()
        .await
        .ok()?;
    if response.status().as_u16() != 200 {
        return None;
    }
    response.json().await.ok()
}

/// Step 2 of the subscriber path: files for one version, with policy flags.
/// Two-call strategy: `limit=200`, then a follow-up at `limit=total` when the
/// first page is short.
pub async fn fetch_artifact_files_for_version(
    ctx: &CommandContext,
    org_name: &str,
    channel: &str,
    package_name: &str,
    version: &str,
) -> Option<Value> {
    check_segment(package_name, "package name").ok()?;
    let path = build_artifacts_path(
        org_name,
        channel,
        &format!("/conda/{package_name}/files"),
    );
    let request = |limit: u64| {
        ctx.client()
            .get(&path)
            .query(&[
                ("version".to_string(), version.to_string()),
                ("limit".to_string(), limit.to_string()),
            ])
            .timeout(REQUEST_TIMEOUT)
            .send()
    };
    let read = |result: reqwest_middleware::Result<reqwest::Response>| async {
        match result {
            Ok(response) if response.status().as_u16() == 200 => {
                response.json::<Value>().await.ok()
            }
            _ => None,
        }
    };
    let data = read(request(200).await).await?;
    let total = data.get("total_count").and_then(Value::as_u64).unwrap_or(0);
    if total <= 200 {
        return Some(data);
    }
    read(request(total).await).await.or(Some(data))
}

/// Fetch full CVE detail for a package from the subscriber path.
pub async fn fetch_cve_detail(
    ctx: &CommandContext,
    org_name: &str,
    channel: &str,
    package_name: &str,
) -> Option<Value> {
    check_segment(package_name, "package name").ok()?;
    let path = build_artifacts_path(
        org_name,
        channel,
        &format!("/conda/{package_name}/cves"),
    );
    let response = ctx
        .client()
        .get(&path)
        .timeout(REQUEST_TIMEOUT)
        .send()
        .await
        .ok()?;
    if response.status().as_u16() != 200 {
        return None;
    }
    response.json().await.ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn attr(id: &str, group: &str, expires: Option<&str>) -> PassportAttribute {
        PassportAttribute {
            id: id.into(),
            group: group.into(),
            data: expires.map(|e| json!({"expires_at": e})),
        }
    }

    fn org(name: &str, attrs: Vec<PassportAttribute>) -> PassportOrganization {
        PassportOrganization {
            name: name.into(),
            attributes: attrs,
        }
    }

    fn business_attr(expires: Option<&str>) -> PassportAttribute {
        attr("security_subscription", "subscriptions", expires)
    }

    fn passport(orgs: Vec<PassportOrganization>) -> Passport {
        Passport { organizations: orgs }
    }

    #[test]
    fn business_tier_detection() {
        let active = org("acme", vec![business_attr(None)]);
        assert!(has_business_subscription(&active));
        // business_subscription counts too.
        let bus = org("b", vec![attr("business_subscription", "subscriptions", None)]);
        assert!(has_business_subscription(&bus));
        // Commercial/free are not business tier.
        let commercial = org("c", vec![attr("commercial_subscription", "subscriptions", None)]);
        assert!(!has_business_subscription(&commercial));
        // Wrong group.
        let wrong_group = org("g", vec![attr("security_subscription", "groups", None)]);
        assert!(!has_business_subscription(&wrong_group));
    }

    #[test]
    fn business_tier_expiry() {
        let expired = org("e", vec![business_attr(Some("2000-01-01T00:00:00Z"))]);
        assert!(!has_business_subscription(&expired));
        let unparseable = org("u", vec![business_attr(Some("not-a-date"))]);
        assert!(!has_business_subscription(&unparseable));
    }

    #[test]
    fn resolve_picks_single_org() {
        let p = passport(vec![org("acme", vec![business_attr(None)])]);
        match resolve_org(&p, None) {
            OrgResolution::Resolved(org) => assert_eq!(org.name, "acme"),
            other => panic!("unexpected {other:?}"),
        }
    }

    #[test]
    fn resolve_none_without_business_org() {
        let p = passport(vec![org("free", vec![])]);
        assert!(matches!(resolve_org(&p, None), OrgResolution::None));
    }

    #[test]
    fn resolve_multiple_requires_choice() {
        let p = passport(vec![
            org("a", vec![business_attr(None)]),
            org("b", vec![business_attr(None)]),
        ]);
        match resolve_org(&p, None) {
            OrgResolution::Choice(choice) => {
                assert_eq!(choice.error, "org_choice_required");
                assert_eq!(choice.available_orgs, vec!["a".to_string(), "b".to_string()]);
            }
            other => panic!("unexpected {other:?}"),
        }
    }

    #[test]
    fn resolve_unknown_org_is_access_denied() {
        let p = passport(vec![org("acme", vec![business_attr(None)])]);
        match resolve_org(&p, Some("nope")) {
            OrgResolution::Error(e) => {
                assert_eq!(e.error, "access_denied");
                assert!(e.message.contains("nope"));
            }
            other => panic!("unexpected {other:?}"),
        }
    }

    #[test]
    fn resolve_free_org_is_no_subscription() {
        let p = passport(vec![org("free", vec![])]);
        match resolve_org(&p, Some("free")) {
            OrgResolution::Error(e) => assert_eq!(e.error, "no_subscription"),
            other => panic!("unexpected {other:?}"),
        }
    }

    #[test]
    fn artifacts_paths() {
        assert_eq!(
            build_artifacts_path("acme", "main", ""),
            "/api/organizations/acme/channels/anaconda/main/artifacts"
        );
        assert_eq!(
            build_artifacts_path("acme", "conda-forge", "/conda/numpy/files"),
            "/api/organizations/acme/channels/acme/conda-forge/artifacts/conda/numpy/files"
        );
    }

    #[test]
    fn strips_org_prefix() {
        assert_eq!(strip_org_prefix("acme", "acme/finance"), "finance");
        assert_eq!(strip_org_prefix("acme", "main"), "main");
    }

    #[test]
    fn unsafe_segments_rejected() {
        for bad in ["../etc", "a/b", "..", "."] {
            assert!(check_segment(bad, "package name").is_err(), "{bad}");
        }
        assert!(check_segment("numpy", "package name").is_ok());
    }
}
