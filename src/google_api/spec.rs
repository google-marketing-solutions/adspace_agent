// Copyright 2026 Google LLC
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     https://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

//! Turns Discovery methods into LLM-callable method specifications.
//!
//! This is the Rust counterpart of the Python ADK pipeline
//! `GoogleApiToOpenApiConverter` → `OpenAPIToolset`/`OperationParser` →
//! `RestApiTool`: every REST method becomes a tool whose parameters are the
//! path parameters, the query parameters and the (flattened) top-level
//! properties of the request body, all renamed to `snake_case`.

use std::fmt::Write as _;

use anyhow::bail;
use indexmap::IndexMap;
use serde_json::{Map, Value, json};

use super::discovery::{DiscoveryDocument, DiscoveryMethod, DiscoveryParameter};
use super::naming::{dedupe_names, parameter_name, tool_name_for_method_id};

/// Default maximum nesting depth for resolved request schemas.
pub const DEFAULT_MAX_SCHEMA_DEPTH: usize = 4;

/// Where a parameter is sent.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ParamLocation {
    /// Substituted into the URL path.
    Path,
    /// Appended to the query string.
    Query,
    /// Part of the JSON request body.
    Body,
    /// Sent as an HTTP header.
    Header,
}

impl ParamLocation {
    fn as_str(self) -> &'static str {
        match self {
            Self::Path => "path",
            Self::Query => "query",
            Self::Body => "body",
            Self::Header => "header",
        }
    }
}

/// How the request body is assembled from the tool arguments.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BodyKind {
    /// The method has no request body.
    None,
    /// The body is an object whose top-level properties are individual params.
    Object,
    /// The body is an array passed through a single `array` param.
    Array,
    /// The body is a scalar/untyped value passed through a single `body` param.
    Whole,
}

/// A single LLM-facing parameter of a method.
#[derive(Debug, Clone, PartialEq)]
pub struct ApiParameter {
    /// Name used by the REST API (`pageToken`, `customersId`, ...).
    pub original_name: String,
    /// Name exposed to the LLM (`page_token`, `customers_id`, ...).
    pub name: String,
    /// Where the parameter is sent.
    pub location: ParamLocation,
    /// JSON schema (with description) for the parameter.
    pub schema: Value,
    /// Whether the LLM must supply it.
    pub required: bool,
    /// Path parameters declared as `{+name}` (reserved expansion keeps `/`).
    pub reserved_expansion: bool,
}

/// Default per-tool declaration budget (bytes of serialised parameter schema
/// plus description). Tools above it are re-resolved with a shallower
/// request schema until they fit; see [`SpecOptions::max_tool_bytes`].
///
/// 12 KB keeps every CM360 `ads` / `reports` method (~11.5 KB) at full
/// depth and only collapses the outliers: CM360 creatives / placements
/// (15-18 KB), DV360 creatives / targeting options (13-21 KB) and Merchant
/// Center product inputs (32 KB).
pub const DEFAULT_MAX_TOOL_BYTES: usize = 12 * 1024;

/// How much of a nested request-body property's description is kept.
/// Top-level parameters (path, query, and the request body's own top-level
/// properties) always keep their full description.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum NestedDescriptions {
    /// The full Discovery text.
    Full,
    /// Only the first sentence. Google API descriptions lead with what the
    /// field is and follow with paragraphs of caveats and examples; the
    /// first sentence carries most of the value at a fraction of the tokens.
    #[default]
    FirstSentence,
    /// No description at all.
    None,
}

impl NestedDescriptions {
    /// Parses an environment value: `full`/`true`, `first_sentence`/`brief`,
    /// `none`/`false`. Unknown values return `None`.
    pub fn parse(raw: &str) -> Option<Self> {
        match raw.trim().to_lowercase().as_str() {
            "full" | "1" | "true" | "yes" | "on" => Some(Self::Full),
            "first_sentence" | "first-sentence" | "first" | "brief" | "short" => {
                Some(Self::FirstSentence)
            }
            "none" | "0" | "false" | "no" | "off" => Some(Self::None),
            _ => None,
        }
    }
}

/// Options controlling spec generation.
#[derive(Debug, Clone, Copy)]
pub struct SpecOptions {
    /// Maximum nesting depth when resolving request schemas; deeper objects
    /// are summarised as an untyped object to keep declarations compact.
    pub max_schema_depth: usize,
    /// Append per-value `enumDescriptions` to enum fields. Off by default:
    /// Google APIs document every enum value verbosely, which multiplies the
    /// declaration size the model must read on each request.
    pub include_enum_descriptions: bool,
    /// How much of each nested request-body property's description to keep.
    pub nested_descriptions: NestedDescriptions,
    /// Drop request-body properties the API documents as output-only /
    /// read-only (`"Output only. ..."`, `"This is a read-only field."`).
    /// The server ignores them on writes, so they only cost tokens and
    /// invite the model to invent values for them.
    pub strip_output_only: bool,
    /// Per-tool declaration budget in bytes (`0` disables). A tool whose
    /// declaration exceeds it is rebuilt with `max_schema_depth` lowered one
    /// level at a time (down to 1) until it fits. Only the handful of giant
    /// write methods (CM360 creatives/placements, DV360 targeting options)
    /// are affected; typical tools are well under 2 KB.
    pub max_tool_bytes: usize,
}

impl Default for SpecOptions {
    fn default() -> Self {
        Self {
            max_schema_depth: DEFAULT_MAX_SCHEMA_DEPTH,
            include_enum_descriptions: false,
            nested_descriptions: NestedDescriptions::default(),
            strip_output_only: true,
            max_tool_bytes: DEFAULT_MAX_TOOL_BYTES,
        }
    }
}

impl SpecOptions {
    /// Reads `GOOGLE_API_SCHEMA_MAX_DEPTH`, `GOOGLE_API_SCHEMA_ENUM_DESCRIPTIONS`,
    /// `GOOGLE_API_SCHEMA_NESTED_DESCRIPTIONS` (`full` / `first_sentence` /
    /// `none`), `GOOGLE_API_SCHEMA_KEEP_OUTPUT_ONLY` and
    /// `GOOGLE_API_SCHEMA_MAX_TOOL_BYTES` from the environment.
    pub fn from_env() -> Self {
        let defaults = Self::default();
        let flag = |name: &str, default: bool| {
            std::env::var(name)
                .ok()
                .map(|v| {
                    matches!(
                        v.trim().to_lowercase().as_str(),
                        "1" | "true" | "yes" | "on"
                    )
                })
                .unwrap_or(default)
        };
        Self {
            max_schema_depth: std::env::var("GOOGLE_API_SCHEMA_MAX_DEPTH")
                .ok()
                .and_then(|v| v.parse::<usize>().ok())
                .map(|d| d.max(1))
                .unwrap_or(defaults.max_schema_depth),
            include_enum_descriptions: flag(
                "GOOGLE_API_SCHEMA_ENUM_DESCRIPTIONS",
                defaults.include_enum_descriptions,
            ),
            nested_descriptions: std::env::var("GOOGLE_API_SCHEMA_NESTED_DESCRIPTIONS")
                .ok()
                .and_then(|v| NestedDescriptions::parse(&v))
                .unwrap_or(defaults.nested_descriptions),
            strip_output_only: !flag("GOOGLE_API_SCHEMA_KEEP_OUTPUT_ONLY", false),
            max_tool_bytes: std::env::var("GOOGLE_API_SCHEMA_MAX_TOOL_BYTES")
                .ok()
                .and_then(|v| v.parse::<usize>().ok())
                .unwrap_or(defaults.max_tool_bytes),
        }
    }
}

