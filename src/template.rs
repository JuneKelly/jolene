use std::collections::BTreeMap;
use std::path::Path;
use std::sync::Arc;

use anyhow::{Context, Result, bail};
use minijinja::Value;
use minijinja::value::{Enumerator, Object, ObjectRepr};

use crate::config;
use crate::types::content::{ContentItem, ContentType};
use crate::types::manifest::Manifest;
use crate::types::var_value::VarValue;

// ---------------------------------------------------------------------------
// MiniJinja environment setup
// ---------------------------------------------------------------------------

fn create_env() -> Result<minijinja::Environment<'static>> {
    let mut env = minijinja::Environment::new();
    env.set_syntax(
        minijinja::syntax::SyntaxConfig::builder()
            .block_delimiters("{%~", "~%}")
            .variable_delimiters("{~", "~}")
            .comment_delimiters("{#~", "~#}")
            .build()
            .context("Failed to configure template syntax")?,
    );
    env.set_undefined_behavior(minijinja::UndefinedBehavior::Strict);
    env.set_fuel(Some(50_000));
    Ok(env)
}

// ---------------------------------------------------------------------------
// Expression scanning
// ---------------------------------------------------------------------------

/// Check if content contains any Jolene template expressions.
pub fn scan_for_expressions(content: &str) -> bool {
    content.contains("{~") || content.contains("{%~") || content.contains("{#~")
}

// ---------------------------------------------------------------------------
// Scan content items for template expressions
// ---------------------------------------------------------------------------

/// Scan each content item's source files and set `templated = true` if any
/// file contains template expressions.
///
/// Items whose names appear in `exclude` are skipped — their `templated` flag
/// stays `false` and their files are never read. This lets authors opt out of
/// template rendering for content that contains literal template delimiters.
pub fn scan_content_items(
    items: &mut [ContentItem],
    clone_root: &Path,
    exclude: &std::collections::HashSet<&str>,
) -> Result<()> {
    for item in items.iter_mut() {
        if exclude.contains(item.name.as_str()) {
            continue;
        }
        match item.content_type {
            ContentType::Command | ContentType::Agent => {
                let path = item.source_path(clone_root);
                let content = std::fs::read_to_string(&path)
                    .with_context(|| format!("Failed to read {}", path.display()))?;
                item.templated = scan_for_expressions(&content);
            }
            ContentType::Skill => {
                let skill_dir = item.source_path(clone_root);
                item.templated = scan_skill_dir(&skill_dir)?;
            }
        }
    }
    Ok(())
}

/// Recursively scan a skill directory for template expressions in any file.
fn scan_skill_dir(dir: &Path) -> Result<bool> {
    for entry in std::fs::read_dir(dir)
        .with_context(|| format!("Failed to read skill directory {}", dir.display()))?
    {
        let entry = entry?;
        let path = entry.path();
        if path.is_dir() && scan_skill_dir(&path)? {
            return Ok(true);
        } else if path.is_file()
            && let Ok(content) = std::fs::read_to_string(&path)
            && scan_for_expressions(&content)
        {
            return Ok(true);
        }
    }
    Ok(false)
}

// ---------------------------------------------------------------------------
// Parse and validate var overrides
// ---------------------------------------------------------------------------

