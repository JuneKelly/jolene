use std::collections::BTreeMap;

use anyhow::{Result, bail};
use serde::{Deserialize, Serialize};

/// A JSON-compatible recursive value type (no null).
///
/// Used for template variables declared in `[template.vars]` and
/// for `--var` / `--vars-json` overrides stored in state.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum VarValue {
    Bool(bool),
    Int(i64),
    Float(f64),
    String(String),
    Array(Vec<VarValue>),
    Object(BTreeMap<String, VarValue>),
}

impl VarValue {
    /// Human-readable type label for error messages.
    pub fn type_label(&self) -> &'static str {
        match self {
            VarValue::Bool(_) => "bool",
            VarValue::Int(_) => "int",
            VarValue::Float(_) => "float",
            VarValue::String(_) => "string",
            VarValue::Array(_) => "array",
            VarValue::Object(_) => "object",
        }
    }

    /// Check if two values have the same top-level type.
    pub fn type_matches(&self, other: &VarValue) -> bool {
        matches!(
            (self, other),
            (VarValue::Bool(_), VarValue::Bool(_))
                | (VarValue::Int(_), VarValue::Int(_))
                | (VarValue::Float(_), VarValue::Float(_))
                | (VarValue::String(_), VarValue::String(_))
                | (VarValue::Array(_), VarValue::Array(_))
                | (VarValue::Object(_), VarValue::Object(_))
        )
    }

    /// Deep merge `other` into `self`.
    ///
    /// For object keys: recursive merge. For scalars/arrays: replace.
    pub fn deep_merge(&mut self, other: VarValue) {
        match (self, other) {
            (VarValue::Object(base), VarValue::Object(overlay)) => {
                for (key, val) in overlay {
                    base.entry(key)
                        .and_modify(|existing| existing.deep_merge(val.clone()))
                        .or_insert(val);
                }
            }
            (this, replacement) => *this = replacement,
        }
    }

    /// Convert to a MiniJinja value for template rendering.
    pub fn into_minijinja_value(self) -> minijinja::Value {
        match self {
            VarValue::Bool(b) => minijinja::Value::from(b),
            VarValue::Int(i) => minijinja::Value::from(i),
            VarValue::Float(f) => minijinja::Value::from(f),
            VarValue::String(s) => minijinja::Value::from(s),
            VarValue::Array(arr) => {
                let items: Vec<minijinja::Value> =
                    arr.into_iter().map(|v| v.into_minijinja_value()).collect();
                minijinja::Value::from(items)
            }
            VarValue::Object(map) => {
                let pairs: Vec<(String, minijinja::Value)> = map
                    .into_iter()
                    .map(|(k, v)| (k, v.into_minijinja_value()))
                    .collect();
                minijinja::Value::from_iter(pairs)
            }
        }
    }

    /// Convert from a TOML value, rejecting datetime.
    pub fn from_toml_value(v: toml::Value) -> Result<VarValue> {
        match v {
            toml::Value::Boolean(b) => Ok(VarValue::Bool(b)),
            toml::Value::Integer(i) => Ok(VarValue::Int(i)),
            toml::Value::Float(f) => Ok(VarValue::Float(f)),
            toml::Value::String(s) => Ok(VarValue::String(s)),
            toml::Value::Array(arr) => {
                let items: Result<Vec<VarValue>> =
                    arr.into_iter().map(VarValue::from_toml_value).collect();
                Ok(VarValue::Array(items?))
            }
            toml::Value::Table(table) => {
                let mut map = BTreeMap::new();
                for (k, v) in table {
                    map.insert(k, VarValue::from_toml_value(v)?);
                }
                Ok(VarValue::Object(map))
            }
            toml::Value::Datetime(_) => {
                bail!("datetime values are not supported in [template.vars]")
            }
        }
    }

    /// Convert from a JSON value, rejecting null.
    pub fn from_json_value(v: serde_json::Value) -> Result<VarValue> {
        match v {
            serde_json::Value::Bool(b) => Ok(VarValue::Bool(b)),
            serde_json::Value::Number(n) => {
                if let Some(i) = n.as_i64() {
                    Ok(VarValue::Int(i))
                } else if let Some(f) = n.as_f64() {
                    Ok(VarValue::Float(f))
                } else {
                    bail!("unsupported JSON number: {}", n)
                }
            }
            serde_json::Value::String(s) => Ok(VarValue::String(s)),
            serde_json::Value::Array(arr) => {
                let items: Result<Vec<VarValue>> =
                    arr.into_iter().map(VarValue::from_json_value).collect();
                Ok(VarValue::Array(items?))
            }
            serde_json::Value::Object(map) => {
                let mut out = BTreeMap::new();
                for (k, v) in map {
                    out.insert(k, VarValue::from_json_value(v)?);
                }
                Ok(VarValue::Object(out))
            }
            serde_json::Value::Null => {
                bail!("null values are not supported")
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn type_label_values() {
        assert_eq!(VarValue::Bool(true).type_label(), "bool");
        assert_eq!(VarValue::Int(42).type_label(), "int");
        assert_eq!(VarValue::Float(3.14).type_label(), "float");
        assert_eq!(VarValue::String("hi".into()).type_label(), "string");
        assert_eq!(VarValue::Array(vec![]).type_label(), "array");
        assert_eq!(VarValue::Object(BTreeMap::new()).type_label(), "object");
    }

    #[test]
    fn type_matches_same_types() {
        assert!(VarValue::Bool(true).type_matches(&VarValue::Bool(false)));
        assert!(VarValue::Int(1).type_matches(&VarValue::Int(2)));
        assert!(VarValue::String("a".into()).type_matches(&VarValue::String("b".into())));
    }

    #[test]
    fn type_matches_different_types() {
        assert!(!VarValue::Bool(true).type_matches(&VarValue::Int(1)));
        assert!(!VarValue::String("a".into()).type_matches(&VarValue::Float(1.0)));
    }

    #[test]
    fn deep_merge_scalars_replace() {
        let mut base = VarValue::String("old".into());
        base.deep_merge(VarValue::String("new".into()));
        assert_eq!(base, VarValue::String("new".into()));
    }

    #[test]
    fn deep_merge_objects_recursive() {
        let mut base = VarValue::Object(BTreeMap::from([
            ("a".into(), VarValue::Int(1)),
            (
                "nested".into(),
                VarValue::Object(BTreeMap::from([
                    ("x".into(), VarValue::Int(10)),
                    ("y".into(), VarValue::Int(20)),
                ])),
            ),
        ]));
        let overlay = VarValue::Object(BTreeMap::from([(
            "nested".into(),
            VarValue::Object(BTreeMap::from([("x".into(), VarValue::Int(99))])),
        )]));
        base.deep_merge(overlay);

        if let VarValue::Object(map) = &base {
            assert_eq!(map.get("a"), Some(&VarValue::Int(1)));
            if let Some(VarValue::Object(nested)) = map.get("nested") {
                assert_eq!(nested.get("x"), Some(&VarValue::Int(99)));
                assert_eq!(nested.get("y"), Some(&VarValue::Int(20)));
            } else {
                panic!("expected nested object");
            }
        } else {
            panic!("expected object");
        }
    }

    #[test]
    fn deep_merge_arrays_replace() {
        let mut base = VarValue::Array(vec![VarValue::Int(1)]);
        base.deep_merge(VarValue::Array(vec![VarValue::Int(2), VarValue::Int(3)]));
        assert_eq!(
            base,
            VarValue::Array(vec![VarValue::Int(2), VarValue::Int(3)])
        );
    }

    #[test]
    fn from_toml_value_string() {
        let v = toml::Value::String("hello".into());
        assert_eq!(
            VarValue::from_toml_value(v).unwrap(),
            VarValue::String("hello".into())
        );
    }

    #[test]
    fn from_toml_value_bool() {
        let v = toml::Value::Boolean(true);
        assert_eq!(VarValue::from_toml_value(v).unwrap(), VarValue::Bool(true));
    }

    #[test]
    fn from_toml_value_integer() {
        let v = toml::Value::Integer(42);
        assert_eq!(VarValue::from_toml_value(v).unwrap(), VarValue::Int(42));
    }

    #[test]
    fn from_toml_value_float() {
        let v = toml::Value::Float(3.14);
        assert_eq!(
            VarValue::from_toml_value(v).unwrap(),
            VarValue::Float(3.14)
        );
    }

    #[test]
    fn from_toml_value_array() {
        let v = toml::Value::Array(vec![toml::Value::Integer(1), toml::Value::Integer(2)]);
        assert_eq!(
            VarValue::from_toml_value(v).unwrap(),
            VarValue::Array(vec![VarValue::Int(1), VarValue::Int(2)])
        );
    }

    #[test]
    fn from_toml_value_table() {
        let mut table = toml::map::Map::new();
        table.insert("key".into(), toml::Value::String("val".into()));
        let v = toml::Value::Table(table);
        let result = VarValue::from_toml_value(v).unwrap();
        assert_eq!(
            result,
            VarValue::Object(BTreeMap::from([(
                "key".into(),
                VarValue::String("val".into())
            )]))
        );
    }

    #[test]
    fn from_toml_value_rejects_datetime() {
        // TOML datetimes must be parsed from a key=value context.
        let table: toml::Table = "dt = 1979-05-27T07:32:00Z".parse().unwrap();
        let v = table.into_iter().next().unwrap().1;
        assert!(VarValue::from_toml_value(v).is_err());
    }

    #[test]
    fn from_json_value_string() {
        let v = serde_json::Value::String("hello".into());
        assert_eq!(
            VarValue::from_json_value(v).unwrap(),
            VarValue::String("hello".into())
        );
    }

    #[test]
    fn from_json_value_bool() {
        let v = serde_json::Value::Bool(false);
        assert_eq!(
            VarValue::from_json_value(v).unwrap(),
            VarValue::Bool(false)
        );
    }

    #[test]
    fn from_json_value_integer() {
        let v = serde_json::json!(42);
        assert_eq!(VarValue::from_json_value(v).unwrap(), VarValue::Int(42));
    }

    #[test]
    fn from_json_value_float() {
        let v = serde_json::json!(3.14);
        assert_eq!(
            VarValue::from_json_value(v).unwrap(),
            VarValue::Float(3.14)
        );
    }

    #[test]
    fn from_json_value_array() {
        let v = serde_json::json!([1, 2]);
        assert_eq!(
            VarValue::from_json_value(v).unwrap(),
            VarValue::Array(vec![VarValue::Int(1), VarValue::Int(2)])
        );
    }

    #[test]
    fn from_json_value_object() {
        let v = serde_json::json!({"key": "val"});
        let result = VarValue::from_json_value(v).unwrap();
        assert_eq!(
            result,
            VarValue::Object(BTreeMap::from([(
                "key".into(),
                VarValue::String("val".into())
            )]))
        );
    }

    #[test]
    fn from_json_value_rejects_null() {
        let v = serde_json::Value::Null;
        assert!(VarValue::from_json_value(v).is_err());
    }

    #[test]
    fn into_minijinja_value_string() {
        let v = VarValue::String("test".into());
        let mj = v.into_minijinja_value();
        assert_eq!(mj.as_str(), Some("test"));
    }

    #[test]
    fn into_minijinja_value_bool() {
        let v = VarValue::Bool(true);
        let mj = v.into_minijinja_value();
        assert_eq!(mj.is_true(), true);
    }

    #[test]
    fn into_minijinja_value_int() {
        let v = VarValue::Int(42);
        let mj = v.into_minijinja_value();
        assert_eq!(mj.as_i64(), Some(42));
    }
}
