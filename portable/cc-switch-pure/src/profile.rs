use crate::model_capabilities::{self as original, ImageInputCapability};
use serde::{
    Deserialize, Deserializer,
    de::{self, MapAccess, Visitor},
};
use serde_json::{Map, Value};
use std::fmt;

pub const HANDLER: &str = "cc-switch.image-capability.resolve";
pub const INPUT_TYPE: &str = "cc-switch.image-capability.request.v1";
pub const OUTPUT_TYPE: &str = "cc-switch.image-capability.result.v1";
pub const MAX_INPUT_BYTES: usize = 65536;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProfileError {
    InvalidInput,
    UnsupportedInput,
    ResourceLimit,
}

// Only the envelope forbids duplicate keys. The nested settings Value retains
// the pinned original serde_json behavior, including its field precedence.
struct Envelope(Map<String, Value>);
impl<'de> Deserialize<'de> for Envelope {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct EnvelopeVisitor;
        impl<'de> Visitor<'de> for EnvelopeVisitor {
            type Value = Envelope;
            fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
                f.write_str("a unique-key request object")
            }
            fn visit_map<M: MapAccess<'de>>(self, mut map: M) -> Result<Envelope, M::Error> {
                let mut fields = Map::new();
                while let Some((key, value)) = map.next_entry::<String, Value>()? {
                    if fields.insert(key, value).is_some() {
                        return Err(de::Error::custom("duplicate request key"));
                    }
                }
                Ok(Envelope(fields))
            }
        }
        deserializer.deserialize_map(EnvelopeVisitor)
    }
}

fn nullable_bool(value: &Value) -> Result<Option<bool>, ProfileError> {
    if value.is_null() {
        Ok(None)
    } else {
        value.as_bool().map(Some).ok_or(ProfileError::InvalidInput)
    }
}

