//! Strict JSON parsing: duplicate object keys are refused at every depth.
use serde_json::Value;

pub fn parse(text: &str) -> std::result::Result<Value, serde_json::Error> {
    use serde::de::{self, Deserialize, Deserializer, MapAccess, SeqAccess, Visitor};

    struct Strict(Value);
    struct V;

    impl<'de> Visitor<'de> for V {
        type Value = Strict;

        fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.write_str("a JSON value")
        }
        fn visit_bool<E>(self, b: bool) -> std::result::Result<Strict, E> {
            Ok(Strict(Value::Bool(b)))
        }
        fn visit_i64<E>(self, n: i64) -> std::result::Result<Strict, E> {
            Ok(Strict(Value::from(n)))
        }
        fn visit_u64<E>(self, n: u64) -> std::result::Result<Strict, E> {
            Ok(Strict(Value::from(n)))
        }
        fn visit_f64<E>(self, n: f64) -> std::result::Result<Strict, E> {
            Ok(Strict(
                serde_json::Number::from_f64(n).map_or(Value::Null, Value::Number),
            ))
        }
        fn visit_str<E>(self, s: &str) -> std::result::Result<Strict, E> {
            Ok(Strict(Value::String(s.to_string())))
        }
        fn visit_string<E>(self, s: String) -> std::result::Result<Strict, E> {
            Ok(Strict(Value::String(s)))
        }
        fn visit_unit<E>(self) -> std::result::Result<Strict, E> {
            Ok(Strict(Value::Null))
        }
        fn visit_none<E>(self) -> std::result::Result<Strict, E> {
            Ok(Strict(Value::Null))
        }
        fn visit_some<D: Deserializer<'de>>(self, d: D) -> std::result::Result<Strict, D::Error> {
            Strict::deserialize(d)
        }
        fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> std::result::Result<Strict, A::Error> {
            let mut out = Vec::new();
            while let Some(Strict(x)) = seq.next_element()? {
                out.push(x);
            }
            Ok(Strict(Value::Array(out)))
        }
        fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> std::result::Result<Strict, A::Error> {
            let mut out = serde_json::Map::new();
            while let Some(k) = map.next_key::<String>()? {
                if out.contains_key(&k) {
                    return Err(de::Error::custom(format!("duplicate key {k:?}")));
                }
                let Strict(x) = map.next_value()?;
                out.insert(k, x);
            }
            Ok(Strict(Value::Object(out)))
        }
    }

    impl<'de> Deserialize<'de> for Strict {
        fn deserialize<D: Deserializer<'de>>(d: D) -> std::result::Result<Strict, D::Error> {
            d.deserialize_any(V)
        }
    }

    serde_json::from_str::<Strict>(text).map(|Strict(v)| v)
}
