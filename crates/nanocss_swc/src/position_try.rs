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
pub(crate) struct CompiledPositionTry {
    pub name: String,
    pub css: String,
}

pub(crate) fn compile_position_try(
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
) -> CompiledPositionTry {
    if call.args.len() != 1 || call.args[0].spread.is_some() {
        panic!("[nanocss] css.positionTry(...) must be called with a static object expression.");
    }
    let Expr::Object(object) = &*call.args[0].expr else {
        panic!("[nanocss] css.positionTry(...) must be called with a static object expression.");
    };
    if object.props.is_empty() {
        panic!("[nanocss] css.positionTry(...) must define at least one descriptor.");
    }

    for property in &object.props {
        let PropOrSpread::Prop(property) = property else {
            panic!("[nanocss] css.positionTry(...) objects cannot contain spreads.");
        };
        let Prop::KeyValue(property) = &**property else {
            panic!("[nanocss] css.positionTry(...) values must be expressions.");
        };
        let Some(property_name) = prop_name_to_string(&property.key) else {
            panic!("[nanocss] css.positionTry(...) descriptor keys must be statically known.");
        };
        if !is_allowed_position_try_property(&property_name) {
            panic!(
                "[nanocss] css.positionTry(...) only supports positionAnchor, positionArea, inset, margin, size, and self-alignment descriptors."
            );
        }
    }

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
        api_name: "css.positionTry(...)",
        allow_shorthand_properties: true,
    };
    let declarations = compile_static_style_object_to_css_declarations(object, &mut context);

    let prefix = if debug {
        "--_nanocss_position_try-"
    } else {
        "--npt-"
    };
    let name = format!("{}{}", prefix, hash(&json_declarations(&declarations)));
    let css = create_position_try_css(&name, &declarations, debug);

    CompiledPositionTry { name, css }
}

fn is_allowed_position_try_property(property_name: &str) -> bool {
    // Keep aligned with CSSPositionTryDescriptors:
    // https://drafts.csswg.org/css-anchor-position-1/#the-csspositiontryrule-interface
    matches!(
        property_name,
        "positionAnchor"
            | "positionArea"
            | "top"
            | "right"
            | "bottom"
            | "left"
            | "inset"
            | "insetBlock"
            | "insetBlockStart"
            | "insetBlockEnd"
            | "insetInline"
            | "insetInlineStart"
            | "insetInlineEnd"
            | "margin"
            | "marginTop"
            | "marginRight"
            | "marginBottom"
            | "marginLeft"
            | "marginBlock"
            | "marginBlockStart"
            | "marginBlockEnd"
            | "marginInline"
            | "marginInlineStart"
            | "marginInlineEnd"
            | "width"
            | "minWidth"
            | "maxWidth"
            | "height"
            | "minHeight"
            | "maxHeight"
            | "blockSize"
            | "minBlockSize"
            | "maxBlockSize"
            | "inlineSize"
            | "minInlineSize"
            | "maxInlineSize"
            | "alignSelf"
            | "justifySelf"
            | "placeSelf"
    )
}

fn create_position_try_css(name: &str, declarations: &[(String, String)], debug: bool) -> String {
    let (space, newline) = if debug { (" ", "\n") } else { ("", "") };
    let indent = format!("{space}{space}");
    let mut css = vec![format!("@position-try {name}{space}{{")];
    for (property_name, value) in declarations {
        css.push(format!("{indent}{property_name}:{space}{value};"));
    }
    css.push("}".to_string());
    css.join(newline)
}

fn json_declarations(declarations: &[(String, String)]) -> String {
    let mut json = String::from("{");
    for (index, (property_name, value)) in declarations.iter().enumerate() {
        if index > 0 {
            json.push(',');
        }
        json.push_str(&json_quote(property_name));
        json.push(':');
        json.push_str(&json_quote(value));
    }
    json.push('}');
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
