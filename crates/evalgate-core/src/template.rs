use regex::Regex;
use serde_json::Value;
pub fn render(template: &str, vars: &Value) -> Result<String, String> {
    let pattern = Regex::new(r"\{\{\s*([A-Za-z_][A-Za-z0-9_.]*)\s*\}\}").expect("static regex");
    let mut missing = None;
    let rendered = pattern.replace_all(template, |caps: &regex::Captures<'_>| {
        let key = &caps[1];
        match lookup(vars, key) {
            Some(Value::String(v)) => v.clone(),
            Some(v) if v.is_null() => "null".into(),
            Some(v) => v.to_string(),
            None => {
                missing = Some(key.to_owned());
                String::new()
            }
        }
    });
    missing.map_or_else(
        || Ok(rendered.into_owned()),
        |key| Err(format!("missing template variable {key:?}")),
    )
}
fn lookup<'a>(value: &'a Value, path: &str) -> Option<&'a Value> {
    path.split('.')
        .try_fold(value, |current, part| current.get(part))
}
#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    #[test]
    fn renders_strings_and_numbers() {
        assert_eq!(
            render("Hi {{ name }} #{{n}}", &json!({"name":"A","n":2})).unwrap(),
            "Hi A #2"
        );
    }
    #[test]
    fn renders_nested() {
        assert_eq!(render("{{a.b}}", &json!({"a":{"b":"c"}})).unwrap(), "c");
    }
    #[test]
    fn reports_missing() {
        assert_eq!(
            render("{{nope}}", &json!({})).unwrap_err(),
            "missing template variable \"nope\""
        );
    }
    #[test]
    fn leaves_plain_text() {
        assert_eq!(render("hello", &json!({})).unwrap(), "hello");
    }
}
