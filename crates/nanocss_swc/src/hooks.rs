use std::collections::HashMap;

use crate::{ast::format_number, constants::is_unitless_number, hash::hash};

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum HookValue {
    String(String),
    Number(f64),
    Boolean(bool),
    Null,
    Dynamic(String),
    Object(Vec<(String, HookValue)>),
}

pub(crate) fn is_hook_name(name: &str) -> bool {
    name.starts_with('@') || name.starts_with(':') || name.starts_with('[')
}

pub(crate) fn is_hook_key(name: &str) -> bool {
    name == "default" || is_hook_name(name)
}

pub(crate) fn valid_hook_name_description() -> &'static str {
    "Hooks must be default, or start with \"@\", \":\", or \"[\"."
}

#[derive(Clone)]
enum ConditionTree {
    Hook(String),
    And(Box<ConditionTree>, Box<ConditionTree>),
}

pub(crate) struct HookCompiler {
    debug: bool,
    hooks: Vec<String>,
    hook_conditions: HashMap<String, String>,
    condition_styles: HashMap<String, (usize, String)>,
    next_condition_style_index: usize,
}

impl HookCompiler {
    pub fn new(debug: bool) -> Self {
        Self {
            debug,
            hooks: Vec::new(),
            hook_conditions: HashMap::new(),
            condition_styles: HashMap::new(),
            next_condition_style_index: 0,
        }
    }

    pub fn compile_property(
        &mut self,
        property_name: &str,
        value: &HookValue,
    ) -> Vec<(String, String)> {
        let mut base_style = None;
        let mut conditional_styles = Vec::new();
        collect_styles(
            property_name,
            value,
            &mut Vec::new(),
            &mut base_style,
            &mut conditional_styles,
        );

        let mut style = HashMap::new();
        if let Some(value) = base_style {
            style.insert(property_name.to_string(), value);
        }

        for (conditions, value) in conditional_styles {
            for condition in &conditions {
                self.add_hook(condition);
            }

            let id = self.condition_id(&conditions);
            let fallback = style
                .get(property_name)
                .and_then(|value| stringify_compiled_style_value(property_name, value, None))
                .unwrap_or_else(|| "revert-layer".to_string());
            let Some(value) =
                stringify_compiled_style_value(property_name, &value, Some(&fallback))
            else {
                continue;
            };
            let (space, _) = self.formatting();
            style.insert(
                property_name.to_string(),
                HookValue::String(format!(
                    "var(--{id}-1,{space}{value}){space}var(--{id}-0,{space}{fallback})"
                )),
            );
        }

        style
            .into_iter()
            .filter_map(|(property_name, value)| match value {
                HookValue::String(value) => Some((property_name, value)),
                HookValue::Dynamic(name) => Some((property_name, format!("var({name})"))),
                HookValue::Number(value) => Some((
                    property_name.clone(),
                    stringify_style_value(&property_name, value),
                )),
                HookValue::Boolean(value) => Some((property_name, value.to_string())),
                HookValue::Null | HookValue::Object(_) => None,
            })
            .collect()
    }

    pub fn style_sheet(&self) -> String {
        if self.hooks.is_empty() && self.condition_styles.is_empty() {
            return String::new();
        }

        let (space, newline) = self.formatting();
        let indent = format!("{space}{space}");
        let mut sheet = format!("*{space}{{{newline}");

        for hook in &self.hooks {
            sheet.push_str(&self.variable_pair(&self.hook_name_to_id(hook), 0, 1));
        }
        let mut condition_styles = self
            .condition_styles
            .iter()
            .map(|(property, (index, value))| (*index, property, value))
            .collect::<Vec<_>>();
        condition_styles.sort_by_key(|(index, _, _)| *index);
        for (_, property, value) in condition_styles {
            sheet.push_str(&format!("{indent}{property}:{space}{value};{newline}"));
        }

        sheet.push_str(&format!("}}{newline}"));

        for hook in &self.hooks {
            let condition = &self.hook_conditions[hook];
            let id = self.hook_name_to_id(hook);
            if condition.starts_with('@') {
                sheet.push_str(&format!(
                    "{condition}{space}{{{newline}{indent}*{space}{{{newline}{}{indent}}}{newline}}}{newline}",
                    self.variable_pair(&id, 1, 2)
                ));
            } else {
                sheet.push_str(&format!(
                    "{}{space}{{{newline}{}{}}}{newline}",
                    condition.replace('&', "*"),
                    self.variable_pair(&id, 1, 1),
                    ""
                ));
            }
        }

        sheet
    }

