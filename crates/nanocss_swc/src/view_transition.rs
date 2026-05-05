use std::collections::{HashMap, HashSet};

use swc_core::ecma::ast::{CallExpr, Expr, Prop, PropOrSpread};

use crate::{
    ast::prop_name_to_string,
    define_consts::ConstGroups,
    generated_strings::GeneratedString,
    hash::hash,
    hooks::HookCompiler,
    styles::{StaticCssCompileContext, compile_static_style_object_to_css_declarations},
};

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct CompiledViewTransitionClass {
    pub name: String,
    pub css: String,
}

pub(crate) fn compile_view_transition_class(
    call: &CallExpr,
    css_names: &HashSet<String>,
    variable_groups: &HashMap<String, HashMap<String, String>>,
    imported_variable_group_names: &HashSet<String>,
    const_groups: &ConstGroups,
    generated_string_names: &HashMap<String, GeneratedString>,
    hook_compiler: &mut HookCompiler,
    file_identity: &str,
    dynamic_hook_id: &mut usize,
    debug: bool,
    env: &serde_json::Value,
) -> CompiledViewTransitionClass {
    if call.args.len() != 1 || call.args[0].spread.is_some() {
        panic!(
            "[nanocss] css.viewTransitionClass(...) must be called with a static object expression."
        );
    }
    let Expr::Object(options) = &*call.args[0].expr else {
        panic!(
            "[nanocss] css.viewTransitionClass(...) must be called with a static object expression."
        );
    };
    if options.props.is_empty() {
        panic!("[nanocss] css.viewTransitionClass(...) must define at least one section.");
    }

    let mut sections = Vec::new();
    for property in &options.props {
        let PropOrSpread::Prop(property) = property else {
            panic!("[nanocss] css.viewTransitionClass(...) objects cannot contain spreads.");
        };
        let Prop::KeyValue(property) = &**property else {
            panic!("[nanocss] css.viewTransitionClass(...) values must be expressions.");
        };
        let Some(section_name) = prop_name_to_string(&property.key) else {
            panic!("[nanocss] css.viewTransitionClass(...) keys must be statically known.");
        };
        let pseudo_element = view_transition_pseudo_element(&section_name);
        let Expr::Object(style) = &*property.value else {
            panic!(
                "[nanocss] css.viewTransitionClass(...) section values must be static style object expressions."
            );
        };

        let mut context = StaticCssCompileContext {
            css_names,
            variable_groups,
            imported_variable_group_names,
            const_groups,
            generated_string_names,
            hook_compiler,
            file_identity,
            dynamic_hook_id,
            debug,
            env,
            api_name: "css.viewTransitionClass(...)",
            allow_shorthand_properties: false,
        };
        let declarations = compile_static_style_object_to_css_declarations(style, &mut context);
        sections.push((section_name, pseudo_element, declarations));
    }

    let prefix = if debug {
        "__nanocss_view_transition-"
    } else {
        "nvt-"
    };
    let name = format!("{}{}", prefix, hash(&json_sections(&sections)));
    let css = create_view_transition_css(&name, &sections, debug);

    CompiledViewTransitionClass { name, css }
}

fn view_transition_pseudo_element(section_name: &str) -> &'static str {
    // Keep aligned with the view-transition-class pseudo-element set:
    // https://developer.mozilla.org/docs/Web/CSS/view-transition-class
    match section_name {
        "group" => "::view-transition-group",
        "imagePair" => "::view-transition-image-pair",
        "old" => "::view-transition-old",
        "new" => "::view-transition-new",
        _ => panic!(
            "[nanocss] css.viewTransitionClass(...) only supports group, imagePair, old, and new sections."
        ),
    }
}

fn create_view_transition_css(
    name: &str,
    sections: &[(String, &'static str, Vec<(String, String)>)],
    debug: bool,
) -> String {
    let (space, newline) = if debug { (" ", "\n") } else { ("", "") };
    let indent = format!("{space}{space}");
    let mut css = Vec::new();

    for (_, pseudo_element, declarations) in sections {
        css.push(format!("{pseudo_element}(*.{name}){space}{{"));
        for (property_name, value) in declarations {
            css.push(format!("{indent}{property_name}:{space}{value};"));
        }
        css.push("}".to_string());
    }

    css.join(newline)
}

fn json_sections(sections: &[(String, &'static str, Vec<(String, String)>)]) -> String {
    let mut json = String::from("[");
    for (section_index, (section_name, _, declarations)) in sections.iter().enumerate() {
        if section_index > 0 {
            json.push(',');
        }
        json.push('[');
        json.push_str(&json_quote(section_name));
        json.push_str(",{");
        for (declaration_index, (property_name, value)) in declarations.iter().enumerate() {
            if declaration_index > 0 {
                json.push(',');
            }
            json.push_str(&json_quote(property_name));
            json.push(':');
            json.push_str(&json_quote(value));
        }
        json.push_str("}]");
    }
    json.push(']');
    json
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
