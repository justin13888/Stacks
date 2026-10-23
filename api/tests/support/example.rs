//! A minimal valid instance of a schema in the API's own document.
//!
//! The sweep uses it to send each operation a request that passes every
//! extractor, so what it then observes is the handler's own answer: a 500
//! from the degraded database, for instance. It covers the constructs the
//! document uses; anything else is a panic naming the schema, so a new
//! construct is noticed rather than silently mis-filled.

use serde_json::{Map, Value, json};

/// The document as JSON, for resolving `$ref`s.
pub struct Examples<'a> {
    document: &'a Value,
}

impl<'a> Examples<'a> {
    pub fn new(document: &'a Value) -> Self {
        Self { document }
    }

    fn resolve(&self, reference: &str) -> &'a Value {
        let pointer = reference
            .strip_prefix('#')
            .unwrap_or_else(|| panic!("a non-local $ref: {reference}"));
        self.document
            .pointer(pointer)
            .unwrap_or_else(|| panic!("an unresolved $ref: {reference}"))
    }

    /// A minimal instance of `schema`.
    pub fn of(&self, schema: &Value) -> Value {
        match schema {
            Value::Bool(_) => json!({}),
            Value::Object(o) => self.of_object(o),
            other => panic!("not a schema: {other}"),
        }
    }

    fn of_object(&self, o: &Map<String, Value>) -> Value {
        if let Some(Value::String(r)) = o.get("$ref") {
            return self.of(self.resolve(r));
        }
        if let Some(examples) = o.get("examples").and_then(Value::as_array)
            && let Some(first) = examples.first()
        {
            return first.clone();
        }
        if let Some(c) = o.get("const") {
            return c.clone();
        }
        if let Some(e) = o.get("enum").and_then(Value::as_array) {
            return e.first().cloned().unwrap_or(Value::Null);
        }
        for key in ["oneOf", "anyOf"] {
            if let Some(branches) = o.get(key).and_then(Value::as_array) {
                let first = branches
                    .iter()
                    .find(|b| b.get("type") != Some(&json!("null")))
                    .or_else(|| branches.first())
                    .unwrap_or_else(|| panic!("an empty {key}"));
                return self.merge(self.of(first), o);
            }
        }
        if let Some(parts) = o.get("allOf").and_then(Value::as_array) {
            let mut merged = Map::new();
            for part in parts {
                if let Value::Object(p) = self.of(part) {
                    merged.extend(p);
                }
            }
            return self.merge(Value::Object(merged), o);
        }
        let ty = match o.get("type") {
            Some(Value::String(t)) => t.clone(),
            Some(Value::Array(ts)) => ts
                .iter()
                .filter_map(Value::as_str)
                .find(|t| *t != "null")
                .unwrap_or("null")
                .to_owned(),
            Some(other) => panic!("an unknown type: {other}"),
            None if o.contains_key("properties") => "object".into(),
            None => return json!({}),
        };
        match ty.as_str() {
            "object" => self.merge(json!({}), o),
            "array" => {
                let min = o.get("minItems").and_then(Value::as_u64).unwrap_or(0);
                let item = o.get("items").map(|i| self.of(i)).unwrap_or(json!({}));
                Value::Array((0..min).map(|_| item.clone()).collect())
            }
            "string" => self.string(o),
            "integer" => json!(number(o).max(0.0) as i64),
            "number" => json!(number(o)),
            "boolean" => json!(false),
            "null" => Value::Null,
            other => panic!("an unknown type: {other}"),
        }
    }

    /// `base` with every required property of `o` filled in.
    fn merge(&self, base: Value, o: &Map<String, Value>) -> Value {
        let Value::Object(mut out) = base else {
            return base;
        };
        let Some(properties) = o.get("properties").and_then(Value::as_object) else {
            return Value::Object(out);
        };
        let required: Vec<&str> = o
            .get("required")
            .and_then(Value::as_array)
            .map(|r| r.iter().filter_map(Value::as_str).collect())
            .unwrap_or_default();
        for name in required {
            if out.contains_key(name) {
                continue;
            }
            let schema = properties
                .get(name)
                .unwrap_or_else(|| panic!("required `{name}` has no schema"));
            out.insert(name.to_owned(), self.of(schema));
        }
        Value::Object(out)
    }

    fn string(&self, o: &Map<String, Value>) -> Value {
        match o.get("format").and_then(Value::as_str) {
            Some("uuid") => json!(uuid::Uuid::now_v7().to_string()),
            Some("date") => json!("2024-01-01"),
            Some("date-time") => {
                json!((chrono::Utc::now() + chrono::Duration::days(30)).to_rfc3339())
            }
            Some("uri") => json!("https://example.test/"),
            _ => {
                let min = o
                    .get("minLength")
                    .and_then(Value::as_u64)
                    .unwrap_or(1)
                    .max(1);
                json!("x".repeat(usize::try_from(min).unwrap_or(1)))
            }
        }
    }
}

fn number(o: &Map<String, Value>) -> f64 {
    o.get("minimum")
        .and_then(Value::as_f64)
        .or_else(|| {
            o.get("exclusiveMinimum")
                .and_then(Value::as_f64)
                .map(|m| m + 1.0)
        })
        .unwrap_or(1.0)
        .max(1.0)
}