    fn add_hook(&mut self, hook: &str) {
        if self.hook_conditions.contains_key(hook) {
            return;
        }
        self.hooks.push(hook.to_string());
        let condition = if hook.starts_with(':') || hook.starts_with('[') {
            format!("&{hook}")
        } else {
            hook.to_string()
        };
        self.hook_conditions.insert(hook.to_string(), condition);
    }

    fn hook_name_to_id(&self, hook_name: &str) -> String {
        let spec_hash = hash_json_string(&self.hook_conditions[hook_name]);
        if self.debug {
            format!("{}-{spec_hash}", sanitize_hook_name(hook_name))
        } else {
            spec_hash
        }
    }

    fn condition_id(&mut self, conditions: &[String]) -> String {
        let condition = condition_tree(conditions);
        match &condition {
            ConditionTree::Hook(hook) => self.hook_name_to_id(hook),
            _ => {
                let name = format!(
                    "{}{}",
                    if self.debug { "cond-" } else { "c-" },
                    hash_json_string(&condition_to_id(self, &condition))
                );
                self.create_condition_vars(&name, &condition);
                name
            }
        }
    }

    fn create_condition_vars(&mut self, name: &str, condition: &ConditionTree) -> String {
        let ConditionTree::And(a, b) = condition else {
            return match condition {
                ConditionTree::Hook(hook) => self.hook_name_to_id(hook),
                _ => unreachable!(),
            };
        };

        let a = self.create_condition_vars(&format!("{name}A"), a);
        let b = self.create_condition_vars(&format!("{name}B"), b);
        let (space, _) = self.formatting();
        self.set_condition_style(
            format!("--{name}-0"),
            format!("var(--{a}-0){space}var(--{b}-0)"),
        );
        self.set_condition_style(
            format!("--{name}-1"),
            format!("var(--{a}-1,{space}var(--{b}-1))"),
        );
        name.to_string()
    }

    fn set_condition_style(&mut self, property: String, value: String) {
        let index = self.next_condition_style_index;
        self.next_condition_style_index += 1;
        self.condition_styles.insert(property, (index, value));
    }

    fn variable_pair(&self, id: &str, initial: usize, indents: usize) -> String {
        let (space, newline) = self.formatting();
        let indent = format!("{space}{space}").repeat(indents);
        [0, 1]
            .map(|index| {
                let value = if initial == index {
                    "initial"
                } else if space.is_empty() {
                    " "
                } else {
                    ""
                };
                format!("{indent}--{id}-{index}:{space}{value};{newline}")
            })
            .join("")
    }

    fn formatting(&self) -> (&'static str, &'static str) {
        if self.debug { (" ", "\n") } else { ("", "") }
    }
}

fn collect_styles(
    property_name: &str,
    value: &HookValue,
    conditions: &mut Vec<String>,
    base_style: &mut Option<HookValue>,
    conditional_styles: &mut Vec<(Vec<String>, HookValue)>,
) {
    match value {
        HookValue::Object(entries) => {
            for (condition, value) in entries {
                if condition == "default" {
                    collect_styles(
                        property_name,
                        value,
                        conditions,
                        base_style,
                        conditional_styles,
                    );
                } else {
                    conditions.push(condition.clone());
                    collect_styles(
                        property_name,
                        value,
                        conditions,
                        base_style,
                        conditional_styles,
                    );
                    conditions.pop();
                }
            }
        }
        value if conditions.is_empty() => {
            *base_style = Some(value.clone());
        }
        value => conditional_styles.push((conditions.clone(), value.clone())),
    }
    let _ = property_name;
}

fn condition_tree(conditions: &[String]) -> ConditionTree {
    let (head, tail) = conditions
        .split_first()
        .expect("conditions must not be empty");
    if tail.is_empty() {
        return ConditionTree::Hook(head.clone());
    }
    ConditionTree::And(
        Box::new(ConditionTree::Hook(head.clone())),
        Box::new(condition_tree(tail)),
    )
}

fn condition_to_id(compiler: &HookCompiler, condition: &ConditionTree) -> String {
    match condition {
        ConditionTree::Hook(hook) => compiler.hook_name_to_id(hook),
        ConditionTree::And(a, b) => {
            format!(
                "_{}-and-{}_",
                condition_to_id(compiler, a),
                condition_to_id(compiler, b)
            )
        }
    }
}

fn sanitize_hook_name(hook_name: &str) -> String {
    hook_name
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() || character == '-' {
                character
            } else {
                '_'
            }
        })
        .collect()
}

fn hash_json_string(value: &str) -> String {
    hash(&format!("{value:?}"))
}