/// Parse `--var` and `--vars-json` flags, validate against declared vars.
///
/// Returns `(merged_vars, overrides_only)`. `overrides_only` is `None` when
/// no flags were given; otherwise it contains only the user-supplied overrides.
#[allow(clippy::type_complexity)]
pub fn parse_and_validate_var_overrides(
    var_flags: &[String],
    vars_json_flags: &[String],
    declared_vars: &BTreeMap<String, VarValue>,
) -> Result<(BTreeMap<String, VarValue>, Option<BTreeMap<String, VarValue>>)> {
    if var_flags.is_empty() && vars_json_flags.is_empty() {
        return Ok((declared_vars.clone(), None));
    }

    let mut merged = declared_vars.clone();
    let mut overrides: BTreeMap<String, VarValue> = BTreeMap::new();

    // Process left-to-right: interleave --var and --vars-json is not possible
    // with clap (they come as separate vecs), so we process --var first, then --vars-json.
    // The plan says "left-to-right" but since clap collects them separately,
    // we process --var flags then --vars-json flags.

    for flag in var_flags {
        let (key, raw_value) = flag
            .split_once('=')
            .ok_or_else(|| anyhow::anyhow!("--var '{}': expected KEY=VALUE format", flag))?;

        let declared = declared_vars.get(key).ok_or_else(|| {
            let available = declared_var_list(declared_vars);
            anyhow::anyhow!(
                "--var {}: key '{}' is not declared in [template.vars].\n  Declared vars: {}",
                flag,
                key,
                available
            )
        })?;

        let value = infer_var_value(raw_value);

        if !value.type_matches(declared) {
            bail!(
                "--var {}={}: declared as {} in [template.vars], expected {}",
                key,
                raw_value,
                declared.type_label(),
                type_expectation(declared)
            );
        }

        merged.insert(key.to_string(), value.clone());
        overrides.insert(key.to_string(), value);
    }

    for json_str in vars_json_flags {
        let parsed: serde_json::Value = serde_json::from_str(json_str)
            .map_err(|e| anyhow::anyhow!("--vars-json: invalid JSON: {}", e))?;

        let serde_json::Value::Object(map) = parsed else {
            let type_name = json_type_name(&parsed);
            bail!(
                "--vars-json: expected a JSON object at the top level, got {}.",
                type_name
            );
        };

        for (key, json_val) in map {
            if json_val.is_null() {
                bail!(
                    "--vars-json: key '{}' has a null value, which is not supported.\n  Permitted types: string, bool, number, array, object.",
                    key
                );
            }

            let declared = declared_vars.get(&key).ok_or_else(|| {
                let available = declared_var_list(declared_vars);
                anyhow::anyhow!(
                    "--vars-json: key '{}' is not declared in [template.vars].\n  Declared vars: {}",
                    key,
                    available
                )
            })?;

            let value = VarValue::from_json_value(json_val)?;

            if !value.type_matches(declared) {
                bail!(
                    "--vars-json: key '{}' declared as {} in [template.vars],\n  but got a {} value.",
                    key,
                    declared.type_label(),
                    value.type_label()
                );
            }

            // Deep merge for objects, replace for everything else.
            if let Some(existing) = merged.get_mut(&key) {
                existing.deep_merge(value.clone());
            } else {
                merged.insert(key.clone(), value.clone());
            }

            if let Some(existing_override) = overrides.get_mut(&key) {
                existing_override.deep_merge(value);
            } else {
                overrides.insert(key, value);
            }
        }
    }

    Ok((merged, Some(overrides)))
}

/// Infer a VarValue from a raw string (from `--var key=value`).
fn infer_var_value(raw: &str) -> VarValue {
    if raw == "true" {
        return VarValue::Bool(true);
    }
    if raw == "false" {
        return VarValue::Bool(false);
    }
    if let Ok(i) = raw.parse::<i64>() {
        return VarValue::Int(i);
    }
    if let Ok(f) = raw.parse::<f64>()
        && f.is_finite()
    {
        return VarValue::Float(f);
    }
    VarValue::String(raw.to_string())
}

/// Human-readable expectation string for type mismatch errors.
fn type_expectation(v: &VarValue) -> &'static str {
    match v {
        VarValue::Bool(_) => "true or false",
        VarValue::Int(_) => "an integer",
        VarValue::Float(_) => "a float",
        VarValue::String(_) => "a string",
        VarValue::Array(_) => "an array (use --vars-json)",
        VarValue::Object(_) => "an object (use --vars-json)",
    }
}

fn json_type_name(v: &serde_json::Value) -> &'static str {
    match v {
        serde_json::Value::Null => "null",
        serde_json::Value::Bool(_) => "bool",
        serde_json::Value::Number(_) => "number",
        serde_json::Value::String(_) => "string",
        serde_json::Value::Array(_) => "array",
        serde_json::Value::Object(_) => "object",
    }
}

fn declared_var_list(vars: &BTreeMap<String, VarValue>) -> String {
    if vars.is_empty() {
        return "(none)".to_string();
    }
    vars.keys().map(|s| s.as_str()).collect::<Vec<_>>().join(", ")
}

// ---------------------------------------------------------------------------
// JoleneObject — the `jolene` template context
// ---------------------------------------------------------------------------

