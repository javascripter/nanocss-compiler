use std::collections::{HashMap, HashSet};

use swc_core::{
    common::DUMMY_SP,
    ecma::ast::{
        CallExpr, ComputedPropName, Expr, Lit, ObjectLit, Prop, PropName, PropOrSpread, Str,
    },
};

use crate::{
    ast::create_property_member_expr,
    define_consts::{ConstGroups, const_value_to_expression, resolve_constant_token},
    env::replace_env_references,
    generated_strings::GeneratedString,
    hooks::HookCompiler,
    styles::parse_hook_object_with_consts,
    variables::{
        CompiledVariableDefault, CompiledVariableProperty, VARIABLE_DEFAULTS_PROPERTY_NAME,
        compile_define_vars, create_variable_default_name, parse_create_theme_overrides_arg,
        parse_define_vars_arg,
    },
};

pub(crate) struct DefineVarsReplacement {
    pub group_name: String,
    pub group: HashMap<String, String>,
    pub defaults: Vec<CompiledVariableDefault>,
    pub properties: Vec<CompiledVariableProperty>,
    pub init: ObjectLit,
}

pub(crate) fn compile_define_vars_declaration(
    group_name: String,
    call: &CallExpr,
    css_names: &HashSet<String>,
    file_identity: &str,
    variable_groups: &HashMap<String, HashMap<String, String>>,
    imported_variable_group_names: &HashSet<String>,
    const_groups: &ConstGroups,
    generated_string_names: &HashMap<String, GeneratedString>,
    hook_compiler: &mut HookCompiler,
    env: &serde_json::Value,
    debug: bool,
) -> DefineVarsReplacement {
    if call.args.len() != 1 || call.args[0].spread.is_some() {
        panic!("[nanocss] css.defineVars(...) must be called with a static object expression.");
    }

    let mut tokens_arg = (*call.args[0].expr).clone();
    replace_env_references(&mut tokens_arg, css_names, env);
    let tokens = parse_define_vars_arg(
        &group_name,
        &tokens_arg,
        css_names,
        file_identity,
        variable_groups,
        imported_variable_group_names,
        const_groups,
        generated_string_names,
        debug,
    );
    let compiled = compile_define_vars(&group_name, &tokens, file_identity, hook_compiler, debug);
    let mut group = HashMap::new();
    let mut defaults = Vec::new();
    let mut variable_properties = Vec::new();
    let mut properties = Vec::new();
    let mut default_properties = Vec::new();

    for token in compiled {
        let token_name = token.token_name;
        let custom_property_name = token.custom_property_name;
        group.insert(token_name.clone(), custom_property_name.clone());
        defaults.extend(token.defaults);
        if let Some(property) = token.property {
            variable_properties.push(property);
        }
        properties.push(PropOrSpread::Prop(Box::new(Prop::KeyValue(
            swc_core::ecma::ast::KeyValueProp {
                key: PropName::Str(Str {
                    span: DUMMY_SP,
                    value: token_name.into(),
                    raw: None,
                }),
                value: Box::new(Expr::Lit(Lit::Str(Str {
                    span: DUMMY_SP,
                    value: custom_property_name.clone().into(),
                    raw: None,
                }))),
            },
        ))));
        default_properties.push(PropOrSpread::Prop(Box::new(Prop::KeyValue(
            swc_core::ecma::ast::KeyValueProp {
                key: PropName::Str(Str {
                    span: DUMMY_SP,
                    value: custom_property_name.clone().into(),
                    raw: None,
                }),
                value: Box::new(Expr::Lit(Lit::Str(Str {
                    span: DUMMY_SP,
                    value: create_variable_default_value(&custom_property_name, debug).into(),
                    raw: None,
                }))),
            },
        ))));
    }
    properties.push(PropOrSpread::Prop(Box::new(Prop::KeyValue(
        swc_core::ecma::ast::KeyValueProp {
            key: PropName::Str(Str {
                span: DUMMY_SP,
                value: VARIABLE_DEFAULTS_PROPERTY_NAME.into(),
                raw: None,
            }),
            value: Box::new(Expr::Object(ObjectLit {
                span: DUMMY_SP,
                props: default_properties,
            })),
        },
    ))));

    DefineVarsReplacement {
        group_name,
        group,
        defaults,
        properties: variable_properties,
        init: ObjectLit {
            span: DUMMY_SP,
            props: properties,
        },
    }
}

pub(crate) fn compile_create_theme_call(
    group_arg: &Expr,
    call: &CallExpr,
    css_names: &HashSet<String>,
    local_group: Option<&HashMap<String, String>>,
    const_groups: &ConstGroups,
    hook_compiler: &mut HookCompiler,
    env: &serde_json::Value,
) -> ObjectLit {
    if call.args.len() != 2 || call.args.iter().any(|arg| arg.spread.is_some()) {
        panic!("[nanocss] css.createTheme(...) overrides must be a static object expression.");
    }

    let mut overrides_arg = (*call.args[1].expr).clone();
    replace_env_references(&mut overrides_arg, css_names, env);
    let overrides = parse_create_theme_overrides_arg(&overrides_arg, css_names);
    let mut properties = vec![PropOrSpread::Spread(swc_core::ecma::ast::SpreadElement {
        dot3_token: DUMMY_SP,
        expr: Box::new(create_property_member_expr(
            (*group_arg).clone(),
            VARIABLE_DEFAULTS_PROPERTY_NAME,
        )),
    })];

    for (token_name, value) in overrides {
        let (key, property_name) = if let Some(group) = local_group {
            let Some(custom_property_name) = group.get(&token_name) else {
                panic!(
                    "[nanocss] \"{}\" is not defined in the variable group passed to css.createTheme(...).",
                    token_name
                );
            };
            (
                PropName::Str(Str {
                    span: DUMMY_SP,
                    value: custom_property_name.clone().into(),
                    raw: None,
                }),
                custom_property_name.clone(),
            )
        } else {
            (
                PropName::Computed(ComputedPropName {
                    span: DUMMY_SP,
                    expr: Box::new(create_property_member_expr(group_arg.clone(), &token_name)),
                }),
                "--".to_string(),
            )
        };
        let value = resolve_constant_token(&value, const_groups)
            .map(|value| const_value_to_expression(&value))
            .unwrap_or(value);
        let value = compile_theme_value(&property_name, value, const_groups, hook_compiler);

        properties.push(PropOrSpread::Prop(Box::new(Prop::KeyValue(
            swc_core::ecma::ast::KeyValueProp {
                key,
                value: Box::new(value),
            },
        ))));
    }

    ObjectLit {
        span: DUMMY_SP,
        props: properties,
    }
}

fn compile_theme_value(
    property_name: &str,
    value: Expr,
    const_groups: &ConstGroups,
    hook_compiler: &mut HookCompiler,
) -> Expr {
    let Expr::Object(object) = &value else {
        return value;
    };
    let Some(hook_value) = parse_hook_object_with_consts(object, const_groups) else {
        return value;
    };
    let Some((_, compiled_value)) = hook_compiler
        .compile_property(property_name, &hook_value)
        .into_iter()
        .next()
    else {
        return value;
    };
    Expr::Lit(Lit::Str(Str {
        span: DUMMY_SP,
        value: compiled_value.into(),
        raw: None,
    }))
}

fn create_variable_default_value(custom_property_name: &str, debug: bool) -> String {
    format!(
        "var({})",
        create_variable_default_name(custom_property_name, debug)
    )
}
