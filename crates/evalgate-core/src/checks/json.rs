use super::result;
use crate::schema::{CheckResult, CheckSpec};
use serde_json::Value;
pub fn run(spec: &CheckSpec, output: &str) -> Result<CheckResult, String> {
    let parsed: Value = match serde_json::from_str(output) {
        Ok(v) => v,
        Err(e) => return Ok(result(&spec.kind, false, 0.0, format!("invalid JSON: {e}"))),
    };
    if spec.kind == "json_valid" {
        return Ok(result("json_valid", true, 1.0, "valid JSON"));
    }
    let mut errors = Vec::new();
    validate_shape(&parsed, &spec.value, "$", &mut errors)?;
    Ok(result(
        "json_shape",
        errors.is_empty(),
        f64::from(errors.is_empty()),
        if errors.is_empty() {
            "shape matched".into()
        } else {
            errors.join("; ")
        },
    ))
}
fn validate_shape(
    value: &Value,
    schema: &Value,
    path: &str,
    errors: &mut Vec<String>,
) -> Result<(), String> {
    let object = schema
        .as_object()
        .ok_or("json_shape value must be an object")?;
    if let Some(expected) = object.get("type").and_then(Value::as_str) {
        let valid = match expected {
            "object" => value.is_object(),
            "array" => value.is_array(),
            "string" => value.is_string(),
            "number" => value.is_number(),
            "integer" => value.as_i64().is_some() || value.as_u64().is_some(),
            "boolean" => value.is_boolean(),
            "null" => value.is_null(),
            other => return Err(format!("unsupported JSON shape type {other:?}")),
        };
        if !valid {
            errors.push(format!("{path}: expected {expected}"));
            return Ok(());
        }
    }
    if let Some(values) = object.get("enum").and_then(Value::as_array) {
        if !values.contains(value) {
            errors.push(format!("{path}: value is not in enum"));
        }
    }
    if let Some(min) = object.get("min").and_then(Value::as_f64) {
        if value.as_f64().is_none_or(|v| v < min) {
            errors.push(format!("{path}: below minimum {min}"));
        }
    }
    if let Some(max) = object.get("max").and_then(Value::as_f64) {
        if value.as_f64().is_none_or(|v| v > max) {
            errors.push(format!("{path}: above maximum {max}"));
        }
    }
    if let Some(required) = object.get("required").and_then(Value::as_array) {
        let map = value.as_object();
        for key in required {
            let key = key
                .as_str()
                .ok_or("json_shape required entries must be strings")?;
            if map.is_none_or(|m| !m.contains_key(key)) {
                errors.push(format!("{path}: missing required property {key:?}"));
            }
        }
    }
    if let (Some(map), Some(properties)) = (
        value.as_object(),
        object.get("properties").and_then(Value::as_object),
    ) {
        for (key, child_schema) in properties {
            if let Some(child) = map.get(key) {
                validate_shape(child, child_schema, &format!("{path}.{key}"), errors)?;
            }
        }
    }
    if let (Some(items), Some(item_schema)) = (value.as_array(), object.get("items")) {
        for (i, item) in items.iter().enumerate() {
            validate_shape(item, item_schema, &format!("{path}[{i}]"), errors)?;
        }
    }
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    fn spec(v: Value) -> CheckSpec {
        CheckSpec {
            kind: "json_shape".into(),
            value: v,
            weight: 1.,
            threshold: 1.,
            endpoint: None,
            model: None,
        }
    }
    #[test]
    fn valid_json() {
        let mut s = spec(json!({}));
        s.kind = "json_valid".into();
        assert!(run(&s, "[]").unwrap().passed);
    }
    #[test]
    fn invalid_json() {
        let mut s = spec(json!({}));
        s.kind = "json_valid".into();
        assert!(!run(&s, "{").unwrap().passed);
    }
    #[test]
    fn required_and_properties() {
        let s =
            spec(json!({"type":"object","required":["id"],"properties":{"id":{"type":"integer"}}}));
        assert!(run(&s, r#"{"id":2}"#).unwrap().passed);
    }
    #[test]
    fn missing_required() {
        let s = spec(json!({"required":["id"]}));
        assert!(run(&s, "{}").unwrap().detail.contains("missing"));
    }
    #[test]
    fn array_items() {
        let s = spec(json!({"type":"array","items":{"type":"string"}}));
        assert!(!run(&s, r#"["x",1]"#).unwrap().passed);
    }
    #[test]
    fn enum_min_max() {
        let s = spec(json!({"enum":[2,3],"min":2,"max":3}));
        assert!(run(&s, "2").unwrap().passed);
        assert!(!run(&s, "4").unwrap().passed);
    }
    #[test]
    fn nested_error_path() {
        let s = spec(json!({"properties":{"x":{"type":"boolean"}}}));
        assert!(run(&s, r#"{"x":1}"#).unwrap().detail.contains("$.x"));
    }
}