/// The `jolene` object exposed to templates.
struct JoleneObject {
    prefix: String,
    target: String,
    bundle_name: String,
    bundle_version: String,
    vars: Value,
    items: Vec<ContentItem>,
}

/// Return the user-facing name with prefix applied (no file extension).
fn prefixed_name(prefix: &str, name: &str) -> String {
    if prefix.is_empty() {
        name.to_string()
    } else {
        format!("{prefix}--{name}")
    }
}

impl std::fmt::Display for JoleneObject {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "jolene")
    }
}

impl std::fmt::Debug for JoleneObject {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "JoleneObject")
    }
}

impl Object for JoleneObject {
    fn repr(self: &Arc<Self>) -> ObjectRepr {
        ObjectRepr::Map
    }

    fn get_value(self: &Arc<Self>, key: &Value) -> Option<Value> {
        match key.as_str()? {
            "prefix" => Some(Value::from(self.prefix.as_str())),
            "target" => Some(Value::from(self.target.as_str())),
            "bundle" => Some(Value::from_iter([
                ("name", Value::from(self.bundle_name.as_str())),
                ("version", Value::from(self.bundle_version.as_str())),
            ])),
            "vars" => Some(self.vars.clone()),
            _ => None,
        }
    }

    fn enumerate(self: &Arc<Self>) -> Enumerator {
        Enumerator::Str(&["prefix", "target", "bundle", "vars"])
    }

    fn call_method(
        self: &Arc<Self>,
        _state: &minijinja::State,
        name: &str,
        args: &[Value],
    ) -> Result<Value, minijinja::Error> {
        match name {
            "resolve" => {
                let item_name = args
                    .first()
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| {
                        minijinja::Error::new(
                            minijinja::ErrorKind::InvalidOperation,
                            "jolene.resolve() requires a string argument",
                        )
                    })?
                    .to_string();

                let content_type_filter = args.get(1).and_then(|v| v.as_str()).map(String::from);

                // Validate content type filter if provided.
                if let Some(ref ct) = content_type_filter
                    && !["command", "skill", "agent"].contains(&ct.as_str())
                {
                    return Err(minijinja::Error::new(
                        minijinja::ErrorKind::InvalidOperation,
                        format!(
                            "jolene.resolve(\"{}\", \"{}\"): \"{}\" is not a valid content type.\n  Valid types: command, skill, agent.",
                            item_name, ct, ct
                        ),
                    ));
                }

                // Find matching items.
                let matches: Vec<&ContentItem> = self
                    .items
                    .iter()
                    .filter(|i| {
                        if i.name != item_name {
                            return false;
                        }
                        if let Some(ref ct) = content_type_filter {
                            i.content_type.label() == ct.as_str()
                        } else {
                            true
                        }
                    })
                    .collect();

                match matches.len() {
                    0 => {
                        let declared: Vec<String> = self
                            .items
                            .iter()
                            .map(|i| format!("{} ({})", i.name, i.content_type.label()))
                            .collect();
                        Err(minijinja::Error::new(
                            minijinja::ErrorKind::InvalidOperation,
                            format!(
                                "jolene.resolve(\"{}\") references content item '{}', which is not declared in this bundle.\n  Declared items: {}",
                                item_name, item_name, declared.join(", ")
                            ),
                        ))
                    }
                    1 => {
                        let resolved = prefixed_name(&self.prefix, &matches[0].name);
                        Ok(Value::from(resolved))
                    }
                    _ => {
                        if content_type_filter.is_some() {
                            // Multiple items with same name and same content type — shouldn't happen but handle it.
                            let resolved = prefixed_name(&self.prefix, &matches[0].name);
                            Ok(Value::from(resolved))
                        } else {
                            let types: Vec<&str> =
                                matches.iter().map(|i| i.content_type.label()).collect();
                            Err(minijinja::Error::new(
                                minijinja::ErrorKind::InvalidOperation,
                                format!(
                                    "jolene.resolve(\"{}\") is ambiguous — '{}' exists as both a {}.\n  Use jolene.resolve(\"{}\", \"{}\") to disambiguate.",
                                    item_name,
                                    item_name,
                                    types.join(" and a "),
                                    item_name,
                                    types[0]
                                ),
                            ))
                        }
                    }
                }
            }
            _ => Err(minijinja::Error::from(minijinja::ErrorKind::UnknownMethod)),
        }
    }
}