fn stringify_compiled_style_value(
    property_name: &str,
    value: &HookValue,
    fallback: Option<&str>,
) -> Option<String> {
    match value {
        HookValue::String(value) => Some(value.clone()),
        HookValue::Number(value) => Some(stringify_style_value(property_name, *value)),
        HookValue::Boolean(value) => Some(value.to_string()),
        HookValue::Null => Some("revert-layer".to_string()),
        HookValue::Dynamic(name) => Some(
            fallback
                .map(|fallback| format!("var({name}, {fallback})"))
                .unwrap_or_else(|| format!("var({name})")),
        ),
        HookValue::Object(_) => fallback.map(ToString::to_string),
    }
}

fn stringify_style_value(property_name: &str, value: f64) -> String {
    let mut formatted = format_number(value);
    if !is_unitless_number(property_name) {
        formatted.push_str("px");
    }
    formatted
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recognizes_supported_hook_names() {
        for hook in [
            ":first-child",
            ":last-child",
            ":only-child",
            ":nth-child(2n + 1)",
            ":nth-of-type(2)",
            ":nth-last-child(2)",
            ":empty",
            ":hover",
            ":focus",
            ":focus-visible",
            ":focus-within",
            ":active",
            ":visited",
            ":disabled",
            ":is([data-disabled])",
            ":has(> img)",
            "[data-disabled]",
            "@layer components",
            "@media (min-width: 768px)",
            "@container card (min-width: 300px)",
            "@supports (display: grid)",
            "@starting-style",
        ] {
            assert!(is_hook_name(hook), "{hook} should be supported");
        }
    }

    #[test]
    fn rejects_unsupported_hook_names() {
        for hook in ["&[data-disabled]", ".dark &", "hover"] {
            assert!(!is_hook_name(hook), "{hook} should be unsupported");
        }
    }

    #[test]
    fn compiles_hover_values_and_stylesheet() {
        let mut compiler = HookCompiler::new(true);
        let compiled = compiler.compile_property(
            "color",
            &HookValue::Object(vec![
                (
                    "default".to_string(),
                    HookValue::String("black".to_string()),
                ),
                (":hover".to_string(), HookValue::String("red".to_string())),
            ]),
        );

        assert_eq!(
            compiled,
            vec![(
                "color".to_string(),
                "var(--_hover-mbscpo-1, red) var(--_hover-mbscpo-0, black)".to_string()
            )]
        );
        assert!(compiler.style_sheet().contains("*:hover"));
    }

    #[test]
    fn compiles_media_and_nested_conditions() {
        let mut compiler = HookCompiler::new(true);
        let compiled = compiler.compile_property(
            "backgroundColor",
            &HookValue::Object(vec![
                (
                    "default".to_string(),
                    HookValue::String("white".to_string()),
                ),
                (
                    ":hover".to_string(),
                    HookValue::Object(vec![
                        ("default".to_string(), HookValue::String("gray".to_string())),
                        (
                            "@media (min-width: 768px)".to_string(),
                            HookValue::String("blue".to_string()),
                        ),
                    ]),
                ),
            ]),
        );

        assert_eq!(compiled.len(), 1);
        assert!(compiled[0].1.contains("var(--cond-"));
        let style_sheet = compiler.style_sheet();
        assert!(style_sheet.contains("*:hover"));
        assert!(style_sheet.contains("@media (min-width: 768px)"));
        assert!(style_sheet.contains("--cond-"));
    }

    #[test]
    fn dedupes_nested_condition_vars() {
        let mut compiler = HookCompiler::new(true);
        let value = HookValue::Object(vec![
            (
                "default".to_string(),
                HookValue::String("white".to_string()),
            ),
            (
                ":hover".to_string(),
                HookValue::Object(vec![
                    ("default".to_string(), HookValue::String("gray".to_string())),
                    (
                        "@media (min-width: 768px)".to_string(),
                        HookValue::String("blue".to_string()),
                    ),
                ]),
            ),
        ]);

        compiler.compile_property("color", &value);
        compiler.compile_property("backgroundColor", &value);

        let style_sheet = compiler.style_sheet();
        assert_eq!(style_sheet.matches("--cond-27myt-0:").count(), 1);
        assert_eq!(style_sheet.matches("--cond-27myt-1:").count(), 1);
    }

    #[test]
    #[should_panic(expected = "[nanocss] Numeric CSS values must be finite.")]
    fn rejects_non_finite_hook_values() {
        let mut compiler = HookCompiler::new(true);
        compiler.compile_property("width", &HookValue::Number(f64::NEG_INFINITY));
    }
}
