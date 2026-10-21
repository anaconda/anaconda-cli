//! `ana package policy`: organization channel and policy configuration.
//!
//! Ported from bigbend `apps/remote-mcp/remote_mcp/services/org_config.py`.
//! Fetches the org's channels and repo policies in parallel, flattens the
//! nested policy filters into agent-friendly blocks/overrides, and attaches
//! each channel's assigned policy.

use std::collections::HashMap;

use reqwest::StatusCode;
use serde_json::{Map, Value};

use super::REQUEST_TIMEOUT;
use super::errors::ToolError;
use super::schemas::{AssignedPolicy, ChannelConfig, OrgConfigResponse};
use crate::context::CommandContext;

/// Flattened policy: optional `blocks` and `overrides` rule lists.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct FlattenedPolicy {
    pub blocks: Option<Vec<Value>>,
    pub overrides: Option<Vec<Value>>,
}

/// Port of `_format_rule`: one `simple` filter node to an agent-friendly rule.
/// Returns `None` when the node has no `facet` or `criterion`.
fn format_rule(node: &Value) -> Option<Value> {
    let facet = node.get("facet")?.as_str()?;
    let comparator = node.get("comparator").and_then(Value::as_str).unwrap_or("");
    let criterion = node.get("criterion")?;
    let mut map = Map::new();
    map.insert("facet".into(), Value::String(facet.to_string()));
    if comparator == "!=" || comparator == "not in" {
        map.insert("not".into(), criterion.clone());
    } else if facet == "package_age" {
        map.insert(
            "match".into(),
            Value::String(format!("{comparator} {criterion} days")),
        );
    } else if facet == "cve_score" {
        map.insert(
            "match".into(),
            Value::String(format!("{comparator} {criterion}")),
        );
    } else {
        map.insert("match".into(), criterion.clone());
    }
    Some(Value::Object(map))
}

/// Port of `_and_group_or_single`.
fn and_group_or_single(rules: Vec<Value>) -> Option<Value> {
    match rules.len() {
        0 => None,
        1 => rules.into_iter().next(),
        _ => Some(Value::Object({
            let mut map = Map::new();
            map.insert("all_of".into(), Value::Array(rules));
            map
        })),
    }
}

/// Port of `_extract_and_group`: simple rules under an `and` compound.
fn extract_and_group(node: &Value) -> Vec<Value> {
    let mut rules = Vec::new();
    for sub in node
        .get("subfilters")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        match sub.get("type").and_then(Value::as_str) {
            Some("simple") => {
                if let Some(rule) = format_rule(sub) {
                    rules.push(rule);
                }
            }
            Some("compound") => {
                for inner in sub
                    .get("subfilters")
                    .and_then(Value::as_array)
                    .into_iter()
                    .flatten()
                {
                    if inner.get("type").and_then(Value::as_str) == Some("simple")
                        && let Some(rule) = format_rule(inner)
                    {
                        rules.push(rule);
                    }
                }
            }
            _ => {}
        }
    }
    rules
}

/// Port of `_extract_section`: flat rule groups from a compound-OR section.
fn extract_section(section: &Value) -> Vec<Value> {
    if !section.is_object() {
        return Vec::new();
    }
    let mut groups = Vec::new();
    for group_node in section
        .get("subfilters")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        match group_node.get("type").and_then(Value::as_str) {
            Some("simple") => {
                if let Some(rule) = format_rule(group_node) {
                    groups.push(rule);
                }
            }
            Some("compound") => {
                if group_node.get("operator").and_then(Value::as_str) == Some("and") {
                    if let Some(group) = and_group_or_single(extract_and_group(group_node)) {
                        groups.push(group);
                    }
                } else {
                    for inner in group_node
                        .get("subfilters")
                        .and_then(Value::as_array)
                        .into_iter()
                        .flatten()
                    {
                        match inner.get("type").and_then(Value::as_str) {
                            Some("simple") => {
                                if let Some(rule) = format_rule(inner) {
                                    groups.push(rule);
                                }
                            }
                            Some("compound")
                                if inner.get("operator").and_then(Value::as_str) == Some("and") =>
                            {
                                if let Some(group) =
                                    and_group_or_single(extract_and_group(inner))
                                {
                                    groups.push(group);
                                }
                            }
                            _ => {}
                        }
                    }
                }
            }
            _ => {}
        }
    }
    groups
}