// ---------------------------------------------------------------------------
// Rendering
// ---------------------------------------------------------------------------

/// Render all templated content items for a given target.
///
/// Writes rendered output to `~/.jolene/rendered/{hash}/{target}/...`.
pub fn render_content_items(
    items: &[ContentItem],
    clone_root: &Path,
    store_key: &str,
    target_slug: &str,
    prefix: Option<&str>,
    manifest: &Manifest,
    merged_vars: &BTreeMap<String, VarValue>,
) -> Result<()> {
    let has_templated = items.iter().any(|i| i.templated);
    if !has_templated {
        return Ok(());
    }

    let rendered_root = config::rendered_path_for(store_key, target_slug)?;

    let vars_value = build_vars_value(merged_vars);

    let jolene_obj = JoleneObject {
        prefix: prefix.unwrap_or("").to_string(),
        target: target_slug.to_string(),
        bundle_name: manifest.bundle.name.clone(),
        bundle_version: manifest.bundle.version.clone(),
        vars: vars_value,
        items: items.to_vec(),
    };

    let mut env = create_env()?;
    env.add_global("jolene", Value::from_object(jolene_obj));

    for item in items.iter().filter(|i| i.templated) {
        match item.content_type {
            ContentType::Command | ContentType::Agent => {
                render_single_file(
                    &env,
                    &item.source_path(clone_root),
                    &item.rendered_path(&rendered_root),
                    &item.relative_path().to_string_lossy(),
                )?;
            }
            ContentType::Skill => {
                render_skill_dir(
                    &env,
                    &item.source_path(clone_root),
                    &rendered_root.join(item.relative_path()),
                    &item.name,
                )?;
            }
        }
    }

    Ok(())
}

/// Render a single file through MiniJinja.
fn render_single_file(
    env: &minijinja::Environment<'_>,
    src: &Path,
    dst: &Path,
    display_name: &str,
) -> Result<()> {
    let source = std::fs::read_to_string(src)
        .with_context(|| format!("Failed to read template source: {}", src.display()))?;

    let rendered = render_template(env, &source, display_name)?;

    if let Some(parent) = dst.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("Failed to create directory {}", parent.display()))?;
    }

    std::fs::write(dst, rendered)
        .with_context(|| format!("Failed to write rendered file {}", dst.display()))?;

    Ok(())
}

/// Render a skill directory: render templated files, copy others as-is.
fn render_skill_dir(
    env: &minijinja::Environment<'_>,
    src_dir: &Path,
    dst_dir: &Path,
    skill_name: &str,
) -> Result<()> {
    std::fs::create_dir_all(dst_dir)
        .with_context(|| format!("Failed to create rendered skill directory {}", dst_dir.display()))?;

    let prefix = Path::new("skills").join(skill_name);
    render_skill_dir_recursive(env, src_dir, dst_dir, &prefix)
}

fn render_skill_dir_recursive(
    env: &minijinja::Environment<'_>,
    src_dir: &Path,
    dst_dir: &Path,
    rel_prefix: &Path,
) -> Result<()> {
    for entry in std::fs::read_dir(src_dir)
        .with_context(|| format!("Failed to read skill directory {}", src_dir.display()))?
    {
        let entry = entry?;
        let src_path = entry.path();
        let file_name = entry.file_name();
        let dst_path = dst_dir.join(&file_name);

        if src_path.is_dir() {
            std::fs::create_dir_all(&dst_path)?;
            let child_prefix = rel_prefix.join(&file_name);
            render_skill_dir_recursive(env, &src_path, &dst_path, &child_prefix)?;
        } else if src_path.is_file() {
            if let Ok(content) = std::fs::read_to_string(&src_path) {
                if scan_for_expressions(&content) {
                    let display = rel_prefix
                        .join(&file_name)
                        .to_string_lossy()
                        .into_owned();
                    let rendered = render_template(env, &content, &display)?;
                    std::fs::write(&dst_path, rendered)?;
                } else {
                    std::fs::copy(&src_path, &dst_path)?;
                }
            } else {
                // Binary file — copy as-is.
                std::fs::copy(&src_path, &dst_path)?;
            }
        }
    }
    Ok(())
}