/// Whether a property description marks the field as output-only, i.e. set
/// by the server and ignored on writes.
///
/// Recognises the AIP-style `Output only.` / `@OutputOnly` prefix (DV360,
/// Merchant API, Google Ads) and the Campaign Manager 360 idiom
/// `This is a read-only field.` / `This is a read-only, auto-generated
/// field.` Conditional statements (`Output only when ...`, `read-only field
/// if ...`, `read-only after insertion`) do not count: the model may still
/// have to set those.
pub fn is_output_only(description: &str) -> bool {
    let lower = description.trim().to_ascii_lowercase();
    if lower.starts_with("@outputonly") {
        return true;
    }
    let prefixed = |marker: &str| {
        lower
            .strip_prefix(marker)
            .is_some_and(|rest| rest.is_empty() || rest.starts_with(['.', ':', ';', ',']))
    };
    if prefixed("output only") || prefixed("read-only") || prefixed("read only") {
        return true;
    }
    lower.split('.').map(str::trim).any(|sentence| {
        (sentence.starts_with("this is a read-only")
            || sentence.starts_with("it is a read-only")
            || sentence.starts_with("it is a auto-generated, read-only")
            || sentence.starts_with("it is an auto-generated, read-only"))
            && ![
                "if", "for", "relevant", "unless", "when", "after", "only to",
            ]
            .iter()
            .any(|c| sentence.contains(&format!(" {c} ")))
    })
}

/// The first sentence of `text` (through its terminating period), or the
/// whole text when there is no sentence boundary. Common abbreviations
/// (`e.g.`, `i.e.`, `etc.`, `vs.`) and decimals do not end a sentence.
pub fn first_sentence(text: &str) -> &str {
    let text = text.trim();
    let bytes = text.as_bytes();
    let mut search_from = 0;
    while let Some(rel) = text[search_from..].find(['.', '!', '?']) {
        let idx = search_from + rel;
        let next = bytes.get(idx + 1).copied();
        let ends_here = next.is_none_or(|b| b.is_ascii_whitespace());
        if ends_here {
            let before = &text[..idx];
            let abbreviation = ["e.g", "i.e", "etc", "vs", "approx", "incl", "resp"]
                .iter()
                .any(|a| before.to_ascii_lowercase().ends_with(a));
            if !abbreviation {
                return &text[..=idx];
            }
        }
        search_from = idx + 1;
    }
    text
}

/// A fully prepared HTTP request.
#[derive(Debug, Clone, PartialEq)]
pub struct PreparedRequest {
    /// HTTP verb (uppercase).
    pub method: String,
    /// Absolute URL without query string.
    pub url: String,
    /// Query parameters (repeated keys allowed).
    pub query: Vec<(String, String)>,
    /// Header parameters declared by the method.
    pub headers: Vec<(String, String)>,
    /// JSON body, if any.
    pub body: Option<Value>,
}

/// An LLM-callable description of one REST method.
#[derive(Debug, Clone, PartialEq)]
pub struct MethodSpec {
    /// Tool name, e.g. `drive_files_list`.
    pub tool_name: String,
    /// Discovery method id, e.g. `drive.files.list`.
    pub method_id: String,
    /// Tool description.
    pub description: String,
    /// HTTP verb (uppercase).
    pub http_method: String,
    /// Base URL (`rootUrl + servicePath`, no trailing slash).
    pub base_url: String,
    /// Path template with a leading slash, e.g. `/files/{fileId}`.
    pub path_template: String,
    /// Parameters in declaration order.
    pub parameters: Vec<ApiParameter>,
    /// How the body is assembled.
    pub body_kind: BodyKind,
    /// OAuth scopes accepted by the method.
    pub scopes: Vec<String>,
    /// Whether the method supports media upload.
    pub supports_media_upload: bool,
}

/// Builds method specs for every method of a Discovery document.
///
/// Tools whose declaration exceeds [`SpecOptions::max_tool_bytes`] are
/// rebuilt with a shallower request schema (one depth level at a time, down
/// to 1) until they fit.
pub fn build_method_specs(doc: &DiscoveryDocument, options: &SpecOptions) -> Vec<MethodSpec> {
    let resolver = SchemaResolver {
        schemas: &doc.schemas,
        options: *options,
    };
    doc.all_methods()
        .into_iter()
        .filter(|m| !m.id.is_empty())
        .map(|m| {
            let mut spec = build_method_spec(doc, m, &resolver);
            if options.max_tool_bytes == 0 {
                return spec;
            }
            let original = spec.declaration_bytes();
            let mut depth = options.max_schema_depth;
            while depth > 1 && spec.declaration_bytes() > options.max_tool_bytes {
                depth -= 1;
                let shallower = SchemaResolver {
                    schemas: &doc.schemas,
                    options: SpecOptions {
                        max_schema_depth: depth,
                        ..*options
                    },
                };
                spec = build_method_spec(doc, m, &shallower);
            }
            if depth != options.max_schema_depth {
                tracing::debug!(
                    "{}: declaration of {original} bytes exceeds the {} byte budget; \
                     resolved the request schema at depth {depth} ({} bytes)",
                    m.id,
                    options.max_tool_bytes,
                    spec.declaration_bytes()
                );
            }
            spec
        })
        .collect()
}

fn build_method_spec(
    doc: &DiscoveryDocument,
    method: &DiscoveryMethod,
    resolver: &SchemaResolver<'_>,
) -> MethodSpec {
    let raw_path = method
        .flat_path
        .clone()
        .filter(|p| !p.is_empty())
        .unwrap_or_else(|| method.path.clone());
    let path_template = if raw_path.starts_with('/') {
        raw_path
    } else {
        format!("/{raw_path}")
    };
    let template_params = extract_path_parameters(&path_template);

    let mut parameters: Vec<ApiParameter> = Vec::new();

    // 1. Path parameters, in template order. Path values are always sent as
    //    strings, so any numeric `format` declared by Discovery is dropped
    //    along with the type (`{"type":"string","format":"int32"}` is not a
    //    valid Gemini declaration).
    for (name, reserved) in &template_params {
        let declared = method.parameters.get(name.as_str());
        let mut schema = declared.map_or_else(
            || json!({"type": "string"}),
            |p| parameter_schema(p, &resolver.options),
        );
        if let Some(obj) = schema.as_object_mut() {
            obj.remove("items");
            obj.insert("type".to_string(), Value::String("string".to_string()));
            if obj
                .get("format")
                .and_then(Value::as_str)
                .is_some_and(|f| !keep_format("string", f))
            {
                obj.remove("format");
            }
        }
        parameters.push(ApiParameter {
            original_name: name.clone(),
            name: parameter_name(name, "path"),
            location: ParamLocation::Path,
            schema,
            required: true,
            reserved_expansion: *reserved,
        });
    }

    // 2. Query (and other non-path) parameters, in declaration order. Declared
    //    path parameters that are not in the template (the `{+name}` form of a
    //    flattened path) are skipped because the flat path already covers them.
    for (name, param) in &method.parameters {
        if template_params.iter().any(|(n, _)| n == name) {
            continue;
        }
        let location = param.location.as_deref().unwrap_or("query");
        let location = match location {
            "path" => continue,
            "header" => ParamLocation::Header,
            _ => ParamLocation::Query,
        };
        parameters.push(ApiParameter {
            original_name: name.clone(),
            name: parameter_name(name, location.as_str()),
            location,
            schema: parameter_schema(param, &resolver.options),
            required: param.required,
            reserved_expansion: false,
        });
    }

    // 2b. The global `fields` parameter (partial responses). It is the one
    //     global parameter worth exposing: it lets the model shrink large
    //     list/export responses, which the toolset otherwise refuses to hand
    //     over. Methods that declare their own `fields` keep theirs.
    if let Some(fields) = doc.parameters.get("fields")
        && !method.parameters.contains_key("fields")
        && !template_params.iter().any(|(n, _)| n == "fields")
    {
        let mut param = fields.clone();
        if param.description.trim().is_empty() {
            param.description =
                "Selector specifying which fields to include in a partial response.".to_string();
        }
        parameters.push(ApiParameter {
            original_name: "fields".to_string(),
            name: parameter_name("fields", "query"),
            location: ParamLocation::Query,
            schema: parameter_schema(&param, &resolver.options),
            required: false,
            reserved_expansion: false,
        });
    }

    // 3. Request body.
    let mut body_kind = BodyKind::None;
    if let Some(reference) = method.request.as_ref().and_then(|r| r.reference.as_deref()) {
        let resolved = resolver.resolve_ref(reference);
        match resolved.get("type").and_then(Value::as_str) {
            Some("object") => {
                body_kind = BodyKind::Object;
                let required: Vec<&str> = resolved
                    .get("required")
                    .and_then(Value::as_array)
                    .map(|r| r.iter().filter_map(Value::as_str).collect())
                    .unwrap_or_default();
                let properties = resolved.get("properties").and_then(Value::as_object);
                match properties {
                    Some(props) if !props.is_empty() => {
                        for (prop_name, prop_schema) in props {
                            parameters.push(ApiParameter {
                                original_name: prop_name.clone(),
                                name: parameter_name(prop_name, "body"),
                                location: ParamLocation::Body,
                                schema: prop_schema.clone(),
                                required: required.contains(&prop_name.as_str()),
                                reserved_expansion: false,
                            });
                        }
                    }
                    _ => {
                        body_kind = BodyKind::Whole;
                        parameters.push(ApiParameter {
                            original_name: String::new(),
                            name: "body".to_string(),
                            location: ParamLocation::Body,
                            schema: resolved.clone(),
                            required: false,
                            reserved_expansion: false,
                        });
                    }
                }
            }
            Some("array") => {
                body_kind = BodyKind::Array;
                parameters.push(ApiParameter {
                    original_name: "array".to_string(),
                    name: "array".to_string(),
                    location: ParamLocation::Body,
                    schema: resolved.clone(),
                    required: false,
                    reserved_expansion: false,
                });
            }
            _ => {
                body_kind = BodyKind::Whole;
                parameters.push(ApiParameter {
                    original_name: String::new(),
                    name: "body".to_string(),
                    location: ParamLocation::Body,
                    schema: resolved.clone(),
                    required: false,
                    reserved_expansion: false,
                });
            }
        }
    }

    // 4. De-duplicate LLM-facing names.
    let mut names: Vec<String> = parameters.iter().map(|p| p.name.clone()).collect();
    dedupe_names(&mut names);
    for (param, name) in parameters.iter_mut().zip(names) {
        param.name = name;
    }

    MethodSpec {
        tool_name: tool_name_for_method_id(&method.id),
        method_id: method.id.clone(),
        description: method.description.clone(),
        http_method: method.http_method.to_uppercase(),
        base_url: doc.base_url(),
        path_template,
        parameters,
        body_kind,
        scopes: method.scopes.clone(),
        supports_media_upload: method.supports_media_upload,
    }
}

