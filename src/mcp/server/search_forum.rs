//! `search_forum`: free-text search of the Anaconda community forum.
//!
//! Ported from bigbend `apps/remote-mcp/remote_mcp/services/search_forum.py`;
//! each helper names the Python function it ports. The forum index is served
//! by `GET {base}/api/search/forum` and needs no credentials.
//!
//! Python's `_is_utf8_encodable` check has no counterpart: a Rust `String` is
//! always valid UTF-8.

use serde::Deserialize;
use serde_json::Value;

use super::backend::{Backend, REQUEST_TIMEOUT};
use super::schemas::{ForumBlurb, ForumPagination, ForumSearchResponse, ForumTopic, ToolError};

pub const DEFAULT_PAGE: i64 = 1;
pub const DEFAULT_PAGE_SIZE: i64 = 10;
pub const MAX_QUERY_LENGTH: usize = 256;
pub const MAX_PAGE_SIZE: i64 = 100;
/// OpenSearch `index.max_result_window` default.
pub const MAX_RESULT_WINDOW: i64 = 10_000;
pub const PAGING_LIMIT_NOTE: &str = "the result count exceeds the 10000-result paging limit; \
     deeper pages are not reachable, so narrow the query rather than paging further";
const FORUM_SEARCH_PATH: &str = "/api/search/forum";
const UNAVAILABLE_MESSAGE: &str = "Forum search is currently unavailable. Try again later.";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValidatedForumArguments {
    pub query: String,
    pub page: i64,
    pub page_size: i64,
}

/// Port of `_validate_arguments`. Hand-written like the Python: trimming
/// before checking and the cross-field result-window rule do not fit a
/// derive-based validator.
pub fn validate_arguments(
    query: &str,
    page: i64,
    page_size: i64,
) -> Result<ValidatedForumArguments, ToolError> {
    let query = query.trim();
    if query.is_empty() {
        return Err(ToolError::invalid_request(
            "query must be a non-empty, non-whitespace string",
        ));
    }
    if query.chars().count() > MAX_QUERY_LENGTH {
        return Err(ToolError::invalid_request(format!(
            "query must be no longer than {MAX_QUERY_LENGTH} characters"
        )));
    }
    if page < 1 {
        return Err(ToolError::invalid_request("page must be 1 or greater"));
    }
    if !(1..=MAX_PAGE_SIZE).contains(&page_size) {
        return Err(ToolError::invalid_request(format!(
            "page_size must be between 1 and {MAX_PAGE_SIZE}"
        )));
    }
    let window_end = page.saturating_mul(page_size);
    if window_end > MAX_RESULT_WINDOW {
        let max_usable_page = MAX_RESULT_WINDOW / page_size;
        return Err(ToolError::invalid_request(format!(
            "page exceeds the maximum result window of {MAX_RESULT_WINDOW}; for page_size \
             {page_size}, the highest usable page is {max_usable_page}"
        )));
    }
    Ok(ValidatedForumArguments {
        query: query.to_string(),
        page,
        page_size,
    })
}

/// Port of `_read_total`: a non-negative integer total. Returns
/// `(total, was_sanitized)`.
fn read_total(raw: Option<&Value>) -> (i64, bool) {
    // `as_i64` is `None` for floats, so `1.5` and `1.0` are both rejected.
    match raw.and_then(Value::as_i64) {
        Some(total) if total >= 0 => (total, false),
        _ => (0, true),
    }
}

/// Port of `_safe_absolute_url`: return `url` only if it is absolute
/// http(s), has a host, carries no credentials or control characters, and
/// shares scheme, host and port with `expected_origin`.
pub fn safe_absolute_url(url: Option<&Value>, expected_origin: &str) -> Option<String> {
    let url = url?.as_str()?;
    if expected_origin.is_empty()
        || url
            .chars()
            .any(|c| c == '\\' || (c as u32) <= 0x20 || c.is_control())
    {
        return None;
    }
    let parsed_url = url::Url::parse(url).ok()?;
    let parsed_origin = url::Url::parse(expected_origin).ok()?;
    for parsed in [&parsed_url, &parsed_origin] {
        if !matches!(parsed.scheme(), "http" | "https")
            || parsed.host_str().is_none()
            || !parsed.username().is_empty()
            || parsed.password().is_some()
        {
            return None;
        }
    }
    let origin_of = |u: &url::Url| {
        (
            u.scheme().to_string(),
            u.host_str().map(str::to_ascii_lowercase),
            u.port_or_known_default(),
        )
    };
    (origin_of(&parsed_url) == origin_of(&parsed_origin)).then(|| url.to_string())
}