/// Render a template string through the MiniJinja environment.
fn render_template(
    env: &minijinja::Environment<'_>,
    source: &str,
    display_name: &str,
) -> Result<String> {
    env.render_str(source, ())
        .map_err(|e| format_template_error(e, display_name))
}

/// Convert a MiniJinja error into a user-friendly anyhow error.
fn format_template_error(err: minijinja::Error, file: &str) -> anyhow::Error {
    let kind = err.kind();
    match kind {
        minijinja::ErrorKind::OutOfFuel => {
            anyhow::anyhow!(
                "Template in {} exceeded execution limit.\n  Possible infinite loop in template logic.",
                file
            )
        }
        minijinja::ErrorKind::SyntaxError => {
            if let Some(line) = err.line() {
                anyhow::anyhow!(
                    "Template syntax error in {} (line {}):\n  {}",
                    file,
                    line,
                    err
                )
            } else {
                anyhow::anyhow!("Template syntax error in {}:\n  {}", file, err)
            }
        }
        _ => {
            if let Some(detail) = err.detail() {
                anyhow::anyhow!("Template error in {}:\n  {}", file, detail)
            } else {
                anyhow::anyhow!("Template error in {}:\n  {}", file, err)
            }
        }
    }
}

fn build_vars_value(vars: &BTreeMap<String, VarValue>) -> Value {
    let pairs: Vec<(String, Value)> = vars
        .iter()
        .map(|(k, v)| (k.clone(), v.clone().into_minijinja_value()))
        .collect();
    Value::from_iter(pairs)
}

// ---------------------------------------------------------------------------
// Validation of stored overrides (for update flow)
// ---------------------------------------------------------------------------

/// Validate that stored var overrides are still compatible with the manifest.
///
/// Returns an error if a stored key was removed or its type changed.
pub fn validate_stored_overrides(
    stored: &BTreeMap<String, VarValue>,
    declared: &BTreeMap<String, VarValue>,
    source_kind_flag: &str,
) -> Result<()> {
    for (key, val) in stored {
        match declared.get(key) {
            None => {
                let available = declared_var_list(declared);
                bail!(
                    "Stored variable override '{}' is no longer declared in [template.vars].\n  The bundle update removed this variable. Uninstall and reinstall with corrected overrides:\n    jolene uninstall {} && jolene install {} [--var key=value] [--vars-json ...]\n  Declared vars: {}",
                    key,
                    source_kind_flag,
                    source_kind_flag,
                    available
                );
            }
            Some(declared_val) => {
                if !val.type_matches(declared_val) {
                    let available = declared_var_list(declared);
                    bail!(
                        "Stored variable override '{}' has type {}, but [template.vars]\n  now declares it as {}. Uninstall and reinstall with corrected overrides:\n    jolene uninstall {} && jolene install {} [--var key=value] [--vars-json ...]\n  Declared vars: {}",
                        key,
                        val.type_label(),
                        declared_val.type_label(),
                        source_kind_flag,
                        source_kind_flag,
                        available
                    );
                }
            }
        }
    }
    Ok(())
}