/// Extracts `{name}` / `{+name}` segments from a path template.
fn extract_path_parameters(path: &str) -> Vec<(String, bool)> {
    let mut params = Vec::new();
    let mut rest = path;
    while let Some(start) = rest.find('{') {
        let Some(len) = rest[start..].find('}') else {
            break;
        };
        let inner = &rest[start + 1..start + len];
        let (name, reserved) = match inner.strip_prefix('+') {
            Some(n) => (n, true),
            None => (inner, false),
        };
        if !name.is_empty() && !params.iter().any(|(n, _): &(String, bool)| n == name) {
            params.push((name.to_string(), reserved));
        }
        rest = &rest[start + len + 1..];
    }
    params
}

/// Formats supported by Gemini function declarations for each JSON type.
const ALLOWED_FORMATS: &[(&str, &str)] = &[
    ("integer", "int32"),
    ("integer", "int64"),
    ("number", "float"),
    ("number", "double"),
    ("string", "date-time"),
    ("string", "enum"),
];

fn keep_format(json_type: &str, format: &str) -> bool {
    ALLOWED_FORMATS.contains(&(json_type, format))
}

/// Leaf method names (lower-cased prefixes) of `POST` methods that read
/// rather than write; see [`MethodSpec::is_mutating`].
const READ_LIKE_POST_PREFIXES: &[&str] = &[
    "get", "list", "search", "query", "run", "generate", "lookup", "download", "audit", "batchget",
];

/// Builds the JSON schema for a query/path parameter.
fn parameter_schema(param: &DiscoveryParameter, options: &SpecOptions) -> Value {
    let json_type = param.type_.as_deref().unwrap_or("string");
    let json_type = if json_type == "any" {
        "string"
    } else {
        json_type
    };
    let mut schema = Map::new();
    schema.insert("type".to_string(), Value::String(json_type.to_string()));
    let enum_descriptions = if options.include_enum_descriptions {
        param.enum_descriptions.as_deref()
    } else {
        None
    };
    let description = describe(
        &param.description,
        param.enum_values.as_deref(),
        enum_descriptions,
    );
    if !description.is_empty() {
        schema.insert("description".to_string(), Value::String(description));
    }
    if let Some(values) = &param.enum_values {
        schema.insert("enum".to_string(), json!(values));
    }
    if let Some(format) = &param.format
        && keep_format(json_type, format)
    {
        schema.insert("format".to_string(), Value::String(format.clone()));
    }
    let schema = Value::Object(schema);
    if param.repeated {
        let mut array = Map::new();
        array.insert("type".to_string(), Value::String("array".to_string()));
        if let Some(desc) = schema.get("description") {
            array.insert("description".to_string(), desc.clone());
        }
        let mut items = schema.clone();
        if let Some(obj) = items.as_object_mut() {
            obj.remove("description");
        }
        array.insert("items".to_string(), items);
        return Value::Object(array);
    }
    schema
}

/// Combines a description with enum value descriptions.
fn describe(
    description: &str,
    enum_values: Option<&[String]>,
    enum_descriptions: Option<&[String]>,
) -> String {
    let mut out = description.trim().to_string();
    if let (Some(values), Some(descs)) = (enum_values, enum_descriptions)
        && values.len() == descs.len()
        && descs.iter().any(|d| !d.trim().is_empty())
    {
        if !out.is_empty() {
            out.push_str(" Values: ");
        } else {
            out.push_str("Values: ");
        }
        let joined: Vec<String> = values
            .iter()
            .zip(descs)
            .map(|(v, d)| {
                if d.trim().is_empty() {
                    v.clone()
                } else {
                    format!("{v} ({})", d.trim())
                }
            })
            .collect();
        out.push_str(&joined.join("; "));
    }
    out
}

/// Resolves Discovery schemas into self-contained JSON schemas.
struct SchemaResolver<'a> {
    schemas: &'a IndexMap<String, Value>,
    options: SpecOptions,
}