/// Port of `flatten_policy_filter`: collapse a nested policy filter into
/// blocks (remove/exclude) and overrides (add/include) lists.
///
/// Handles the V2 schema (`add_packages` / `remove_packages`) and the V1
/// positional layout (`subfilters[0]` = blocks, `subfilters[1]` = overrides).
pub fn flatten_policy_filter(raw: Option<&Value>, schema_version: i64) -> FlattenedPolicy {
    let mut result = FlattenedPolicy::default();
    let Some(raw) = raw else {
        return result;
    };

    let (blocks, overrides) = if schema_version == 2 && raw.get("add_packages").is_some() {
        (
            extract_section(raw.get("remove_packages").unwrap_or(&Value::Null)),
            extract_section(raw.get("add_packages").unwrap_or(&Value::Null)),
        )
    } else {
        let top = raw.get("subfilters").and_then(Value::as_array);
        (
            top.and_then(|s| s.first()).map(extract_section).unwrap_or_default(),
            top.and_then(|s| s.get(1)).map(extract_section).unwrap_or_default(),
        )
    };

    result.blocks = (!blocks.is_empty()).then_some(blocks);
    result.overrides = (!overrides.is_empty()).then_some(overrides);
    result
}

/// Attach a policy name to its flattened filter, mirroring `org_config`'s
/// `policy_entry = {"name": …}` + flat keys.
fn assigned_policy(name: String, flat: FlattenedPolicy) -> AssignedPolicy {
    AssignedPolicy {
        name,
        blocks: flat.blocks,
        overrides: flat.overrides,
    }
}

/// Command entry: check credentials, resolve the org, then fetch its
/// configuration. Port of the `org_config` tool's org-resolution wrapper.
pub async fn run_policy(
    ctx: &CommandContext,
    org: Option<String>,
) -> Result<OrgConfigResponse, ToolError> {
    if !super::has_credentials(&ctx.config) {
        return Err(ToolError::unauthenticated());
    }

    let passport = super::org::fetch_whoami(ctx).await.ok_or_else(|| {
        ToolError::new(
            "upstream_error",
            "Could not retrieve your organization memberships. Try again later.",
        )
    })?;
    let org_name = match super::org::resolve_org(&passport, org.as_deref()) {
        super::org::OrgResolution::Resolved(org) => org.name,
        super::org::OrgResolution::None => {
            return Err(ToolError::new(
                "no_subscription",
                "None of your organizations have a business subscription with channel access.",
            ));
        }
        super::org::OrgResolution::Choice(choice) => {
            return Err(ToolError::org_choice(choice.message, choice.available_orgs));
        }
        super::org::OrgResolution::Error(error) => return Err(error),
    };

    get_org_config(ctx, &org_name).await
}

