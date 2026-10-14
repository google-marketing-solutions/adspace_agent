//! Tool/parameter naming helpers.
//!
//! These mirror the Python ADK helpers (`_to_snake_case`,
//! `rename_python_keywords`, the 60-character tool-name cap and the parameter
//! de-duplication rule) byte-for-byte so that tool filters written for the
//! Python agent (for example `displayvideo_advertisers_ad_groups_bulk_list_assigned_target`)
//! keep working unchanged.

use std::sync::LazyLock;

use regex::Regex;

/// Maximum length of a generated tool (function) name.
pub const MAX_TOOL_NAME_LEN: usize = 60;

/// Python reserved words; parameters named like these get a `param_` prefix.
const PYTHON_KEYWORDS: &[&str] = &[
    "False", "None", "True", "and", "as", "assert", "async", "await", "break", "class", "continue",
    "def", "del", "elif", "else", "except", "finally", "for", "from", "global", "if", "import",
    "in", "is", "lambda", "nonlocal", "not", "or", "pass", "raise", "return", "try", "while",
    "with", "yield",
];

static NON_ALNUM: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"[^a-zA-Z0-9]+").expect("valid regex"));
static LOWER_UPPER: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"([a-z0-9])([A-Z])").expect("valid regex"));
static ACRONYM: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"([A-Z]+)([A-Z][a-z])").expect("valid regex"));
static MULTI_UNDERSCORE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"_+").expect("valid regex"));

/// Converts a string into `snake_case` exactly like the Python ADK.
///
/// Handles lowerCamelCase, UpperCamelCase, space/dot-separated names,
/// acronyms (e.g. `REST API`) and consecutive uppercase letters.
///
/// # Example
///
/// ```
/// use adspace_agent::google_api::naming::to_snake_case;
/// assert_eq!(to_snake_case("camelCase"), "camel_case");
/// assert_eq!(to_snake_case("drive.files.list"), "drive_files_list");
/// assert_eq!(to_snake_case("youtube.v3.liveChat"), "youtube_v3_live_chat");
/// ```
pub fn to_snake_case(text: &str) -> String {
    let text = NON_ALNUM.replace_all(text, "_");
    let text = LOWER_UPPER.replace_all(&text, "${1}_${2}");
    let text = ACRONYM.replace_all(&text, "${1}_${2}");
    let text = text.to_lowercase();
    let text = MULTI_UNDERSCORE.replace_all(&text, "_");
    text.trim_matches('_').to_string()
}

/// Prefixes Python keywords with `param_` (ADK compatibility).
pub fn rename_python_keywords(name: &str) -> String {
    if PYTHON_KEYWORDS.contains(&name) {
        format!("param_{name}")
    } else {
        name.to_string()
    }
}

/// Derives the tool name for a Discovery method id such as
/// `displayvideo.advertisers.lineItems.bulkEditAssignedTargetingOptions`.
///
/// The name is snake_cased and truncated to [`MAX_TOOL_NAME_LEN`] characters.
pub fn tool_name_for_method_id(method_id: &str) -> String {
    to_snake_case(method_id)
        .chars()
        .take(MAX_TOOL_NAME_LEN)
        .collect()
}

/// Derives the LLM-facing parameter name for an API parameter.
///
/// Falls back to a location-based default (`body`, `query_param`,
/// `path_param`, `header_param`, `value`) when the original name is empty.
pub fn parameter_name(original_name: &str, location: &str) -> String {
    let inferred = rename_python_keywords(&to_snake_case(original_name));
    if !inferred.is_empty() {
        return inferred;
    }
    match location {
        "body" => "body",
        "query" => "query_param",
        "path" => "path_param",
        "header" => "header_param",
        "cookie" => "cookie_param",
        _ => "value",
    }
    .to_string()
}

/// De-duplicates parameter names in place: the first occurrence keeps its
/// name, later ones get `_0`, `_1`, ... suffixes (ADK behaviour).
pub fn dedupe_names(names: &mut [String]) {
    let mut counts: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
    for name in names.iter_mut() {
        let count = counts.entry(name.clone()).or_insert(0);
        if *count > 0 {
            let suffixed = format!("{name}_{}", *count - 1);
            *count += 1;
            *name = suffixed;
        } else {
            *count = 1;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn snake_case_matches_python_examples() {
        assert_eq!(to_snake_case("camelCase"), "camel_case");
        assert_eq!(to_snake_case("UpperCamelCase"), "upper_camel_case");
        assert_eq!(to_snake_case("space separated"), "space_separated");
        assert_eq!(to_snake_case("REST API"), "rest_api");
        assert_eq!(to_snake_case("getHTTPResponse"), "get_http_response");
        assert_eq!(to_snake_case("__leading__"), "leading");
    }

    #[test]
    fn tool_names_match_python_filters() {
        assert_eq!(
            tool_name_for_method_id(
                "displayvideo.advertisers.lineItems.bulkEditAssignedTargetingOptions"
            ),
            "displayvideo_advertisers_line_items_bulk_edit_assigned_targe"
        );
        assert_eq!(
            tool_name_for_method_id(
                "displayvideo.advertisers.campaigns.listAssignedTargetingOptions"
            ),
            "displayvideo_advertisers_campaigns_list_assigned_targeting_o"
        );
        assert_eq!(
            tool_name_for_method_id("youtube.youtube.v3.liveChat.messages.stream"),
            "youtube_youtube_v3_live_chat_messages_stream"
        );
        assert_eq!(
            tool_name_for_method_id("googleads.customers.googleAds.search"),
            "googleads_customers_google_ads_search"
        );
        assert_eq!(
            tool_name_for_method_id("googleads.customers.generateAudienceCompositionInsights"),
            "googleads_customers_generate_audience_composition_insights"
        );
        assert_eq!(
            tool_name_for_method_id("drive.teamdrives.list"),
            "drive_teamdrives_list"
        );
        assert_eq!(
            tool_name_for_method_id("dfareporting.eventTags.insert"),
            "dfareporting_event_tags_insert"
        );
    }

    #[test]
    fn parameter_names_handle_keywords_and_defaults() {
        assert_eq!(parameter_name("pageToken", "query"), "page_token");
        assert_eq!(parameter_name("from", "query"), "param_from");
        assert_eq!(parameter_name("", "body"), "body");
        assert_eq!(parameter_name("", "query"), "query_param");
        assert_eq!(parameter_name("", "path"), "path_param");
        assert_eq!(parameter_name("", "header"), "header_param");
        assert_eq!(parameter_name("", ""), "value");
    }

    #[test]
    fn dedupe_adds_numeric_suffixes() {
        let mut names = vec![
            "name".to_string(),
            "id".to_string(),
            "name".to_string(),
            "name".to_string(),
        ];
        dedupe_names(&mut names);
        assert_eq!(names, vec!["name", "id", "name_0", "name_1"]);
    }
}