impl SchemaResolver<'_> {
    fn resolve_ref(&self, reference: &str) -> Value {
        let mut stack = Vec::new();
        self.convert(&json!({ "$ref": reference }), 0, &mut stack)
    }

    fn summary(name: &str, description: &str) -> Value {
        let mut text = format!("{name} object (see the API reference for its fields).");
        if !description.trim().is_empty() {
            text = format!("{} {text}", description.trim());
        }
        json!({ "type": "object", "description": text })
    }

    /// Applies [`SpecOptions::nested_descriptions`] to a description found
    /// at `depth`. Depth 1 is the request schema and depth 2 its top-level
    /// properties, which become tool parameters and always keep their text.
    fn nested_description<'d>(&self, description: &'d str, depth: usize) -> &'d str {
        if depth <= 2 {
            return description;
        }
        match self.options.nested_descriptions {
            NestedDescriptions::Full => description,
            NestedDescriptions::FirstSentence => first_sentence(description),
            NestedDescriptions::None => "",
        }
    }

    fn convert(&self, schema: &Value, depth: usize, stack: &mut Vec<String>) -> Value {
        let Some(obj) = schema.as_object() else {
            return json!({ "type": "string" });
        };
        let description = obj.get("description").and_then(Value::as_str).unwrap_or("");

        if let Some(reference) = obj.get("$ref").and_then(Value::as_str) {
            let name = reference
                .trim_start_matches('#')
                .trim_start_matches('/')
                .to_string();
            let name = name.rsplit('/').next().unwrap_or(&name).to_string();
            // The description belongs to the referencing property, which
            // sits at `depth` (the referenced object is one level deeper).
            let description = self.nested_description(description, depth);
            if stack.contains(&name) || depth >= self.options.max_schema_depth {
                return Self::summary(&name, description);
            }
            let Some(target) = self.schemas.get(&name) else {
                return Self::summary(&name, description);
            };
            stack.push(name);
            let mut resolved = self.convert(target, depth + 1, stack);
            stack.pop();
            if !description.is_empty()
                && let Some(r) = resolved.as_object_mut()
                && !r.contains_key("description")
            {
                r.insert(
                    "description".to_string(),
                    Value::String(description.to_string()),
                );
            }
            return resolved;
        }

        let json_type = obj.get("type").and_then(Value::as_str).unwrap_or("object");
        // The request schema itself sits at depth 1 and its top-level
        // properties at depth 2; anything deeper is "nested" and its
        // description is subject to `nested_descriptions`.
        let description = self.nested_description(description, depth);
        let mut out = Map::new();
        match json_type {
            "object" => {
                out.insert("type".to_string(), Value::String("object".to_string()));
                if !description.is_empty() {
                    out.insert(
                        "description".to_string(),
                        Value::String(description.to_string()),
                    );
                }
                if let Some(props) = obj.get("properties").and_then(Value::as_object) {
                    // Output-only properties are dropped from request
                    // bodies at every depth: the server ignores them.
                    let props: Vec<(&String, &Value)> = props
                        .iter()
                        .filter(|(_, p)| {
                            !self.options.strip_output_only
                                || !p
                                    .get("description")
                                    .and_then(Value::as_str)
                                    .is_some_and(is_output_only)
                        })
                        .collect();
                    if depth >= self.options.max_schema_depth {
                        let mut text = description.trim().to_string();
                        if !text.is_empty() {
                            text.push(' ');
                        }
                        let _ = write!(
                            text,
                            "Object with fields: {}.",
                            props
                                .iter()
                                .map(|(k, _)| k.as_str())
                                .collect::<Vec<_>>()
                                .join(", ")
                        );
                        out.insert("description".to_string(), Value::String(text));
                    } else {
                        let mut converted = Map::new();
                        let mut required = Vec::new();
                        for (prop_name, prop_schema) in props {
                            converted.insert(
                                prop_name.clone(),
                                self.convert(prop_schema, depth + 1, stack),
                            );
                            if prop_schema
                                .get("required")
                                .and_then(Value::as_bool)
                                .unwrap_or(false)
                            {
                                required.push(Value::String(prop_name.clone()));
                            }
                        }
                        out.insert("properties".to_string(), Value::Object(converted));
                        if !required.is_empty() {
                            out.insert("required".to_string(), Value::Array(required));
                        }
                    }
                }
            }
            "array" => {
                out.insert("type".to_string(), Value::String("array".to_string()));
                if !description.is_empty() {
                    out.insert(
                        "description".to_string(),
                        Value::String(description.to_string()),
                    );
                }
                let items = obj.get("items").map_or_else(
                    || json!({ "type": "string" }),
                    |i| self.convert(i, depth + 1, stack),
                );
                out.insert("items".to_string(), items);
            }
            "any" => {
                out.insert("type".to_string(), Value::String("object".to_string()));
                let mut text = description.trim().to_string();
                if !text.is_empty() {
                    text.push(' ');
                }
                text.push_str("(Any JSON value.)");
                out.insert("description".to_string(), Value::String(text));
            }
            primitive => {
                out.insert("type".to_string(), Value::String(primitive.to_string()));
                let enum_values: Option<Vec<String>> =
                    obj.get("enum").and_then(Value::as_array).map(|v| {
                        v.iter()
                            .filter_map(Value::as_str)
                            .map(str::to_string)
                            .collect()
                    });
                let enum_descs: Option<Vec<String>> = if self.options.include_enum_descriptions {
                    obj.get("enumDescriptions")
                        .and_then(Value::as_array)
                        .map(|v| {
                            v.iter()
                                .filter_map(Value::as_str)
                                .map(str::to_string)
                                .collect()
                        })
                } else {
                    None
                };
                let text = describe(description, enum_values.as_deref(), enum_descs.as_deref());
                if !text.is_empty() {
                    out.insert("description".to_string(), Value::String(text));
                }
                if let Some(values) = enum_values {
                    out.insert("enum".to_string(), json!(values));
                }
                if let Some(format) = obj.get("format").and_then(Value::as_str)
                    && keep_format(primitive, format)
                {
                    out.insert("format".to_string(), Value::String(format.to_string()));
                }
            }
        }
        Value::Object(out)
    }
}

impl MethodSpec {
    /// Approximate size of this tool's declaration as sent to the model:
    /// the serialised parameter schema plus the description.
    pub fn declaration_bytes(&self) -> usize {
        serde_json::to_string(&self.parameters_schema())
            .map(|s| s.len())
            .unwrap_or(0)
            + self.description.len()
    }

    /// Whether the method changes state. `GET` never does; `DELETE`, `PUT`
    /// and `PATCH` always do; `POST` is judged by the method's leaf name so
    /// that queries, searches, report runs and tag/forecast generation
    /// (`dimensionValues.query`, `targetingOptions.search`, `reports.run`,
    /// `placements.generatetags`, ...) count as reads while `insert`,
    /// `create`, `bulkEdit`, `publish`, `upload`, ... count as writes.
    pub fn is_mutating(&self) -> bool {
        match self.http_method.as_str() {
            "GET" | "HEAD" | "OPTIONS" => false,
            "POST" => {
                let leaf = self
                    .method_id
                    .rsplit('.')
                    .next()
                    .unwrap_or("")
                    .to_ascii_lowercase();
                !READ_LIKE_POST_PREFIXES
                    .iter()
                    .any(|prefix| leaf.starts_with(prefix))
            }
            _ => true,
        }
    }

    /// JSON schema describing the tool's arguments.
    pub fn parameters_schema(&self) -> Value {
        let mut properties = Map::new();
        let mut required = Vec::new();
        for param in &self.parameters {
            properties.insert(param.name.clone(), param.schema.clone());
            if param.required {
                required.push(Value::String(param.name.clone()));
            }
        }
        let mut schema = Map::new();
        schema.insert("type".to_string(), Value::String("object".to_string()));
        schema.insert("properties".to_string(), Value::Object(properties));
        if !required.is_empty() {
            schema.insert("required".to_string(), Value::Array(required));
        }
        Value::Object(schema)
    }

    /// Looks up a parameter by its LLM-facing name.
    pub fn parameter(&self, name: &str) -> Option<&ApiParameter> {
        self.parameters.iter().find(|p| p.name == name)
    }