/// Fetch channels and policies for `org_name` and assemble the response.
/// Port of `get_org_config`.
pub async fn get_org_config(
    ctx: &CommandContext,
    org_name: &str,
) -> Result<OrgConfigResponse, ToolError> {
    let channels_path = format!("/api/organizations/{org_name}/channels");
    let policies_path = format!("/api/organizations/{org_name}/repo-policies/v2");

    let (channels_resp, policies_resp) = futures_util::join!(
        ctx.client().get(&channels_path).timeout(REQUEST_TIMEOUT).send(),
        ctx.client().get(&policies_path).timeout(REQUEST_TIMEOUT).send(),
    );
    let channels_resp = channels_resp.map_err(|e| {
        ToolError::new(
            "upstream_error",
            format!("Failed to fetch org configuration: {e}"),
        )
    })?;
    let policies_resp = policies_resp.map_err(|e| {
        ToolError::new(
            "upstream_error",
            format!("Failed to fetch org configuration: {e}"),
        )
    })?;

    let channels_status = channels_resp.status();
    let channels_body: Option<Value> = channels_resp.json().await.ok();
    if channels_status != StatusCode::OK {
        let (upstream_code, upstream_msg) = channels_body
            .as_ref()
            .and_then(|body| {
                let error = body.get("error")?;
                Some((
                    error.get("code").and_then(Value::as_str).unwrap_or("").to_string(),
                    error
                        .get("message")
                        .and_then(Value::as_str)
                        .unwrap_or("")
                        .to_string(),
                ))
            })
            .unwrap_or_default();
        if channels_status == StatusCode::FORBIDDEN && upstream_code == "access_denied" {
            return Err(ToolError::new(
                "no_subscription",
                format!(
                    "Organization '{org_name}' does not have an active subscription. \
                     Channel and policy configuration requires a paid plan."
                ),
            ));
        }
        let suffix = if upstream_msg.is_empty() {
            ".".to_string()
        } else {
            format!(": {upstream_msg}")
        };
        return Err(ToolError::new(
            "upstream_error",
            format!("Channels endpoint returned {}{suffix}", channels_status.as_u16()),
        ));
    }

    let channels_data: Value = channels_body.ok_or_else(|| {
        ToolError::new(
            "upstream_error",
            "Channels endpoint returned non-JSON response.",
        )
    })?;

    let mut policies_by_id: HashMap<String, AssignedPolicy> = HashMap::new();
    let mut notes = Vec::new();
    if policies_resp.status() == StatusCode::OK {
        if let Ok(policies_data) = policies_resp.json::<Value>().await
            && let Some(items) = policies_data.get("items").and_then(Value::as_array)
        {
            for policy in items {
                let flat = flatten_policy_filter(
                    policy.get("filter"),
                    policy.get("schema_version").and_then(Value::as_i64).unwrap_or(1),
                );
                policies_by_id.insert(
                    policy.get("id").and_then(Value::as_str).unwrap_or("").to_string(),
                    assigned_policy(
                        policy
                            .get("name")
                            .and_then(Value::as_str)
                            .unwrap_or("")
                            .to_string(),
                        flat,
                    ),
                );
            }
        }
    } else {
        notes.push(format!(
            "Policy data unavailable (HTTP {}). Assigned policies may be missing.",
            policies_resp.status().as_u16()
        ));
    }

    let org_prefix = format!("{org_name}/");
    let channels = channels_data
        .as_array()
        .into_iter()
        .flatten()
        .map(|ch| {
            let raw_name = ch.get("name").and_then(Value::as_str).unwrap_or("");
            let assigned = ch
                .get("assigned_policy")
                .and_then(|a| a.get("id"))
                .and_then(Value::as_str)
                .and_then(|id| policies_by_id.get(id))
                .cloned();
            let name = raw_name
                .strip_prefix(&org_prefix)
                .unwrap_or(raw_name)
                .to_string();
            ChannelConfig {
                name,
                channel_type: ch.get("type").and_then(Value::as_str).unwrap_or("").to_string(),
                privacy: ch.get("privacy").and_then(Value::as_str).unwrap_or("").to_string(),
                url: format!("https://repo.anaconda.cloud/repo/{raw_name}"),
                assigned_policy: assigned,
            }
        })
        .collect();

    Ok(OrgConfigResponse {
        org_name: org_name.to_string(),
        channels,
        notes,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn single_block_rule() {
        let raw = json!({"type":"compound","operator":"or","subfilters":[
            {"type":"compound","operator":"or","subfilters":[
                {"type":"compound","operator":"and","subfilters":[
                    {"type":"simple","facet":"cve_score","comparator":">","criterion":8}
                ]}
            ]}
        ]});
        let flat = flatten_policy_filter(Some(&raw), 1);
        assert_eq!(
            flat.blocks,
            Some(vec![json!({"facet":"cve_score","match":"> 8"})])
        );
        assert_eq!(flat.overrides, None);
    }

    #[test]
    fn and_group_in_blocks() {
        let raw = json!({"type":"compound","operator":"or","subfilters":[
            {"type":"compound","operator":"or","subfilters":[
                {"type":"compound","operator":"and","subfilters":[
                    {"type":"simple","facet":"cve_score","comparator":">","criterion":7},
                    {"type":"simple","facet":"cve_status","comparator":"==","criterion":"active"}
                ]},
                {"type":"compound","operator":"and","subfilters":[
                    {"type":"simple","facet":"cve_score","comparator":">","criterion":7},
                    {"type":"simple","facet":"cve_status","comparator":"==","criterion":"reported"}
                ]}
            ]}
        ]});
        let flat = flatten_policy_filter(Some(&raw), 1);
        assert_eq!(
            flat.blocks,
            Some(vec![
                json!({"all_of":[
                    {"facet":"cve_score","match":"> 7"},
                    {"facet":"cve_status","match":"active"}
                ]}),
                json!({"all_of":[
                    {"facet":"cve_score","match":"> 7"},
                    {"facet":"cve_status","match":"reported"}
                ]}),
            ])
        );
    }

    #[test]
    fn blocks_and_overrides() {
        let raw = json!({"type":"compound","operator":"or","subfilters":[
            {"type":"compound","operator":"or","subfilters":[
                {"type":"compound","operator":"and","subfilters":[
                    {"type":"simple","facet":"conda_spec","comparator":"==","criterion":"*"}
                ]}
            ]},
            {"type":"compound","operator":"or","subfilters":[
                {"type":"compound","operator":"and","subfilters":[
                    {"type":"simple","facet":"conda_spec","comparator":"==","criterion":"python"}
                ]}
            ]}
        ]});
        let flat = flatten_policy_filter(Some(&raw), 1);
        assert_eq!(flat.blocks, Some(vec![json!({"facet":"conda_spec","match":"*"})]));
        assert_eq!(
            flat.overrides,
            Some(vec![json!({"facet":"conda_spec","match":"python"})])
        );
    }

    #[test]
    fn platform_not_rules() {
        let raw = json!({"type":"compound","operator":"or","subfilters":[
            {"type":"compound","operator":"or","subfilters":[
                {"type":"compound","operator":"and","subfilters":[
                    {"type":"simple","facet":"platform","comparator":"!=","criterion":"osx-arm64"},
                    {"type":"simple","facet":"platform","comparator":"!=","criterion":"noarch"}
                ]}
            ]}
        ]});
        let flat = flatten_policy_filter(Some(&raw), 1);
        assert_eq!(
            flat.blocks,
            Some(vec![json!({"all_of":[
                {"facet":"platform","not":"osx-arm64"},
                {"facet":"platform","not":"noarch"}
            ]})])
        );
    }

    #[test]
    fn empty_and_none_filters() {
        assert_eq!(flatten_policy_filter(None, 1), FlattenedPolicy::default());
        let raw = json!({"type":"compound","operator":"or","subfilters":[]});
        assert_eq!(flatten_policy_filter(Some(&raw), 1), FlattenedPolicy::default());
    }

    #[test]
    fn v2_schema() {
        let raw = json!({
            "add_packages": {"type":"compound","operator":"or","subfilters":[
                {"type":"compound","operator":"and","subfilters":[
                    {"type":"simple","facet":"conda_spec","comparator":"==","criterion":"*"}
                ]}
            ]},
            "remove_packages": {"type":"compound","operator":"or","subfilters":[
                {"type":"compound","operator":"and","subfilters":[
                    {"type":"simple","facet":"cve_score","comparator":">=","criterion":9.0}
                ]}
            ]}
        });
        let flat = flatten_policy_filter(Some(&raw), 2);
        assert_eq!(flat.blocks, Some(vec![json!({"facet":"cve_score","match":">= 9.0"})]));
        assert_eq!(flat.overrides, Some(vec![json!({"facet":"conda_spec","match":"*"})]));
    }
}
