use std::collections::HashSet;

use crate::{
    keyframes::CompiledKeyframes,
    position_try::CompiledPositionTry,
    variables::{CompiledVariableDefault, CompiledVariableProperty},
    view_transition::CompiledViewTransitionClass,
};

pub(crate) fn create_style_sheet(
    hook_css: &str,
    keyframes: &[CompiledKeyframes],
    position_tries: &[CompiledPositionTry],
    view_transition_classes: &[CompiledViewTransitionClass],
    variable_properties: &[CompiledVariableProperty],
    variable_defaults: &[CompiledVariableDefault],
    debug: bool,
) -> String {
    let newline = if debug { "\n" } else { "" };
    let mut css = Vec::new();
    let mut seen_css = HashSet::new();

    if !hook_css.is_empty() {
        push_unique_css(&mut css, &mut seen_css, hook_css.to_string());
    }

    for keyframes in keyframes {
        push_unique_css(&mut css, &mut seen_css, keyframes.css.clone());
    }

    for position_try in position_tries {
        push_unique_css(&mut css, &mut seen_css, position_try.css.clone());
    }

    for view_transition_class in view_transition_classes {
        push_unique_css(&mut css, &mut seen_css, view_transition_class.css.clone());
    }

    for property in variable_properties {
        let (space, newline, indent) = if debug {
            (" ", "\n", "  ")
        } else {
            ("", "", "")
        };
        push_unique_css(
            &mut css,
            &mut seen_css,
            [
                format!("@property {}{space}{{", property.custom_property_name),
                format!("{indent}syntax:{space}\"{}\";", property.syntax),
                format!("{indent}inherits:{space}true;"),
                format!("{indent}initial-value:{space}{};", property.initial_value),
                "}".to_string(),
            ]
            .join(newline),
        );
    }

    if !variable_defaults.is_empty() {
        let (space, indent) = if debug { (" ", "  ") } else { ("", "") };
        let mut defaults = vec![format!("*{space}{{")];
        for default in variable_defaults {
            defaults.push(format!(
                "{indent}{}:{space}{};",
                default.custom_property_name, default.value
            ));
        }
        defaults.push("}".to_string());
        push_unique_css(&mut css, &mut seen_css, defaults.join(newline));
    }

    css.join(newline)
}

fn push_unique_css(css: &mut Vec<String>, seen_css: &mut HashSet<String>, chunk: String) {
    if seen_css.insert(chunk.clone()) {
        css.push(chunk);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn combines_keyframes_and_variable_defaults() {
        let style_sheet = create_style_sheet(
            "",
            &[CompiledKeyframes {
                name: "fade".to_string(),
                css: "@keyframes fade {}".to_string(),
            }],
            &[CompiledPositionTry {
                name: "--fallback".to_string(),
                css: "@position-try --fallback {}".to_string(),
            }],
            &[CompiledViewTransitionClass {
                name: "transition".to_string(),
                css: "::view-transition-new(*.transition) {}".to_string(),
            }],
            &[CompiledVariableProperty {
                custom_property_name: "--color".to_string(),
                syntax: "<color>",
                initial_value: "green".to_string(),
            }],
            &[CompiledVariableDefault {
                custom_property_name: "--color--nd".to_string(),
                value: "green".to_string(),
            }],
            true,
        );

        assert_eq!(
            style_sheet,
            "@keyframes fade {}\n@position-try --fallback {}\n::view-transition-new(*.transition) {}\n@property --color {\n  syntax: \"<color>\";\n  inherits: true;\n  initial-value: green;\n}\n* {\n  --color--nd: green;\n}"
        );
    }

    #[test]
    fn dedupes_identical_generated_css_chunks() {
        let style_sheet = create_style_sheet(
            "",
            &[
                CompiledKeyframes {
                    name: "fade".to_string(),
                    css: "@keyframes fade {}".to_string(),
                },
                CompiledKeyframes {
                    name: "fade".to_string(),
                    css: "@keyframes fade {}".to_string(),
                },
            ],
            &[],
            &[],
            &[],
            &[],
            true,
        );

        assert_eq!(style_sheet, "@keyframes fade {}");
    }
}