    /// Prepares the HTTP request for the given tool arguments.
    ///
    /// # Errors
    ///
    /// Returns an error when `args` is not a JSON object or a required path
    /// parameter is missing.
    pub fn prepare_request(&self, args: &Value) -> anyhow::Result<PreparedRequest> {
        let empty = Map::new();
        let args = match args {
            Value::Object(map) => map,
            Value::Null => &empty,
            other => bail!("tool arguments must be a JSON object, got {other}"),
        };

        // Path.
        let mut path = self.path_template.clone();
        for param in self
            .parameters
            .iter()
            .filter(|p| p.location == ParamLocation::Path)
        {
            let value = args
                .get(&param.name)
                .filter(|v| !v.is_null())
                .map(scalar_to_string);
            let Some(value) = value else {
                bail!(
                    "Missing required path parameter '{}' for {}",
                    param.name,
                    self.tool_name
                );
            };
            let encoded = percent_encode(&value, param.reserved_expansion);
            let plain = format!("{{{}}}", param.original_name);
            let reserved = format!("{{+{}}}", param.original_name);
            path = path.replace(&plain, &encoded).replace(&reserved, &encoded);
        }
        let url = format!("{}{}", self.base_url, path);

        // Query.
        let mut query = Vec::new();
        for param in self
            .parameters
            .iter()
            .filter(|p| p.location == ParamLocation::Query)
        {
            let Some(value) = args.get(&param.name) else {
                continue;
            };
            match value {
                Value::Null => {}
                Value::Array(items) => {
                    for item in items {
                        if !item.is_null() {
                            let s = scalar_to_string(item);
                            if !s.is_empty() {
                                query.push((param.original_name.clone(), s));
                            }
                        }
                    }
                }
                other => {
                    let s = scalar_to_string(other);
                    if !s.is_empty() {
                        query.push((param.original_name.clone(), s));
                    }
                }
            }
        }

        // Headers (rare; e.g. `Content-Language`). Header values cannot carry
        // line breaks, so those are rejected rather than sent.
        let mut headers = Vec::new();
        for param in self
            .parameters
            .iter()
            .filter(|p| p.location == ParamLocation::Header)
        {
            let Some(value) = args.get(&param.name).filter(|v| !v.is_null()) else {
                continue;
            };
            let s = scalar_to_string(value);
            if s.is_empty() {
                continue;
            }
            if s.contains(['\r', '\n']) {
                bail!(
                    "Header parameter '{}' for {} must not contain line breaks",
                    param.name,
                    self.tool_name
                );
            }
            headers.push((param.original_name.clone(), s));
        }

        // Body.
        let body_param = self
            .parameters
            .iter()
            .find(|p| p.location == ParamLocation::Body);
        let body = match self.body_kind {
            BodyKind::None => None,
            BodyKind::Object => {
                let mut body = Map::new();
                for param in self
                    .parameters
                    .iter()
                    .filter(|p| p.location == ParamLocation::Body)
                {
                    if let Some(value) = args.get(&param.name)
                        && !value.is_null()
                    {
                        body.insert(param.original_name.clone(), value.clone());
                    }
                }
                Some(Value::Object(body))
            }
            // The single body parameter is looked up by its (possibly
            // de-duplicated) LLM-facing name rather than a literal `array`.
            BodyKind::Array => body_param
                .and_then(|p| args.get(&p.name))
                .cloned()
                .filter(|v| !v.is_null()),
            // Models sometimes pass an object/array body as a JSON string;
            // decode it unless the schema really asks for a string.
            BodyKind::Whole => body_param.and_then(|p| {
                let value = args.get(&p.name).cloned().filter(|v| !v.is_null())?;
                let wants_string = p.schema.get("type").and_then(Value::as_str) == Some("string");
                Some(match value {
                    Value::String(s) if !wants_string => {
                        serde_json::from_str(&s).unwrap_or(Value::String(s))
                    }
                    other => other,
                })
            }),
        };

        Ok(PreparedRequest {
            method: self.http_method.clone(),
            url,
            query,
            headers,
            body,
        })
    }
}

/// Renders a scalar JSON value the way a query/path string expects it.
fn scalar_to_string(value: &Value) -> String {
    match value {
        Value::String(s) => s.clone(),
        Value::Bool(b) => b.to_string(),
        Value::Number(n) => n.to_string(),
        Value::Null => String::new(),
        other => other.to_string(),
    }
}