/// Evaluate only bounded invocation data. Unknown is a successful three-state
/// result; malformed input and unsupported profiles are separate failures.
pub fn evaluate_json(input: &[u8]) -> Result<Vec<u8>, ProfileError> {
    if input.len() > MAX_INPUT_BYTES {
        return Err(ProfileError::ResourceLimit);
    }
    let Envelope(fields) = serde_json::from_slice(input).map_err(|_| ProfileError::InvalidInput)?;
    let required = |key: &str| fields.get(key).ok_or(ProfileError::InvalidInput);
    let version = required("version")?
        .as_u64()
        .ok_or(ProfileError::InvalidInput)?;
    if version != 1 {
        return Err(ProfileError::UnsupportedInput);
    }
    let method = required("method")?
        .as_str()
        .ok_or(ProfileError::InvalidInput)?;
    let model = required("model")?
        .as_str()
        .ok_or(ProfileError::InvalidInput)?;
    let allowed: &[&str] = match method {
        "resolve" => &[
            "version",
            "method",
            "model",
            "declared_support",
            "use_confirmed_registry",
        ],
        "from_settings" => &[
            "version",
            "method",
            "model",
            "settings",
            "use_confirmed_registry",
        ],
        "from_modalities" => &["version", "method", "model", "modalities"],
        _ => return Err(ProfileError::UnsupportedInput),
    };
    if fields.len() != allowed.len() || fields.keys().any(|key| !allowed.contains(&key.as_str())) {
        return Err(ProfileError::InvalidInput);
    }
    let capability = match method {
        "resolve" => original::resolve_image_input_capability(
            model,
            nullable_bool(required("declared_support")?)?,
            required("use_confirmed_registry")?
                .as_bool()
                .ok_or(ProfileError::InvalidInput)?,
        ),
        "from_settings" => original::image_input_capability_from_settings(
            required("settings")?,
            model,
            required("use_confirmed_registry")?
                .as_bool()
                .ok_or(ProfileError::InvalidInput)?,
        ),
        "from_modalities" => {
            let value = required("modalities")?;
            let modalities = if value.is_null() {
                None
            } else {
                Some(
                    value
                        .as_array()
                        .ok_or(ProfileError::InvalidInput)?
                        .iter()
                        .map(|item| {
                            item.as_str()
                                .map(String::from)
                                .ok_or(ProfileError::InvalidInput)
                        })
                        .collect::<Result<Vec<_>, _>>()?,
                )
            };
            original::image_input_capability_from_modalities(model, modalities.as_deref())
        }
        _ => unreachable!(),
    };
    let image_input = match capability {
        ImageInputCapability::Supported => "Supported",
        ImageInputCapability::Unsupported => "Unsupported",
        ImageInputCapability::Unknown => "Unknown",
    };
    Ok(format!("{{\"version\":1,\"image_input\":\"{image_input}\"}}").into_bytes())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    fn label(cap: ImageInputCapability) -> &'static str {
        match cap {
            ImageInputCapability::Supported => "Supported",
            ImageInputCapability::Unsupported => "Unsupported",
            ImageInputCapability::Unknown => "Unknown",
        }
    }
    fn result(value: Value) -> String {
        let output = evaluate_json(&serde_json::to_vec(&value).unwrap()).unwrap();
        assert!(output.len() <= 1024);
        serde_json::from_slice::<Value>(&output).unwrap()["image_input"]
            .as_str()
            .unwrap()
            .into()
    }
    #[test]
    fn resolve_matches_unmodified_original() {
        for model in [
            "gpt-5.4",
            "GLM-5.2[1M]",
            "models/GLM-5.2",
            "deepseek-v4-flash",
            "deepseek-v4-pro",
            "vendor/custom-vision",
            "",
            " glm-5.2 ",
        ] {
            for declared in [None, Some(false), Some(true)] {
                for registry in [false, true] {
                    assert_eq!(
                        result(
                            json!({"version":1,"method":"resolve","model":model,"declared_support":declared,"use_confirmed_registry":registry})
                        ),
                        label(original::resolve_image_input_capability(
                            model, declared, registry
                        ))
                    );
                }
            }
        }
    }
    #[test]
    fn modalities_missing_and_empty_match_original() {
        for modalities in [
            None,
            Some(vec![]),
            Some(vec!["text".into()]),
            Some(vec![" IMAGE ".into()]),
        ] {
            for model in ["GLM-5.2[1M]", "unknown"] {
                assert_eq!(
                    result(
                        json!({"version":1,"method":"from_modalities","model":model,"modalities":modalities})
                    ),
                    label(original::image_input_capability_from_modalities(
                        model,
                        modalities.as_deref()
                    ))
                );
            }
        }
    }
    #[test]
    fn settings_shapes_and_precedence_match_original() {
        for settings in [
            json!(null),
            json!([]),
            json!({"models":{"alias":{"vision":true}}}),
            json!({"modelCatalog":{"models":[{"model":"alias","supportsImage":false,"vision":true}]}}),
            json!({"modelCatalog":[{"id":"alias","inputModalities":["image"]}]}),
            json!({"models":[{"name":"alias","supportsImage":"invalid","vision":true,"modalities":{"input":["text"]}}]}),
        ] {
            for registry in [true, false] {
                assert_eq!(
                    result(
                        json!({"version":1,"method":"from_settings","model":"alias","settings":settings,"use_confirmed_registry":registry})
                    ),
                    label(original::image_input_capability_from_settings(
                        &settings, "alias", registry
                    ))
                );
            }
        }
    }
    #[test]
    fn unknown_is_not_transport_failure() {
        assert_eq!(
            result(
                json!({"version":1,"method":"resolve","model":"unknown","declared_support":null,"use_confirmed_registry":false})
            ),
            "Unknown"
        );
        assert_eq!(
            evaluate_json(br#"{"version":2,"method":"resolve","model":"unknown"}"#),
            Err(ProfileError::UnsupportedInput)
        );
    }
    #[test]
    fn malformed_envelopes_are_rejected() {
        for input in [b"[]".as_slice(), b"null", b"\xff", b"{}", br#"{"version":1,"version":1}"#,
            br#"{"version":1,"method":"resolve","model":"x","declared_support":null,"use_confirmed_registry":false,"extra":0}"#,
            br#"{"version":1,"method":"resolve","model":"x","declared_support":"true","use_confirmed_registry":false}"#,
            br#"{"version":1,"method":"from_modalities","model":"x","modalities":[1]}"#,
            br#"{"version":1,"method":"from_modalities","model":"x"}"#,
            br#"{"version":1,"method":"from_modalities","model":"x","modalities":null}{}"#] {
            assert_eq!(evaluate_json(input), Err(ProfileError::InvalidInput));
        }
    }
    #[test]
    fn bounded_input_limit_is_enforced() {
        assert_eq!(
            evaluate_json(&vec![b' '; MAX_INPUT_BYTES + 1]),
            Err(ProfileError::ResourceLimit)
        );
        let mut exact = serde_json::to_vec(
            &json!({"version":1,"method":"from_modalities","model":"x","modalities":null}),
        )
        .unwrap();
        exact.resize(MAX_INPUT_BYTES, b' ');
        assert!(evaluate_json(&exact).is_ok());
    }
}