/// Rebuild merged vars from declared defaults + stored overrides.
pub fn merge_stored_overrides(
    declared: &BTreeMap<String, VarValue>,
    stored: &BTreeMap<String, VarValue>,
) -> BTreeMap<String, VarValue> {
    let mut merged = declared.clone();
    for (key, val) in stored {
        if let Some(existing) = merged.get_mut(key) {
            existing.deep_merge(val.clone());
        }
    }
    merged
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scan_for_expressions_detects_variable() {
        assert!(scan_for_expressions("Hello {~ jolene.prefix ~}"));
    }

    #[test]
    fn scan_for_expressions_detects_block() {
        assert!(scan_for_expressions("{%~ if true ~%}yes{%~ endif ~%}"));
    }

    #[test]
    fn scan_for_expressions_detects_comment() {
        assert!(scan_for_expressions("{#~ a comment ~#}"));
    }

    #[test]
    fn scan_for_expressions_negative() {
        assert!(!scan_for_expressions("No template here {{ jinja2 }}"));
    }

    #[test]
    fn scan_for_expressions_empty() {
        assert!(!scan_for_expressions(""));
    }

    #[test]
    fn infer_var_value_bool_true() {
        assert_eq!(infer_var_value("true"), VarValue::Bool(true));
    }

    #[test]
    fn infer_var_value_bool_false() {
        assert_eq!(infer_var_value("false"), VarValue::Bool(false));
    }

    #[test]
    fn infer_var_value_int() {
        assert_eq!(infer_var_value("42"), VarValue::Int(42));
    }

    #[test]
    fn infer_var_value_float() {
        assert_eq!(infer_var_value("3.14"), VarValue::Float(3.14));
    }

    #[test]
    fn infer_var_value_string() {
        assert_eq!(
            infer_var_value("hello"),
            VarValue::String("hello".into())
        );
    }

    #[test]
    fn infer_var_value_url_stays_string() {
        assert_eq!(
            infer_var_value("https://example.com?a=1&b=2"),
            VarValue::String("https://example.com?a=1&b=2".into())
        );
    }

    #[test]
    fn infer_var_value_nan_stays_string() {
        assert_eq!(infer_var_value("NaN"), VarValue::String("NaN".into()));
    }

    #[test]
    fn infer_var_value_infinity_stays_string() {
        assert_eq!(
            infer_var_value("inf"),
            VarValue::String("inf".into())
        );
        assert_eq!(
            infer_var_value("infinity"),
            VarValue::String("infinity".into())
        );
    }

    #[test]
    fn parse_overrides_no_flags() {
        let declared = BTreeMap::from([("key".into(), VarValue::String("default".into()))]);
        let (merged, overrides) =
            parse_and_validate_var_overrides(&[], &[], &declared).unwrap();
        assert_eq!(merged.get("key"), Some(&VarValue::String("default".into())));
        assert!(overrides.is_none());
    }

    #[test]
    fn parse_overrides_var_flag() {
        let declared = BTreeMap::from([("name".into(), VarValue::String("default".into()))]);
        let (merged, overrides) = parse_and_validate_var_overrides(
            &["name=custom".to_string()],
            &[],
            &declared,
        )
        .unwrap();
        assert_eq!(
            merged.get("name"),
            Some(&VarValue::String("custom".into()))
        );
        assert!(overrides.is_some());
    }

    #[test]
    fn parse_overrides_unknown_key_errors() {
        let declared = BTreeMap::from([("name".into(), VarValue::String("default".into()))]);
        let result = parse_and_validate_var_overrides(
            &["typo=value".to_string()],
            &[],
            &declared,
        );
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("typo"));
    }

    #[test]
    fn parse_overrides_type_mismatch_errors() {
        let declared = BTreeMap::from([("flag".into(), VarValue::Bool(false))]);
        let result = parse_and_validate_var_overrides(
            &["flag=notabool".to_string()],
            &[],
            &declared,
        );
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("bool"));
    }

    #[test]
    fn parse_overrides_vars_json() {
        let declared = BTreeMap::from([
            ("channels".into(), VarValue::Array(vec![VarValue::String("slack".into())])),
        ]);
        let (merged, _) = parse_and_validate_var_overrides(
            &[],
            &[r#"{"channels": ["email", "pagerduty"]}"#.to_string()],
            &declared,
        )
        .unwrap();
        assert_eq!(
            merged.get("channels"),
            Some(&VarValue::Array(vec![
                VarValue::String("email".into()),
                VarValue::String("pagerduty".into())
            ]))
        );
    }

    #[test]
    fn parse_overrides_vars_json_null_errors() {
        let declared = BTreeMap::from([("key".into(), VarValue::String("val".into()))]);
        let result = parse_and_validate_var_overrides(
            &[],
            &[r#"{"key": null}"#.to_string()],
            &declared,
        );
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("null"));
    }

    #[test]
    fn parse_overrides_vars_json_not_object_errors() {
        let declared = BTreeMap::new();
        let result =
            parse_and_validate_var_overrides(&[], &[r#""just a string""#.to_string()], &declared);
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("expected a JSON object"));
    }

    #[test]
    fn render_simple_template() {
        let env = create_env().unwrap();
        let result = render_template(&env, "plain text no expressions", "test.md");
        assert_eq!(result.unwrap(), "plain text no expressions");
    }

    #[test]
    fn render_with_jolene_context() {
        let items = vec![
            ContentItem::new(ContentType::Command, "deploy"),
        ];
        let vars = BTreeMap::from([("url".into(), VarValue::String("https://example.com".into()))]);

        let obj = JoleneObject {
            prefix: "acme".to_string(),
            target: "claude-code".to_string(),
            bundle_name: "test-pkg".to_string(),
            bundle_version: "1.0.0".to_string(),
            vars: build_vars_value(&vars),
            items,
        };

        let mut env = create_env().unwrap();
        env.add_global("jolene", Value::from_object(obj));

        let template = "prefix={~ jolene.prefix ~} target={~ jolene.target ~} url={~ jolene.vars.url ~}";
        let result = env.render_str(template, ()).unwrap();
        assert_eq!(result, "prefix=acme target=claude-code url=https://example.com");
    }

    #[test]
    fn render_resolve_with_prefix() {
        let items = vec![ContentItem::new(ContentType::Command, "deploy")];

        let obj = JoleneObject {
            prefix: "acme".to_string(),
            target: "claude-code".to_string(),
            bundle_name: "test".to_string(),
            bundle_version: "1.0.0".to_string(),
            vars: Value::from_iter(Vec::<(String, Value)>::new()),
            items,
        };

        let mut env = create_env().unwrap();
        env.add_global("jolene", Value::from_object(obj));

        let result = env.render_str("{~ jolene.resolve(\"deploy\") ~}", ()).unwrap();
        assert_eq!(result, "acme--deploy");
    }

    #[test]
    fn render_resolve_no_prefix() {
        let items = vec![ContentItem::new(ContentType::Command, "deploy")];

        let obj = JoleneObject {
            prefix: "".to_string(),
            target: "claude-code".to_string(),
            bundle_name: "test".to_string(),
            bundle_version: "1.0.0".to_string(),
            vars: Value::from_iter(Vec::<(String, Value)>::new()),
            items,
        };

        let mut env = create_env().unwrap();
        env.add_global("jolene", Value::from_object(obj));

        let result = env.render_str("{~ jolene.resolve(\"deploy\") ~}", ()).unwrap();
        assert_eq!(result, "deploy");
    }

    #[test]
    fn render_resolve_unknown_item_errors() {
        let items = vec![ContentItem::new(ContentType::Command, "deploy")];

        let obj = JoleneObject {
            prefix: "".to_string(),
            target: "claude-code".to_string(),
            bundle_name: "test".to_string(),
            bundle_version: "1.0.0".to_string(),
            vars: Value::from_iter(Vec::<(String, Value)>::new()),
            items,
        };

        let mut env = create_env().unwrap();
        env.add_global("jolene", Value::from_object(obj));

        let result = env.render_str("{~ jolene.resolve(\"nonexistent\") ~}", ());
        assert!(result.is_err());
    }

    #[test]
    fn render_resolve_ambiguous_errors() {
        let items = vec![
            ContentItem::new(ContentType::Command, "review"),
            ContentItem::new(ContentType::Skill, "review"),
        ];

        let obj = JoleneObject {
            prefix: "".to_string(),
            target: "claude-code".to_string(),
            bundle_name: "test".to_string(),
            bundle_version: "1.0.0".to_string(),
            vars: Value::from_iter(Vec::<(String, Value)>::new()),
            items,
        };

        let mut env = create_env().unwrap();
        env.add_global("jolene", Value::from_object(obj));

        let result = env.render_str("{~ jolene.resolve(\"review\") ~}", ());
        assert!(result.is_err());
    }

    #[test]
    fn render_resolve_disambiguated() {
        let items = vec![
            ContentItem::new(ContentType::Command, "review"),
            ContentItem::new(ContentType::Skill, "review"),
        ];

        let obj = JoleneObject {
            prefix: "".to_string(),
            target: "claude-code".to_string(),
            bundle_name: "test".to_string(),
            bundle_version: "1.0.0".to_string(),
            vars: Value::from_iter(Vec::<(String, Value)>::new()),
            items,
        };

        let mut env = create_env().unwrap();
        env.add_global("jolene", Value::from_object(obj));

        let result = env
            .render_str("{~ jolene.resolve(\"review\", \"command\") ~}", ())
            .unwrap();
        assert_eq!(result, "review");
    }

    #[test]
    fn render_bundle_info() {
        let obj = JoleneObject {
            prefix: "".to_string(),
            target: "claude-code".to_string(),
            bundle_name: "my-pkg".to_string(),
            bundle_version: "2.1.0".to_string(),
            vars: Value::from_iter(Vec::<(String, Value)>::new()),
            items: vec![],
        };

        let mut env = create_env().unwrap();
        env.add_global("jolene", Value::from_object(obj));

        let result = env
            .render_str("{~ jolene.bundle.name ~} v{~ jolene.bundle.version ~}", ())
            .unwrap();
        assert_eq!(result, "my-pkg v2.1.0");
    }

    #[test]
    fn render_conditional_target() {
        let obj = JoleneObject {
            prefix: "".to_string(),
            target: "codex".to_string(),
            bundle_name: "test".to_string(),
            bundle_version: "1.0.0".to_string(),
            vars: Value::from_iter(Vec::<(String, Value)>::new()),
            items: vec![],
        };

        let mut env = create_env().unwrap();
        env.add_global("jolene", Value::from_object(obj));

        let template = "{%~ if jolene.target == \"codex\" ~%}codex-only{%~ endif ~%}";
        let result = env.render_str(template, ()).unwrap();
        assert_eq!(result, "codex-only");
    }

    #[test]
    fn validate_stored_overrides_removed_key() {
        let stored = BTreeMap::from([("old_key".into(), VarValue::String("val".into()))]);
        let declared = BTreeMap::from([("new_key".into(), VarValue::String("val".into()))]);
        let result = validate_stored_overrides(&stored, &declared, "--github test/test");
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("old_key"));
    }

    #[test]
    fn validate_stored_overrides_type_changed() {
        let stored = BTreeMap::from([("flag".into(), VarValue::Bool(true))]);
        let declared = BTreeMap::from([("flag".into(), VarValue::String("now a string".into()))]);
        let result = validate_stored_overrides(&stored, &declared, "--github test/test");
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("bool"));
    }

    #[test]
    fn validate_stored_overrides_ok() {
        let stored = BTreeMap::from([("key".into(), VarValue::String("override".into()))]);
        let declared = BTreeMap::from([("key".into(), VarValue::String("default".into()))]);
        assert!(
            validate_stored_overrides(&stored, &declared, "--github test/test").is_ok()
        );
    }

    // scan_content_items exclude tests

    #[test]
    fn scan_content_items_excluded_item_stays_not_templated() {
        let dir = tempfile::tempdir().unwrap();
        let commands_dir = dir.path().join("commands");
        std::fs::create_dir_all(&commands_dir).unwrap();
        // Write a file that would normally be detected as templated.
        std::fs::write(
            commands_dir.join("docs.md"),
            "Use {~ jolene.resolve(\"deploy\") ~} to invoke.",
        )
        .unwrap();

        let mut items = vec![ContentItem::new(ContentType::Command, "docs")];
        let exclude = std::collections::HashSet::from(["docs"]);
        scan_content_items(&mut items, dir.path(), &exclude).unwrap();
        assert!(!items[0].templated, "excluded item should not be templated");
    }

    #[test]
    fn scan_content_items_non_excluded_with_delimiters_is_templated() {
        let dir = tempfile::tempdir().unwrap();
        let commands_dir = dir.path().join("commands");
        std::fs::create_dir_all(&commands_dir).unwrap();
        std::fs::write(
            commands_dir.join("docs.md"),
            "Use {~ jolene.resolve(\"deploy\") ~} to invoke.",
        )
        .unwrap();

        let mut items = vec![ContentItem::new(ContentType::Command, "docs")];
        let exclude = std::collections::HashSet::new();
        scan_content_items(&mut items, dir.path(), &exclude).unwrap();
        assert!(items[0].templated, "non-excluded item with delimiters should be templated");
    }

    #[test]
    fn scan_content_items_empty_exclude_unchanged() {
        let dir = tempfile::tempdir().unwrap();
        let commands_dir = dir.path().join("commands");
        std::fs::create_dir_all(&commands_dir).unwrap();
        std::fs::write(commands_dir.join("plain.md"), "no template expressions here").unwrap();

        let mut items = vec![ContentItem::new(ContentType::Command, "plain")];
        let exclude = std::collections::HashSet::new();
        scan_content_items(&mut items, dir.path(), &exclude).unwrap();
        assert!(!items[0].templated);
    }
}
