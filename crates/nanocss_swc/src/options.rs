use std::collections::BTreeMap;

use serde::Deserialize;

pub type HtmlDefaultStyle = BTreeMap<String, serde_json::Value>;
pub type HtmlDefaults = BTreeMap<String, HtmlDefaultStyle>;

#[derive(Clone, Debug, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct TransformOptions {
    pub debug: bool,
    pub import_sources: Vec<String>,
    pub input_source_map: Option<serde_json::Value>,
    pub html_defaults: HtmlDefaults,
    pub env: serde_json::Value,
}

impl Default for TransformOptions {
    fn default() -> Self {
        Self {
            debug: false,
            import_sources: vec!["nanocss-compiler".to_string()],
            input_source_map: None,
            html_defaults: BTreeMap::new(),
            env: serde_json::Value::Object(Default::default()),
        }
    }
}

impl TransformOptions {
    pub fn from_json(config: Option<String>) -> Self {
        let Some(config) = config else {
            return Self::default();
        };
        serde_json::from_str(&config)
            .unwrap_or_else(|error| panic!("[nanocss] Invalid transform options JSON: {error}."))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_debug_option() {
        let options = TransformOptions::from_json(Some(r#"{"debug":true}"#.to_string()));

        assert!(options.debug);
        assert_eq!(options.import_sources, vec!["nanocss-compiler"]);
    }

    #[test]
    fn parses_import_sources_option() {
        let options = TransformOptions::from_json(Some(
            r#"{"importSources":["@/lib/nanocss","nanocss-compiler"]}"#.to_string(),
        ));

        assert_eq!(
            options.import_sources,
            vec!["@/lib/nanocss", "nanocss-compiler"]
        );
    }

    #[test]
    fn parses_input_source_map_option() {
        let options = TransformOptions::from_json(Some(
            r#"{"inputSourceMap":{"version":3,"sources":[],"names":[],"mappings":""}}"#.to_string(),
        ));

        assert!(options.input_source_map.is_some());
    }

    #[test]
    fn parses_html_defaults_option() {
        let options = TransformOptions::from_json(Some(
            r#"{"htmlDefaults":{"div":{"boxSizing":"border-box","marginTop":0}}}"#.to_string(),
        ));

        assert_eq!(options.html_defaults.len(), 1);
        assert_eq!(
            options
                .html_defaults
                .get("div")
                .and_then(|properties| properties.get("boxSizing")),
            Some(&serde_json::Value::String("border-box".to_string()))
        );
    }

    #[test]
    fn parses_env_option() {
        let options = TransformOptions::from_json(Some(
            r##"{"env":{"colors":{"primary":"#123456"},"space":8}}"##.to_string(),
        ));

        assert_eq!(
            options.env.pointer("/colors/primary"),
            Some(&serde_json::Value::String("#123456".to_string()))
        );
    }

    #[test]
    fn defaults_debug_to_false() {
        let options = TransformOptions::from_json(None);
        assert!(!options.debug);
        assert!(options.html_defaults.is_empty());
    }

    #[test]
    #[should_panic(expected = "[nanocss] Invalid transform options JSON:")]
    fn rejects_invalid_json() {
        TransformOptions::from_json(Some("not json".to_string()));
    }
}
