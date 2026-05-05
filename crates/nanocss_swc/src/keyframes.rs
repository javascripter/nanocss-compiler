use crate::{
    ast::{css_property_name, format_number},
    constants::is_unitless_number,
    hash::hash,
};

#[derive(Debug, Clone, PartialEq)]
pub(crate) enum KeyframesValue {
    String(String),
    Number(f64),
}

pub(crate) type KeyframesFrames = Vec<(String, Vec<(String, KeyframesValue)>)>;

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct CompiledKeyframes {
    pub name: String,
    pub css: String,
}

fn stringify_style_value(property_name: &str, value: &KeyframesValue) -> String {
    match value {
        KeyframesValue::String(value) => value.clone(),
        KeyframesValue::Number(value) => {
            let mut formatted = format_number(*value);
            if !is_unitless_number(property_name) {
                formatted.push_str("px");
            }
            formatted
        }
    }
}

fn json_quote(value: &str) -> String {
    let mut quoted = String::from("\"");
    for character in value.chars() {
        match character {
            '"' => quoted.push_str("\\\""),
            '\\' => quoted.push_str("\\\\"),
            '\n' => quoted.push_str("\\n"),
            '\r' => quoted.push_str("\\r"),
            '\t' => quoted.push_str("\\t"),
            character => quoted.push(character),
        }
    }
    quoted.push('"');
    quoted
}

fn json_keyframes(frames: &KeyframesFrames) -> String {
    let mut json = String::from("{");
    for (frame_index, (frame_name, frame)) in frames.iter().enumerate() {
        if frame_index > 0 {
            json.push(',');
        }
        json.push_str(&json_quote(frame_name));
        json.push_str(":{");
        for (property_index, (property_name, value)) in frame.iter().enumerate() {
            if property_index > 0 {
                json.push(',');
            }
            json.push_str(&json_quote(&css_property_name(property_name)));
            json.push(':');
            match value {
                KeyframesValue::String(value) => json.push_str(&json_quote(value)),
                KeyframesValue::Number(value) => json.push_str(&format_number(*value)),
            }
        }
        json.push('}');
    }
    json.push('}');
    json
}

pub(crate) fn compile_keyframes(frames: &KeyframesFrames, debug: bool) -> CompiledKeyframes {
    let prefix = if debug { "__nanocss_keyframes-" } else { "nk-" };
    let name = format!("{}{}", prefix, hash(&json_keyframes(frames)));
    let (space, newline) = if debug { (" ", "\n") } else { ("", "") };
    let indent = format!("{space}{space}");
    let mut css = vec![format!("@keyframes {name}{space}{{")];

    for (frame_name, frame) in frames {
        css.push(format!("{indent}{frame_name}{space}{{"));
        for (property_name, value) in frame {
            let property_name = css_property_name(property_name);
            css.push(format!(
                "{indent}{indent}{property_name}:{space}{};",
                stringify_style_value(&property_name, value)
            ));
        }
        css.push(format!("{indent}}}"));
    }
    css.push("}".to_string());

    CompiledKeyframes {
        name,
        css: css.join(newline),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compiles_keyframes_with_matching_hash() {
        let compiled = compile_keyframes(
            &vec![
                (
                    "0%".to_string(),
                    vec![("opacity".to_string(), KeyframesValue::Number(0.0))],
                ),
                (
                    "100%".to_string(),
                    vec![("opacity".to_string(), KeyframesValue::Number(1.0))],
                ),
            ],
            true,
        );

        assert_eq!(compiled.name, "__nanocss_keyframes-1ii5yk");
        assert_eq!(
            compiled.css,
            "@keyframes __nanocss_keyframes-1ii5yk {\n  0% {\n    opacity: 0;\n  }\n  100% {\n    opacity: 1;\n  }\n}"
        );
    }

    #[test]
    fn adds_px_to_length_values() {
        assert_eq!(
            stringify_style_value("width", &KeyframesValue::Number(10.0)),
            "10px"
        );
        assert_eq!(
            stringify_style_value("opacity", &KeyframesValue::Number(1.0)),
            "1"
        );
    }

    #[test]
    fn hyphenates_camel_case_keyframe_properties() {
        let compiled = compile_keyframes(
            &vec![(
                "from".to_string(),
                vec![
                    (
                        "backgroundColor".to_string(),
                        KeyframesValue::String("red".to_string()),
                    ),
                    (
                        "WebkitTransform".to_string(),
                        KeyframesValue::String("none".to_string()),
                    ),
                    (
                        "msTransform".to_string(),
                        KeyframesValue::String("none".to_string()),
                    ),
                    ("--progress".to_string(), KeyframesValue::Number(1.0)),
                ],
            )],
            true,
        );

        assert!(compiled.css.contains("background-color: red;"));
        assert!(compiled.css.contains("-webkit-transform: none;"));
        assert!(compiled.css.contains("-ms-transform: none;"));
        assert!(compiled.css.contains("--progress: 1;"));
        assert!(!compiled.css.contains("backgroundColor"));
    }

    #[test]
    #[should_panic(expected = "[nanocss] Numeric CSS values must be finite.")]
    fn rejects_non_finite_keyframe_values() {
        compile_keyframes(
            &vec![(
                "from".to_string(),
                vec![("opacity".to_string(), KeyframesValue::Number(f64::INFINITY))],
            )],
            true,
        );
    }
}