/// Port of `_normalize_blurb`: collapse whitespace runs to single spaces.
fn normalize_blurb(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

#[derive(Deserialize)]
struct RawBlurb {
    username: String,
    blurb: String,
    post_number: i64,
}

#[derive(Deserialize)]
struct RawTopic {
    id: i64,
    title: String,
    category: String,
    posts_count: i64,
    reply_count: i64,
    views: i64,
    created_at: Option<String>,
    last_posted_at: Option<String>,
    blurbs: Vec<RawBlurb>,
}

/// Parse an ISO-8601 timestamp and re-serialize it the way pydantic emits a
/// Python `datetime` (the remote's `ForumTopic.created_at` is typed
/// `datetime`): microseconds only when non-zero, `Z` for UTC, `±HH:MM` for
/// other offsets, no suffix for naive values. `None` if it does not parse.
fn normalize_timestamp(value: &str) -> Option<String> {
    use chrono::{DateTime, NaiveDateTime, Timelike};
    fn base(naive: &NaiveDateTime) -> String {
        let mut out = naive.format("%Y-%m-%dT%H:%M:%S").to_string();
        let micros = naive.nanosecond() / 1_000;
        if micros != 0 {
            out.push_str(&format!(".{micros:06}"));
        }
        out
    }
    if let Ok(aware) = DateTime::parse_from_rfc3339(value) {
        let offset = aware.offset().local_minus_utc();
        let suffix = if offset == 0 {
            "Z".to_string()
        } else {
            aware.format("%:z").to_string()
        };
        return Some(base(&aware.naive_local()) + &suffix);
    }
    NaiveDateTime::parse_from_str(value, "%Y-%m-%dT%H:%M:%S%.f")
        .ok()
        .map(|naive| base(&naive))
}

/// Port of `_map_topic`. Returns `Err` with a short reason when unreadable.
fn map_topic(raw: &Value, expected_origin: &str) -> Result<ForumTopic, String> {
    let Value::Object(map) = raw else {
        return Err("topic was not a mapping".to_string());
    };
    let topic: RawTopic = serde_json::from_value(raw.clone()).map_err(|e| e.to_string())?;
    let timestamp = |field: &str, value: Option<String>| match value {
        None => Ok(None),
        Some(value) => normalize_timestamp(&value)
            .map(Some)
            .ok_or_else(|| format!("{field} is not a timestamp")),
    };
    let created_at = timestamp("created_at", topic.created_at)?;
    let last_posted_at = timestamp("last_posted_at", topic.last_posted_at)?;
    Ok(ForumTopic {
        id: topic.id,
        title: topic.title,
        // An absent or unsafe url is nulled (and disclosed), never a reason to
        // drop the topic.
        url: safe_absolute_url(map.get("url"), expected_origin),
        category: topic.category,
        posts_count: topic.posts_count,
        reply_count: topic.reply_count,
        views: topic.views,
        created_at,
        last_posted_at,
        blurbs: topic
            .blurbs
            .into_iter()
            .filter_map(|b| {
                let blurb = normalize_blurb(&b.blurb);
                (!blurb.is_empty()).then_some(ForumBlurb {
                    username: b.username,
                    blurb,
                    post_number: b.post_number,
                })
            })
            .collect(),
    })
}

/// Turn a search API payload into the tool response. Port of the response
/// half of `search_forum` plus `_read_topics` and `_pagination`.
pub fn build_response(
    args: &ValidatedForumArguments,
    payload: &Value,
    expected_origin: &str,
) -> Result<ForumSearchResponse, ToolError> {
    let unavailable = || ToolError::new("search_unavailable", UNAVAILABLE_MESSAGE);
    let Value::Object(object) = payload else {
        tracing::error!("forum search response was not a JSON object");
        return Err(unavailable());
    };
    let Some(Value::Array(raw_topics)) = object.get("topics") else {
        tracing::error!("forum search response did not contain a topics list");
        return Err(unavailable());
    };
    let (mut total, total_was_sanitized) = read_total(object.get("total"));

    let mut topics = Vec::with_capacity(raw_topics.len());
    let mut parse_errors = Vec::new();
    for raw in raw_topics {
        match map_topic(raw, expected_origin) {
            Ok(topic) => topics.push(topic),
            Err(reason) => parse_errors.push(reason),
        }
    }
    let unreadable = parse_errors.len();
    if unreadable > 0 {
        tracing::error!(
            unreadable,
            examined = raw_topics.len(),
            sample = ?parse_errors.iter().take(3).collect::<Vec<_>>(),
            "forum search returned unreadable documents"
        );
    }
    if !raw_topics.is_empty() && topics.is_empty() {
        return Err(ToolError::new(
            "search_unavailable",
            format!(
                "{unreadable} forum document(s) matched but none could be read. Try again \
                 later. This is a data problem, not an empty result."
            ),
        ));
    }

    let mut notes = Vec::new();
    if raw_topics.is_empty() && total_was_sanitized {
        notes.push(
            "total was set to 0 because the search API returned an unusable total".to_string(),
        );
    }
    if !raw_topics.is_empty() {
        let minimum_total = (args.page - 1) * args.page_size + raw_topics.len() as i64;
        if total < minimum_total {
            notes.push(format!(
                "total was raised from {total} to {minimum_total} to stay consistent with the \
                 topics returned; the underlying aggregate is approximate and under-reported"
            ));
            total = minimum_total;
        }
    }

    // Ceiling division written so an untrusted, huge `total` cannot overflow.
    let total_pages = if total > 0 {
        (total - 1) / args.page_size + 1
    } else {
        0
    };
    let reachable_pages = MAX_RESULT_WINDOW / args.page_size;
    if total_pages > reachable_pages {
        notes.push(PAGING_LIMIT_NOTE.to_string());
    }
    let pagination = ForumPagination {
        page: args.page,
        page_size: args.page_size,
        total_pages: total_pages.min(reachable_pages),
    };
    if total > 0 && args.page > pagination.total_pages {
        notes.push(format!(
            "requested page {} is past the last available page ({}); use an earlier page",
            args.page, pagination.total_pages
        ));
    }
    if unreadable > 0 {
        notes.push(format!(
            "{unreadable} unreadable forum topic(s) among {} returned topic(s); {} surviving \
             topic(s); total may over-report relative to topics returned",
            raw_topics.len(),
            topics.len()
        ));
    }
    let null_urls = topics.iter().filter(|t| t.url.is_none()).count();
    if !topics.is_empty() && expected_origin.is_empty() {
        notes.push("topic url is null because no forum base URL is configured".to_string());
    } else if null_urls > 0 {
        notes.push(format!(
            "{null_urls} topic url(s) are null because the search API returned a URL that was \
             empty, not absolute, contained control characters, or was not same-origin with \
             the configured forum base URL"
        ));
    }

    Ok(ForumSearchResponse {
        topics,
        total,
        pagination,
        notes,
    })
}

/// Tool entry point.
pub async fn search_forum(
    backend: &Backend,
    query: &str,
    page: i64,
    page_size: i64,
) -> Result<ForumSearchResponse, ToolError> {
    let args = validate_arguments(query, page, page_size)?;
    let unavailable = || ToolError::new("search_unavailable", UNAVAILABLE_MESSAGE);

    let response = backend
        .public_client
        .get(FORUM_SEARCH_PATH)
        .query(&[
            ("query", args.query.as_str()),
            ("page", &args.page.to_string()),
            ("page_size", &args.page_size.to_string()),
            ("spell_check", "false"),
        ])
        .timeout(REQUEST_TIMEOUT)
        .send()
        .await
        .map_err(|e| {
            tracing::error!(error = %e, "forum search request failed");
            unavailable()
        })?;
    let status = response.status();
    if !status.is_success() {
        tracing::error!(%status, "forum search returned an error status");
        return Err(unavailable());
    }
    let payload: Value = response.json().await.map_err(|e| {
        tracing::error!(error = %e, "forum search response was not valid JSON");
        unavailable()
    })?;
    build_response(&args, &payload, &backend.config.forum_base_url)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    const ORIGIN: &str = "https://forum.anaconda.com/";

    fn topic(id: i64, title: &str, url: Value) -> Value {
        json!({
            "id": id,
            "title": title,
            "url": url,
            "category": "Product Help",
            "posts_count": 1,
            "reply_count": 0,
            "views": 980,
            "created_at": "2024-01-02T03:04:05.000Z",
            "last_posted_at": "2024-01-03T03:04:05.000Z",
            "blurbs": [{"username": "u", "blurb": "First blurb", "post_number": 1}],
        })
    }

    fn args(page: i64, page_size: i64) -> ValidatedForumArguments {
        validate_arguments("conda solve", page, page_size).unwrap()
    }

    fn single(url: Value) -> Value {
        json!({"topics": [topic(8544, "TF", url)], "total": 1})
    }

    const NULL_URL_NOTE: &str = "1 topic url(s) are null because the search API returned a URL \
        that was empty, not absolute, contained control characters, or was not same-origin with \
        the configured forum base URL";

    #[test]
    fn rejects_empty_queries() {
        for q in ["", "   "] {
            assert_eq!(
                validate_arguments(q, 1, 10).unwrap_err().error,
                "invalid_request"
            );
        }
    }

    #[test]
    fn enforces_query_length() {
        let too_long = "q".repeat(MAX_QUERY_LENGTH + 1);
        let err = validate_arguments(&too_long, 1, 10).unwrap_err();
        assert!(err.message.contains(&MAX_QUERY_LENGTH.to_string()));
        let max = "q".repeat(MAX_QUERY_LENGTH);
        assert_eq!(validate_arguments(&max, 1, 10).unwrap().query, max);
    }

    #[test]
    fn trims_query() {
        assert_eq!(
            validate_arguments("  conda solve  ", 1, 10).unwrap().query,
            "conda solve"
        );
    }

    #[test]
    fn rejects_out_of_range_paging() {
        for (page, size) in [(0, 10), (-1, 10), (1, 0), (1, 101)] {
            assert!(
                validate_arguments("q", page, size).is_err(),
                "{page}/{size}"
            );
        }
        assert_eq!(validate_arguments("q", 1, 100).unwrap().page_size, 100);
    }

    #[test]
    fn result_window_boundary() {
        assert!(validate_arguments("q", 100, 100).is_ok());
        let err = validate_arguments("q", 101, 100).unwrap_err();
        assert!(err.message.contains("10000"));
        assert!(err.message.contains("highest usable page is 100"));
    }

    #[test]
    fn maps_topics_in_order() {
        let payload = json!({
            "topics": [
                topic(1001, "First topic", json!("https://forum.anaconda.com/t/first-topic/1001")),
                topic(1002, "Second topic", json!("https://forum.anaconda.com/t/second-topic/1002")),
            ],
            "total": 2,
        });
        let result = build_response(&args(1, 10), &payload, ORIGIN).unwrap();
        let titles: Vec<_> = result.topics.iter().map(|t| t.title.as_str()).collect();
        assert_eq!(titles, ["First topic", "Second topic"]);
        assert_eq!(result.topics[0].id, 1001);
        assert_eq!(
            result.topics[0].url.as_deref(),
            Some("https://forum.anaconda.com/t/first-topic/1001")
        );
        assert_eq!(result.topics[0].blurbs[0].blurb, "First blurb");
        assert!(result.notes.is_empty());
    }

    #[test]
    fn unusable_total_is_sanitized_and_floored() {
        for total in [json!(null), json!("1"), json!(1.5), json!(true), json!(-1)] {
            let mut payload = single(json!("https://forum.anaconda.com/t/a/1"));
            payload["total"] = total;
            let result = build_response(&args(1, 10), &payload, ORIGIN).unwrap();
            assert_eq!(result.total, 1);
            assert!(
                result
                    .notes
                    .iter()
                    .any(|n| n.contains("total was raised from 0 to 1"))
            );
        }
        let mut payload = single(json!("https://forum.anaconda.com/t/a/1"));
        payload.as_object_mut().unwrap().remove("total");
        assert_eq!(
            build_response(&args(1, 10), &payload, ORIGIN)
                .unwrap()
                .total,
            1
        );
    }

    #[test]
    fn unusable_topics_shape_is_search_unavailable() {
        for payload in [
            json!({"topics": "x", "total": 1}),
            json!({"topics": null, "total": 1}),
            json!({"topics": {}, "total": 1}),
            json!({"total": 1}),
            json!([]),
        ] {
            assert_eq!(
                build_response(&args(1, 10), &payload, ORIGIN)
                    .unwrap_err()
                    .error,
                "search_unavailable"
            );
        }
    }

    #[test]
    fn partially_unmappable_topics_floor_total_on_raw_hits() {
        let payload = json!({
            "topics": [topic(1, "ok", json!("https://forum.anaconda.com/t/a/1")), {"id": "bad"}],
            "total": 0,
        });
        let result = build_response(&args(3, 10), &payload, ORIGIN).unwrap();
        assert_eq!(result.topics.len(), 1);
        assert_eq!(result.total, 22);
        assert_eq!(result.pagination.total_pages, 3);
        assert!(result.notes[0].starts_with("total was raised from 0 to 22"));
        assert!(
            result
                .notes
                .iter()
                .any(|n| n.contains("total may over-report"))
        );
    }

    #[test]
    fn empty_page_with_positive_total_is_success() {
        let payload = json!({"topics": [], "total": 25});
        let result = build_response(&args(1, 10), &payload, ORIGIN).unwrap();
        assert!(result.topics.is_empty());
        assert_eq!(result.total, 25);
    }

    #[test]
    fn empty_page_with_unusable_total_discloses_sanitization() {
        let payload = json!({"topics": [], "total": null});
        let result = build_response(&args(1, 10), &payload, ORIGIN).unwrap();
        assert_eq!(result.total, 0);
        assert_eq!(result.pagination.total_pages, 0);
        assert_eq!(
            result.notes,
            ["total was set to 0 because the search API returned an unusable total"]
        );
    }

    #[test]
    fn all_unmappable_is_not_an_empty_result() {
        let mut bad_time = topic(2, "t", json!(null));
        bad_time["created_at"] = json!("not-a-timestamp");
        let mut missing_title = topic(3, "t", json!(null));
        missing_title.as_object_mut().unwrap().remove("title");
        let payload = json!({"topics": [null, bad_time, missing_title], "total": 3});
        let err = build_response(&args(1, 10), &payload, ORIGIN).unwrap_err();
        assert_eq!(err.error, "search_unavailable");
        assert_eq!(
            err.message,
            "3 forum document(s) matched but none could be read. Try again later. This is a \
             data problem, not an empty result."
        );
    }

    #[test]
    fn blurbs_are_normalized_and_empty_ones_dropped() {
        let mut payload = single(json!("https://forum.anaconda.com/t/a/1"));
        payload["topics"][0]["blurbs"] = json!([
            {"username": "u", "blurb": "first line\n second\tline   third line", "post_number": 1},
            {"username": "u", "blurb": " \n\t ", "post_number": 2},
        ]);
        let result = build_response(&args(1, 10), &payload, ORIGIN).unwrap();
        assert_eq!(result.topics[0].blurbs.len(), 1);
        assert_eq!(
            result.topics[0].blurbs[0].blurb,
            "first line second line third line"
        );
    }

    #[test]
    fn equivalent_origins_preserve_url() {
        for (url, origin) in [
            (
                "https://forum.anaconda.com/t/topic/1",
                "HTTPS://Forum.Anaconda.com/",
            ),
            (
                "https://forum.anaconda.com/t/topic/1",
                "https://forum.anaconda.com:443/",
            ),
            (
                "https://forum.anaconda.com:443/t/topic/1",
                "https://forum.anaconda.com/",
            ),
        ] {
            assert_eq!(
                safe_absolute_url(Some(&json!(url)), origin).as_deref(),
                Some(url)
            );
        }
    }

    #[test]
    fn unsafe_urls_are_nulled_and_disclosed() {
        let unsafe_urls = [
            json!("https://evil.example.com/t/topic/1"),
            json!("/t/topic/1"),
            json!(""),
            json!(null),
            json!("https://forum.anaconda.com/t/topic/1\nextra"),
            json!("https://forum.anaconda.com/t/topic /1"),
            json!("https://forum.anaconda.com/t/topic/1\\extra"),
            json!("http://[oops"),
            json!("https://user:pw@forum.anaconda.com/t/topic/1"),
        ];
        for url in unsafe_urls {
            let result = build_response(&args(1, 10), &single(url.clone()), ORIGIN).unwrap();
            assert_eq!(result.topics.len(), 1, "{url}");
            assert!(result.topics[0].url.is_none(), "{url}");
            assert_eq!(result.notes, [NULL_URL_NOTE], "{url}");
        }
        let mut payload = single(json!(null));
        payload["topics"][0].as_object_mut().unwrap().remove("url");
        let result = build_response(&args(1, 10), &payload, ORIGIN).unwrap();
        assert_eq!(result.notes, [NULL_URL_NOTE]);
    }

    #[test]
    fn blank_origin_nulls_every_url() {
        let result = build_response(
            &args(1, 10),
            &single(json!("https://forum.anaconda.com/t/a/1")),
            "",
        )
        .unwrap();
        assert!(result.topics[0].url.is_none());
        assert_eq!(
            result.notes,
            ["topic url is null because no forum base URL is configured"]
        );
    }

    #[test]
    fn past_last_page_note() {
        let payload = json!({"topics": [], "total": 25});
        let result = build_response(&args(4, 10), &payload, ORIGIN).unwrap();
        assert_eq!(
            result.notes,
            ["requested page 4 is past the last available page (3); use an earlier page"]
        );
    }

    #[test]
    fn timestamps_match_pydantic_datetime_serialization() {
        for (input, expected) in [
            ("2024-01-02T03:04:05.000Z", Some("2024-01-02T03:04:05Z")),
            ("2024-01-02T03:04:05Z", Some("2024-01-02T03:04:05Z")),
            ("2024-01-02T03:04:05+00:00", Some("2024-01-02T03:04:05Z")),
            (
                "2022-10-12T09:31:23.304Z",
                Some("2022-10-12T09:31:23.304000Z"),
            ),
            (
                "2022-10-12T09:31:23.304000Z",
                Some("2022-10-12T09:31:23.304000Z"),
            ),
            (
                "2024-01-02T03:04:05.5+02:00",
                Some("2024-01-02T03:04:05.500000+02:00"),
            ),
            ("2024-01-02T03:04:05", Some("2024-01-02T03:04:05")),
            ("not-a-timestamp", None),
        ] {
            assert_eq!(normalize_timestamp(input).as_deref(), expected, "{input}");
        }
    }

    #[test]
    fn mapped_topics_carry_normalized_timestamps() {
        let mut payload = single(json!("https://forum.anaconda.com/t/a/1"));
        payload["topics"][0]["last_posted_at"] = json!(null);
        let result = build_response(&args(1, 10), &payload, ORIGIN).unwrap();
        assert_eq!(
            result.topics[0].created_at.as_deref(),
            Some("2024-01-02T03:04:05Z")
        );
        assert_eq!(result.topics[0].last_posted_at, None);
    }

    #[test]
    fn huge_total_does_not_overflow() {
        let payload = json!({"topics": [], "total": i64::MAX});
        let result = build_response(&args(1, 10), &payload, ORIGIN).unwrap();
        assert_eq!(result.total, i64::MAX);
        assert_eq!(result.pagination.total_pages, 1_000);
        assert_eq!(result.notes, [PAGING_LIMIT_NOTE]);
    }

    #[test]
    fn total_pages_is_a_ceiling() {
        for (total, expected) in [(1, 1), (10, 1), (11, 2), (25, 3)] {
            let payload = json!({"topics": [], "total": total});
            let result = build_response(&args(1, 10), &payload, ORIGIN).unwrap();
            assert_eq!(result.pagination.total_pages, expected, "total={total}");
        }
    }

    #[test]
    fn result_window_clamp_note() {
        let payload = json!({"topics": [], "total": 20_000});
        let result = build_response(&args(1, 10), &payload, ORIGIN).unwrap();
        assert_eq!(result.pagination.total_pages, 1_000);
        assert_eq!(result.notes, [PAGING_LIMIT_NOTE]);
    }
}