/// Percent-encodes a path segment value (RFC 3986). With `reserved`, the
/// reserved characters are kept (matching `{+name}` expansion, notably `/`)
/// except `?` and `#`, which would otherwise terminate the path and smuggle
/// a query string or fragment into the request.
fn percent_encode(value: &str, reserved: bool) -> String {
    const RESERVED: &[u8] = b":/[]@!$&'()*+,;=";
    let mut out = String::with_capacity(value.len());
    for byte in value.bytes() {
        let keep = byte.is_ascii_alphanumeric()
            || matches!(byte, b'-' | b'.' | b'_' | b'~')
            || (reserved && RESERVED.contains(&byte));
        if keep {
            out.push(byte as char);
        } else {
            let _ = write!(out, "%{byte:02X}");
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn doc() -> DiscoveryDocument {
        DiscoveryDocument::parse(
            r#"{
          "name": "googleads", "version": "v26",
          "rootUrl": "https://googleads.googleapis.com/", "servicePath": "",
          "schemas": {
            "SearchRequest": {"id": "SearchRequest", "type": "object", "properties": {
              "query": {"type": "string", "description": "GAQL query.", "required": true},
              "pageToken": {"type": "string"},
              "pageSize": {"type": "integer", "format": "int32"},
              "settings": {"$ref": "Settings", "description": "Search settings."},
              "customerId": {"type": "string"}
            }},
            "Settings": {"id": "Settings", "type": "object", "properties": {
              "omitResults": {"type": "boolean"},
              "nested": {"$ref": "Settings"},
              "mode": {"type": "string", "enum": ["A", "B"], "enumDescriptions": ["Mode A", "Mode B"]},
              "tags": {"type": "array", "items": {"type": "string"}},
              "extra": {"type": "any"}
            }}
          },
          "resources": {"customers": {"resources": {"googleAds": {"methods": {
            "search": {
              "id": "googleads.customers.googleAds.search",
              "path": "v26/{+customerId}/googleAds:search",
              "flatPath": "v26/customers/{customersId}/googleAds:search",
              "httpMethod": "POST",
              "description": "Returns all rows that match the search query.",
              "parameters": {
                "customerId": {"type": "string", "location": "path", "required": true, "pattern": "^customers/[^/]+$"},
                "fields": {"type": "string", "location": "query", "repeated": true, "description": "Fields."},
                "from": {"type": "string", "location": "query"}
              },
              "request": {"$ref": "SearchRequest"},
              "scopes": ["https://www.googleapis.com/auth/adwords"]
            }
          }}}}}
        }"#,
        )
        .unwrap()
    }

    #[test]
    fn builds_spec_with_flattened_body_and_dedupe() {
        let options = SpecOptions {
            include_enum_descriptions: true,
            ..SpecOptions::default()
        };
        let specs = build_method_specs(&doc(), &options);
        assert_eq!(specs.len(), 1);
        let spec = &specs[0];
        assert_eq!(spec.tool_name, "googleads_customers_google_ads_search");
        assert_eq!(spec.http_method, "POST");
        assert_eq!(spec.base_url, "https://googleads.googleapis.com");
        assert_eq!(
            spec.path_template,
            "/v26/customers/{customersId}/googleAds:search"
        );
        assert_eq!(spec.body_kind, BodyKind::Object);
        let names: Vec<&str> = spec.parameters.iter().map(|p| p.name.as_str()).collect();
        // path param, query params (declared path param `customerId` dropped), then body props;
        // body `customerId` collides with nothing now, `from` keyword renamed.
        assert_eq!(
            names,
            vec![
                "customers_id",
                "fields",
                "param_from",
                "query",
                "page_token",
                "page_size",
                "settings",
                "customer_id"
            ]
        );
        assert!(spec.parameter("customers_id").unwrap().required);
        assert!(
            spec.parameter("query").unwrap().required,
            "schema-level required propagates"
        );
        assert!(!spec.parameter("page_token").unwrap().required);
        assert_eq!(spec.parameter("fields").unwrap().schema["type"], "array");
        assert_eq!(
            spec.parameter("page_size").unwrap().schema["format"],
            "int32"
        );

        let settings = &spec.parameter("settings").unwrap().schema;
        assert_eq!(settings["type"], "object");
        assert_eq!(settings["description"], "Search settings.");
        assert_eq!(settings["properties"]["mode"]["enum"], json!(["A", "B"]));
        assert_eq!(
            settings["properties"]["mode"]["description"],
            "Values: A (Mode A); B (Mode B)"
        );
        assert_eq!(settings["properties"]["tags"]["items"]["type"], "string");
        assert_eq!(settings["properties"]["extra"]["type"], "object");
        // circular reference is summarised rather than expanded forever
        let nested = &settings["properties"]["nested"];
        assert_eq!(nested["type"], "object");
        assert!(
            nested["description"]
                .as_str()
                .unwrap()
                .contains("Settings object")
        );
        assert!(nested.get("properties").is_none());
    }

    #[test]
    fn parameters_schema_lists_required_names() {
        let specs = build_method_specs(&doc(), &SpecOptions::default());
        let schema = specs[0].parameters_schema();
        assert_eq!(schema["type"], "object");
        assert_eq!(schema["required"], json!(["customers_id", "query"]));
        assert!(schema["properties"]["customers_id"].is_object());
    }

    #[test]
    fn prepares_request_with_path_query_and_body() {
        let specs = build_method_specs(&doc(), &SpecOptions::default());
        let req = specs[0]
            .prepare_request(&json!({
                "customers_id": "123 456",
                "fields": ["a", "b"],
                "param_from": "x",
                "query": "SELECT campaign.id FROM campaign",
                "page_size": 10,
                "page_token": null,
                "settings": {"omitResults": true}
            }))
            .unwrap();
        assert_eq!(req.method, "POST");
        assert_eq!(
            req.url,
            "https://googleads.googleapis.com/v26/customers/123%20456/googleAds:search"
        );
        assert_eq!(
            req.query,
            vec![
                ("fields".to_string(), "a".to_string()),
                ("fields".to_string(), "b".to_string()),
                ("from".to_string(), "x".to_string())
            ]
        );
        assert_eq!(
            req.body,
            Some(
                json!({"query": "SELECT campaign.id FROM campaign", "pageSize": 10, "settings": {"omitResults": true}})
            )
        );
    }

    #[test]
    fn missing_path_parameter_is_an_error() {
        let specs = build_method_specs(&doc(), &SpecOptions::default());
        let err = specs[0]
            .prepare_request(&json!({"query": "q"}))
            .unwrap_err();
        assert!(err.to_string().contains("customers_id"));
    }

    #[test]
    fn reserved_expansion_keeps_slashes() {
        let doc = DiscoveryDocument::parse(
            r#"{"name":"x","version":"v1","rootUrl":"https://x.googleapis.com/","servicePath":"",
            "resources":{"things":{"methods":{"get":{"id":"x.things.get","path":"v1/{+name}","httpMethod":"GET",
            "parameters":{"name":{"type":"string","location":"path","required":true}}}}}}}"#,
        )
        .unwrap();
        let specs = build_method_specs(&doc, &SpecOptions::default());
        assert_eq!(specs[0].parameters.len(), 1);
        assert_eq!(specs[0].parameters[0].name, "name");
        assert!(specs[0].parameters[0].reserved_expansion);
        let req = specs[0]
            .prepare_request(&json!({"name": "things/a b/parts/c"}))
            .unwrap();
        assert_eq!(req.url, "https://x.googleapis.com/v1/things/a%20b/parts/c");
        assert_eq!(req.body, None);
    }

    #[test]
    fn enum_descriptions_are_opt_in() {
        let specs = build_method_specs(&doc(), &SpecOptions::default());
        let mode = &specs[0].parameter("settings").unwrap().schema["properties"]["mode"];
        assert_eq!(mode["enum"], json!(["A", "B"]));
        assert!(
            mode.get("description").is_none(),
            "no enum descriptions by default"
        );
    }

    #[test]
    fn nested_descriptions_can_be_dropped() {
        let options = SpecOptions {
            nested_descriptions: NestedDescriptions::None,
            ..SpecOptions::default()
        };
        let specs = build_method_specs(&doc(), &options);
        let query = &specs[0].parameter("query").unwrap().schema;
        assert_eq!(
            query["description"], "GAQL query.",
            "top-level params keep descriptions"
        );
        let settings = &specs[0].parameter("settings").unwrap().schema;
        assert!(settings["properties"]["mode"].get("description").is_none());
    }

    /// A document with verbose nested descriptions, output-only fields and
    /// a write method, for the pruning tests.
    fn verbose_doc() -> DiscoveryDocument {
        DiscoveryDocument::parse(
            r#"{
          "name": "dfareporting", "version": "v5",
          "rootUrl": "https://dfareporting.googleapis.com/", "servicePath": "dfareporting/v5/",
          "schemas": {
            "Campaign": {"id": "Campaign", "type": "object", "properties": {
              "id": {"type": "string", "description": "ID of this campaign. This is a read-only auto-generated field."},
              "accountId": {"type": "string", "description": "Account ID of this campaign. This is a read-only field that can be left blank."},
              "name": {"type": "string", "description": "Name of this campaign. This is a required field and must be less than 256 characters long and unique among campaigns of the same advertiser."},
              "kind": {"type": "string", "description": "Output only. Identifies what kind of resource this is."},
              "conditional": {"type": "string", "description": "Output only when the flag is set."},
              "afterInsert": {"type": "string", "description": "Advertiser ID. This is a required field that is read-only after insertion."},
              "settings": {"$ref": "Settings", "description": "Settings of this campaign. See the guide for details, e.g. about defaults."}
            }},
            "Settings": {"id": "Settings", "type": "object", "properties": {
              "lastModified": {"type": "string", "description": "Output only. Last modification time."},
              "mode": {"type": "string", "description": "The mode. Possible values are documented at length here. More text follows."},
              "deeper": {"$ref": "Deep", "description": "Deeper settings, i.e. the fine print. Rarely needed."}
            }},
            "Deep": {"id": "Deep", "type": "object", "properties": {
              "a": {"type": "string", "description": "A field. With a second sentence."},
              "b": {"type": "string"}
            }}
          },
          "resources": {"campaigns": {"methods": {
            "insert": {"id": "dfareporting.campaigns.insert", "path": "userprofiles/{profileId}/campaigns", "httpMethod": "POST",
              "parameters": {"profileId": {"type": "string", "location": "path", "required": true}},
              "request": {"$ref": "Campaign"}},
            "list": {"id": "dfareporting.campaigns.list", "path": "userprofiles/{profileId}/campaigns", "httpMethod": "GET",
              "parameters": {"profileId": {"type": "string", "location": "path", "required": true}}},
            "run": {"id": "dfareporting.reports.run", "path": "userprofiles/{profileId}/reports/{reportId}/run", "httpMethod": "POST",
              "parameters": {"profileId": {"type": "string", "location": "path", "required": true}, "reportId": {"type": "string", "location": "path", "required": true}}},
            "patch": {"id": "dfareporting.campaigns.patch", "path": "userprofiles/{profileId}/campaigns", "httpMethod": "PATCH",
              "parameters": {"profileId": {"type": "string", "location": "path", "required": true}},
              "request": {"$ref": "Campaign"}}
          }}}
        }"#,
        )
        .unwrap()
    }

    #[test]
    fn output_only_detection_follows_the_api_idioms() {
        assert!(is_output_only("Output only. The unique ID."));
        assert!(is_output_only("Output only"));
        assert!(is_output_only("@OutputOnly The id."));
        assert!(is_output_only("Read-only. Generated by the server."));
        assert!(is_output_only(
            "Account ID of this ad. This is a read-only field that can be left blank."
        ));
        assert!(is_output_only(
            "ID of this ad. This is a read-only, auto-generated field."
        ));
        assert!(is_output_only("It is a read-only field."));
        // Conditional or lifecycle statements are not output-only.
        assert!(!is_output_only("Output only when commitment is set."));
        assert!(!is_output_only(
            "Advertiser ID. This is a required field that is read-only after insertion."
        ));
        assert!(!is_output_only(
            "This is a read-only field if the asset displayType is OVERLAY."
        ));
        assert!(!is_output_only(
            "This is a read-only field for VPAID_NON_LINEAR_VIDEO assets."
        ));
        assert!(!is_output_only(
            "Read-only convenience field representing the actual URL."
        ));
        assert!(!is_output_only(
            "Name of this campaign. This is a required field."
        ));
    }

    #[test]
    fn first_sentence_handles_abbreviations_and_missing_periods() {
        assert_eq!(first_sentence("The mode. More text."), "The mode.");
        assert_eq!(
            first_sentence("See the guide, e.g. about defaults. More."),
            "See the guide, e.g. about defaults."
        );
        assert_eq!(
            first_sentence("Ratio is 1.5 or more. Next."),
            "Ratio is 1.5 or more."
        );
        assert_eq!(first_sentence("No period here"), "No period here");
        assert_eq!(first_sentence("  Trimmed.  "), "Trimmed.");
        assert_eq!(first_sentence("Really? Yes."), "Really?");
    }

    #[test]
    fn output_only_properties_are_stripped_from_request_bodies() {
        let specs = build_method_specs(&verbose_doc(), &SpecOptions::default());
        let insert = specs
            .iter()
            .find(|s| s.tool_name.ends_with("_insert"))
            .unwrap();
        let names: Vec<&str> = insert.parameters.iter().map(|p| p.name.as_str()).collect();
        assert_eq!(
            names,
            vec![
                "profile_id",
                "name",
                "conditional",
                "after_insert",
                "settings"
            ],
            "id / accountId / kind are output-only"
        );
        let settings = &insert.parameter("settings").unwrap().schema;
        let nested: Vec<&String> = settings["properties"].as_object().unwrap().keys().collect();
        assert_eq!(
            nested,
            vec!["mode", "deeper"],
            "nested output-only dropped too"
        );

        // Opt out keeps everything.
        let specs = build_method_specs(
            &verbose_doc(),
            &SpecOptions {
                strip_output_only: false,
                ..SpecOptions::default()
            },
        );
        let insert = specs
            .iter()
            .find(|s| s.tool_name.ends_with("_insert"))
            .unwrap();
        assert!(insert.parameter("id").is_some());
        assert!(insert.parameter("kind").is_some());
    }

    #[test]
    fn nested_descriptions_keep_only_the_first_sentence_by_default() {
        // Depth 6 so that `settings.deeper` is fully resolved rather than
        // summarised (the request body is depth 1, `settings` resolves at
        // depth 3, `deeper` at depth 5).
        let deep = SpecOptions {
            max_schema_depth: 6,
            ..SpecOptions::default()
        };
        let specs = build_method_specs(&verbose_doc(), &deep);
        let insert = specs
            .iter()
            .find(|s| s.tool_name.ends_with("_insert"))
            .unwrap();
        // Depth 2 (tool parameters): full text.
        assert_eq!(
            insert.parameter("name").unwrap().schema["description"],
            "Name of this campaign. This is a required field and must be less than 256 characters long and unique among campaigns of the same advertiser."
        );
        let settings = &insert.parameter("settings").unwrap().schema;
        assert_eq!(
            settings["description"],
            "Settings of this campaign. See the guide for details, e.g. about defaults."
        );
        // Depth 3 and below: first sentence only.
        assert_eq!(settings["properties"]["mode"]["description"], "The mode.");
        assert_eq!(
            settings["properties"]["deeper"]["description"],
            "Deeper settings, i.e. the fine print."
        );
        assert_eq!(
            settings["properties"]["deeper"]["properties"]["a"]["description"],
            "A field."
        );
        // Summaries of depth-limited refs are trimmed too.
        let specs = build_method_specs(&verbose_doc(), &SpecOptions::default());
        let insert = specs
            .iter()
            .find(|s| s.tool_name.ends_with("_insert"))
            .unwrap();
        assert_eq!(
            insert.parameter("settings").unwrap().schema["properties"]["deeper"]["description"],
            "Deeper settings, i.e. the fine print. Deep object (see the API reference for its fields)."
        );

        let specs = build_method_specs(
            &verbose_doc(),
            &SpecOptions {
                nested_descriptions: NestedDescriptions::Full,
                ..deep
            },
        );
        let insert = specs
            .iter()
            .find(|s| s.tool_name.ends_with("_insert"))
            .unwrap();
        assert_eq!(
            insert.parameter("settings").unwrap().schema["properties"]["mode"]["description"],
            "The mode. Possible values are documented at length here. More text follows."
        );
    }

    #[test]
    fn oversized_declarations_fall_back_to_a_shallower_schema() {
        let deep = SpecOptions {
            max_schema_depth: 6,
            ..SpecOptions::default()
        };
        let full = build_method_specs(&verbose_doc(), &deep);
        let insert = full
            .iter()
            .find(|s| s.tool_name.ends_with("_insert"))
            .unwrap();
        let full_bytes = insert.declaration_bytes();
        assert!(
            insert.parameter("settings").unwrap().schema["properties"]["deeper"]["properties"]
                .is_object()
        );

        // A budget just under the full size drops one level: `deeper` is
        // summarised but `settings` keeps its properties.
        let budgeted = build_method_specs(
            &verbose_doc(),
            &SpecOptions {
                max_tool_bytes: full_bytes - 1,
                ..deep
            },
        );
        let insert = budgeted
            .iter()
            .find(|s| s.tool_name.ends_with("_insert"))
            .unwrap();
        assert!(insert.declaration_bytes() < full_bytes);
        let settings = &insert.parameter("settings").unwrap().schema;
        assert!(settings["properties"]["mode"].is_object());
        assert!(settings["properties"]["deeper"].get("properties").is_none());

        // An impossible budget bottoms out at depth 1 (single `body` param).
        let tiny = build_method_specs(
            &verbose_doc(),
            &SpecOptions {
                max_tool_bytes: 1,
                ..deep
            },
        );
        let insert = tiny
            .iter()
            .find(|s| s.tool_name.ends_with("_insert"))
            .unwrap();
        assert_eq!(insert.body_kind, BodyKind::Whole);
        assert!(insert.parameter("body").is_some());
        // Small tools are untouched by the budget.
        let list = tiny
            .iter()
            .find(|s| s.tool_name.ends_with("_list"))
            .unwrap();
        assert_eq!(
            list,
            full.iter()
                .find(|s| s.tool_name.ends_with("_list"))
                .unwrap()
        );

        // `0` disables the budget.
        let unlimited = build_method_specs(
            &verbose_doc(),
            &SpecOptions {
                max_tool_bytes: 0,
                ..deep
            },
        );
        assert_eq!(unlimited, full);
    }

    #[test]
    fn mutating_methods_are_classified_by_verb_and_leaf_name() {
        let specs = build_method_specs(&verbose_doc(), &SpecOptions::default());
        let by_name = |suffix: &str| {
            specs
                .iter()
                .find(|s| s.tool_name.ends_with(suffix))
                .unwrap()
        };
        assert!(by_name("_insert").is_mutating());
        assert!(by_name("_patch").is_mutating());
        assert!(!by_name("_list").is_mutating());
        assert!(
            !by_name("reports_run").is_mutating(),
            "POST reports.run is a read"
        );
        // The googleAds.search POST of the main fixture is a read too.
        assert!(!build_method_specs(&doc(), &SpecOptions::default())[0].is_mutating());
    }

    #[test]
    fn depth_limit_summarises_deep_objects() {
        // Depth 2 expands the request body's own properties but summarises
        // the nested `Settings` object.
        let specs = build_method_specs(
            &doc(),
            &SpecOptions {
                max_schema_depth: 2,
                ..SpecOptions::default()
            },
        );
        let settings = &specs[0].parameter("settings").unwrap().schema;
        assert_eq!(settings["type"], "object");
        assert!(settings.get("properties").is_none());

        // Depth 1 summarises the whole request body into a single `body` param.
        let specs = build_method_specs(
            &doc(),
            &SpecOptions {
                max_schema_depth: 1,
                ..SpecOptions::default()
            },
        );
        assert_eq!(specs[0].body_kind, BodyKind::Whole);
        assert!(specs[0].parameter("body").is_some());
    }

    #[test]
    fn percent_encoding() {
        assert_eq!(percent_encode("a/b c", false), "a%2Fb%20c");
        assert_eq!(percent_encode("a/b c", true), "a/b%20c");
        assert_eq!(percent_encode("customers/123", false), "customers%2F123");
        // `?` and `#` never survive reserved expansion: they would end the
        // path and inject a query string / fragment.
        assert_eq!(percent_encode("a?x=1#f", true), "a%3Fx=1%23f");
        assert_eq!(percent_encode("a?x=1#f", false), "a%3Fx%3D1%23f");
    }

    #[test]
    fn path_parameters_never_carry_numeric_formats() {
        // dfareporting.videoFormats.get declares `id` as integer/int32.
        let doc = DiscoveryDocument::parse(
            r#"{"name":"dfareporting","version":"v5","rootUrl":"https://dfareporting.googleapis.com/","servicePath":"dfareporting/v5/",
            "resources":{"videoFormats":{"methods":{"get":{"id":"dfareporting.videoFormats.get","path":"userprofiles/{profileId}/videoFormats/{id}","httpMethod":"GET",
            "parameters":{"profileId":{"type":"string","format":"int64","location":"path","required":true},
                          "id":{"type":"integer","format":"int32","location":"path","required":true},
                          "when":{"type":"string","format":"date-time","location":"path","required":true}}}}}}}"#,
        )
        .unwrap();
        let specs = build_method_specs(&doc, &SpecOptions::default());
        let id = &specs[0].parameter("id").unwrap().schema;
        assert_eq!(id, &json!({"type": "string"}));
        let profile = &specs[0].parameter("profile_id").unwrap().schema;
        assert_eq!(profile, &json!({"type": "string"}));
        // `when` is not in the template, so it is skipped as a declared
        // path parameter; string formats that are valid stay valid.
        assert!(specs[0].parameter("when").is_none());
    }

    #[test]
    fn header_parameters_are_sent_and_sanitised() {
        let doc = DiscoveryDocument::parse(
            r#"{"name":"x","version":"v1","rootUrl":"https://x.googleapis.com/","servicePath":"",
            "resources":{"things":{"methods":{"list":{"id":"x.things.list","path":"v1/things","httpMethod":"GET",
            "parameters":{"X-Goog-Quota-User":{"type":"string","location":"header"}}}}}}}"#,
        )
        .unwrap();
        let specs = build_method_specs(&doc, &SpecOptions::default());
        let param = &specs[0].parameters[0];
        assert_eq!(param.location, ParamLocation::Header);
        assert_eq!(param.name, "x_goog_quota_user");
        let req = specs[0]
            .prepare_request(&json!({"x_goog_quota_user": "team-a"}))
            .unwrap();
        assert_eq!(
            req.headers,
            vec![("X-Goog-Quota-User".to_string(), "team-a".to_string())]
        );
        assert!(req.query.is_empty());
        let err = specs[0]
            .prepare_request(&json!({"x_goog_quota_user": "a\r\nEvil: 1"}))
            .unwrap_err();
        assert!(err.to_string().contains("line breaks"));
    }

    #[test]
    fn global_fields_parameter_is_exposed_unless_the_method_declares_one() {
        let doc = DiscoveryDocument::parse(
            r#"{"name":"drive","version":"v3","rootUrl":"https://www.googleapis.com/","servicePath":"drive/v3/",
            "parameters":{"alt":{"type":"string","location":"query"},"fields":{"type":"string","location":"query","description":"Selector specifying which fields to include in a partial response."}},
            "resources":{"files":{"methods":{
              "list":{"id":"drive.files.list","path":"files","httpMethod":"GET","parameters":{"q":{"type":"string","location":"query"}}},
              "own":{"id":"drive.files.own","path":"files:own","httpMethod":"GET","parameters":{"fields":{"type":"string","location":"query","description":"Method-level fields."}}}
            }}}}"#,
        )
        .unwrap();
        let specs = build_method_specs(&doc, &SpecOptions::default());
        let names: Vec<&str> = specs[0]
            .parameters
            .iter()
            .map(|p| p.name.as_str())
            .collect();
        assert_eq!(names, vec!["q", "fields"], "alt is not exposed");
        let fields = specs[0].parameter("fields").unwrap();
        assert_eq!(fields.location, ParamLocation::Query);
        assert!(!fields.required);
        let req = specs[0]
            .prepare_request(&json!({"fields": "files(id,name)"}))
            .unwrap();
        assert_eq!(
            req.query,
            vec![("fields".to_string(), "files(id,name)".to_string())]
        );
        // The method's own declaration wins (no duplicate).
        let own: Vec<&ApiParameter> = specs[1]
            .parameters
            .iter()
            .filter(|p| p.original_name == "fields")
            .collect();
        assert_eq!(own.len(), 1);
        assert_eq!(own[0].schema["description"], "Method-level fields.");
    }

    #[test]
    fn array_body_is_read_through_its_deduped_parameter_name() {
        let doc = DiscoveryDocument::parse(
            r#"{"name":"x","version":"v1","rootUrl":"https://x.googleapis.com/","servicePath":"",
            "schemas":{"Items":{"id":"Items","type":"array","items":{"type":"string"}}},
            "resources":{"things":{"methods":{"set":{"id":"x.things.set","path":"v1/things","httpMethod":"POST",
            "parameters":{"array":{"type":"string","location":"query"}},"request":{"$ref":"Items"}}}}}}"#,
        )
        .unwrap();
        let specs = build_method_specs(&doc, &SpecOptions::default());
        assert_eq!(specs[0].body_kind, BodyKind::Array);
        let names: Vec<&str> = specs[0]
            .parameters
            .iter()
            .map(|p| p.name.as_str())
            .collect();
        assert_eq!(names, vec!["array", "array_0"]);
        let req = specs[0]
            .prepare_request(&json!({"array": "q", "array_0": ["a", "b"]}))
            .unwrap();
        assert_eq!(req.query, vec![("array".to_string(), "q".to_string())]);
        assert_eq!(req.body, Some(json!(["a", "b"])));
    }

    #[test]
    fn whole_body_strings_are_only_decoded_when_the_schema_is_not_a_string() {
        let untyped = DiscoveryDocument::parse(
            r#"{"name":"x","version":"v1","rootUrl":"https://x.googleapis.com/","servicePath":"",
            "schemas":{"Any":{"id":"Any","type":"any"}},
            "resources":{"things":{"methods":{"set":{"id":"x.things.set","path":"v1/things","httpMethod":"POST","request":{"$ref":"Any"}}}}}}"#,
        )
        .unwrap();
        let specs = build_method_specs(&untyped, &SpecOptions::default());
        assert_eq!(specs[0].body_kind, BodyKind::Whole);
        let req = specs[0]
            .prepare_request(&json!({"body": "{\"a\": 1}"}))
            .unwrap();
        assert_eq!(req.body, Some(json!({"a": 1})));

        let stringly = DiscoveryDocument::parse(
            r#"{"name":"x","version":"v1","rootUrl":"https://x.googleapis.com/","servicePath":"",
            "schemas":{"Text":{"id":"Text","type":"string"}},
            "resources":{"things":{"methods":{"set":{"id":"x.things.set","path":"v1/things","httpMethod":"POST","request":{"$ref":"Text"}}}}}}"#,
        )
        .unwrap();
        let specs = build_method_specs(&stringly, &SpecOptions::default());
        assert_eq!(specs[0].body_kind, BodyKind::Whole);
        let req = specs[0].prepare_request(&json!({"body": "123"})).unwrap();
        assert_eq!(req.body, Some(json!("123")), "string schemas stay strings");
    }
}
