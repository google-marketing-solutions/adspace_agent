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
    /// Keep descriptions on nested request-body properties (top-level
    /// parameters always keep theirs).
    pub include_nested_descriptions: bool,
}

impl Default for SpecOptions {
    fn default() -> Self {
        Self {
            max_schema_depth: DEFAULT_MAX_SCHEMA_DEPTH,
            include_enum_descriptions: false,
            include_nested_descriptions: true,
        }
    }
}

impl SpecOptions {
    /// Reads `GOOGLE_API_SCHEMA_MAX_DEPTH`, `GOOGLE_API_SCHEMA_ENUM_DESCRIPTIONS`
    /// and `GOOGLE_API_SCHEMA_NESTED_DESCRIPTIONS` from the environment.
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
            include_nested_descriptions: flag(
                "GOOGLE_API_SCHEMA_NESTED_DESCRIPTIONS",
                defaults.include_nested_descriptions,
            ),
        }
    }
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
pub fn build_method_specs(doc: &DiscoveryDocument, options: &SpecOptions) -> Vec<MethodSpec> {
    let resolver = SchemaResolver {
        schemas: &doc.schemas,
        options: *options,
    };
    doc.all_methods()
        .into_iter()
        .filter(|m| !m.id.is_empty())
        .map(|m| build_method_spec(doc, m, &resolver))
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

    // 1. Path parameters, in template order.
    for (name, reserved) in &template_params {
        let declared = method.parameters.get(name.as_str());
        let mut schema = declared.map_or_else(
            || json!({"type": "string"}),
            |p| parameter_schema(p, &resolver.options),
        );
        if let Some(obj) = schema.as_object_mut() {
            obj.remove("items");
            obj.insert("type".to_string(), Value::String("string".to_string()));
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
        // descriptions are optional.
        let description = if depth > 2 && !self.options.include_nested_descriptions {
            ""
        } else {
            description
        };
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
                    if depth >= self.options.max_schema_depth {
                        let mut text = description.trim().to_string();
                        if !text.is_empty() {
                            text.push(' ');
                        }
                        let _ = write!(
                            text,
                            "Object with fields: {}.",
                            props.keys().cloned().collect::<Vec<_>>().join(", ")
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

        // Body.
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
            BodyKind::Array => args.get("array").cloned().filter(|v| !v.is_null()),
            BodyKind::Whole => self
                .parameters
                .iter()
                .find(|p| p.location == ParamLocation::Body)
                .and_then(|p| args.get(&p.name))
                .cloned()
                .filter(|v| !v.is_null())
                .map(|v| match v {
                    Value::String(s) => serde_json::from_str(&s).unwrap_or(Value::String(s)),
                    other => other,
                }),
        };

        Ok(PreparedRequest {
            method: self.http_method.clone(),
            url,
            query,
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
/// reserved characters (notably `/`) are kept, matching `{+name}` expansion.
fn percent_encode(value: &str, reserved: bool) -> String {
    const RESERVED: &[u8] = b":/?#[]@!$&'()*+,;=";
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
          "name": "googleads", "version": "v25",
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
              "path": "v25/{+customerId}/googleAds:search",
              "flatPath": "v25/customers/{customersId}/googleAds:search",
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
            "/v25/customers/{customersId}/googleAds:search"
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
            "https://googleads.googleapis.com/v25/customers/123%20456/googleAds:search"
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
            include_nested_descriptions: false,
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
    }
}
