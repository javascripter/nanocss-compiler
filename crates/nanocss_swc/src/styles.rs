use std::collections::{HashMap, HashSet};

use swc_core::{
    common::DUMMY_SP,
    ecma::ast::{
        ArrowExpr, BinExpr, BinaryOp, BlockStmtOrExpr, CallExpr, ComputedPropName, CondExpr, Expr,
        Lit, MemberExpr, MemberProp, ObjectLit, Pat, Prop, PropName, PropOrSpread, Str, UnaryExpr,
        UnaryOp,
    },
};

use crate::{
    ast::{css_property_name, format_number, is_css_member_call, prop_name_to_string},
    constants::{is_shorthand_property, is_unitless_number},
    define_consts::{
        ConstGroups, const_value_to_expression, const_value_to_property_name,
        const_value_to_string, resolve_constant_token,
    },
    env::replace_env_references,
    generated_strings::GeneratedString,
    hash::hash,
    hooks::{HookCompiler, HookValue, is_hook_key, is_hook_name, valid_hook_name_description},
    variables::create_variable_default_name,
};

enum DynamicHookExpression {
    PropertyValue(Expr),
    CssValue(Expr),
}

type DynamicHookValue = (String, DynamicHookExpression);
const IMPORTED_VARIABLE_TOKEN_PROPERTY_NAME: &str = "--_nanocss_imported_variable_token";

pub(crate) fn parse_create_arg(
    expression: &Expr,
    css_names: &HashSet<String>,
    variable_groups: &HashMap<String, HashMap<String, String>>,
    imported_variable_group_names: &HashSet<String>,
    const_groups: &ConstGroups,
    hook_compiler: &mut HookCompiler,
    file_identity: &str,
    dynamic_hook_id: &mut usize,
    debug: bool,
    env: &serde_json::Value,
) -> ObjectLit {
    let Expr::Object(object) = expression else {
        panic!("[nanocss] css.create(...) must be called with a static object expression.");
    };
    if object.props.is_empty() {
        panic!("[nanocss] css.create(...) must define at least one style.");
    }

    let mut compiled = object.clone();
    for property in &object.props {
        let PropOrSpread::Prop(property) = property else {
            panic!("[nanocss] css.create(...) objects cannot contain spreads.");
        };
        let Prop::KeyValue(property) = &**property else {
            panic!("[nanocss] css.create(...) values must be style objects.");
        };
        match &*property.value {
            Expr::Object(style) => validate_style_object(style),
            Expr::Arrow(arrow) => {
                let style = validate_dynamic_style_function(arrow);
                validate_style_object(style);
            }
            _ => {
                panic!(
                    "[nanocss] css.create(...) values must be style object expressions or arrow functions returning style object expressions."
                );
            }
        }
    }

    for property in &mut compiled.props {
        let PropOrSpread::Prop(property) = property else {
            continue;
        };
        let Prop::KeyValue(property) = &mut **property else {
            continue;
        };
        match &mut *property.value {
            Expr::Object(style) => compile_style_object(
                style,
                css_names,
                variable_groups,
                imported_variable_group_names,
                const_groups,
                hook_compiler,
                file_identity,
                dynamic_hook_id,
                debug,
                env,
            ),
            Expr::Arrow(arrow) => {
                let BlockStmtOrExpr::Expr(body) = &mut *arrow.body else {
                    continue;
                };
                let Some(style) = get_object_expression_mut(body) else {
                    continue;
                };
                let mut css_names = css_names.clone();
                let mut variable_groups = variable_groups.clone();
                let mut imported_variable_group_names = imported_variable_group_names.clone();
                let mut const_groups = const_groups.clone();
                for param in &arrow.params {
                    if let Pat::Ident(param) = param {
                        let name = param.id.sym.to_string();
                        css_names.remove(&name);
                        variable_groups.remove(&name);
                        imported_variable_group_names.remove(&name);
                        const_groups.remove(&name);
                    }
                }
                compile_style_object(
                    style,
                    &css_names,
                    &variable_groups,
                    &imported_variable_group_names,
                    &const_groups,
                    hook_compiler,
                    file_identity,
                    dynamic_hook_id,
                    debug,
                    env,
                );
            }
            _ => {}
        }
    }

    compiled
}

pub(crate) struct StaticCssCompileContext<'a> {
    pub css_names: &'a HashSet<String>,
    pub variable_groups: &'a HashMap<String, HashMap<String, String>>,
    pub imported_variable_group_names: &'a HashSet<String>,
    pub const_groups: &'a ConstGroups,
    pub generated_string_names: &'a HashMap<String, GeneratedString>,
    pub hook_compiler: &'a mut HookCompiler,
    pub file_identity: &'a str,
    pub dynamic_hook_id: &'a mut usize,
    pub debug: bool,
    pub env: &'a serde_json::Value,
    pub api_name: &'static str,
    pub allow_shorthand_properties: bool,
}

pub(crate) fn compile_static_style_object_to_css_declarations(
    object: &ObjectLit,
    context: &mut StaticCssCompileContext,
) -> Vec<(String, String)> {
    if !context.allow_shorthand_properties {
        validate_style_object(object);
    }

    let mut compiled = object.clone();
    replace_generated_string_references(
        &mut compiled,
        context.generated_string_names,
        context.api_name,
    );
    compile_style_object(
        &mut compiled,
        context.css_names,
        context.variable_groups,
        context.imported_variable_group_names,
        context.const_groups,
        context.hook_compiler,
        context.file_identity,
        context.dynamic_hook_id,
        context.debug,
        context.env,
    );

    let mut declarations = Vec::new();
    for property in &compiled.props {
        let PropOrSpread::Prop(property) = property else {
            panic!(
                "[nanocss] {} style objects cannot contain spreads.",
                context.api_name
            );
        };
        let Prop::KeyValue(property) = &**property else {
            panic!(
                "[nanocss] {} style values must be expressions.",
                context.api_name
            );
        };
        let Some(property_name) = prop_name_to_string(&property.key) else {
            panic!(
                "[nanocss] {} style property keys must be statically known.",
                context.api_name
            );
        };
        let value = serialize_static_css_value(&property_name, &property.value, context.api_name);
        declarations.push((css_property_name(&property_name), value));
    }

    declarations
}

fn replace_generated_string_references(
    object: &mut ObjectLit,
    generated_string_names: &HashMap<String, GeneratedString>,
    api_name: &str,
) {
    for property in &mut object.props {
        let PropOrSpread::Prop(property) = property else {
            continue;
        };
        let Prop::KeyValue(property) = &mut **property else {
            continue;
        };
        replace_generated_string_references_in_expr(
            &mut property.value,
            generated_string_names,
            api_name,
        );
    }
}

fn replace_generated_string_references_in_expr(
    expression: &mut Expr,
    generated_string_names: &HashMap<String, GeneratedString>,
    api_name: &str,
) {
    match expression {
        Expr::Ident(identifier) => {
            if let Some(name) = generated_string_names.get(&identifier.sym.to_string()) {
                if !name.is_css_identifier() {
                    panic!(
                        "[nanocss] {} style values can only use generated css.keyframes(...) or css.positionTry(...) strings.",
                        api_name
                    );
                }
                *expression = Expr::Lit(Lit::Str(Str {
                    span: DUMMY_SP,
                    value: name.value.clone().into(),
                    raw: None,
                }));
            }
        }
        Expr::Object(object) => {
            replace_generated_string_references(object, generated_string_names, api_name)
        }
        Expr::Paren(paren) => replace_generated_string_references_in_expr(
            &mut paren.expr,
            generated_string_names,
            api_name,
        ),
        Expr::Call(call) => {
            for arg in &mut call.args {
                replace_generated_string_references_in_expr(
                    &mut arg.expr,
                    generated_string_names,
                    api_name,
                );
            }
        }
        _ => {}
    }
}

fn serialize_static_css_value(property_name: &str, expression: &Expr, api_name: &str) -> String {
    match expression {
        Expr::Lit(Lit::Str(value)) => value
            .value
            .as_str()
            .map(ToString::to_string)
            .unwrap_or_default(),
        Expr::Lit(Lit::Num(value)) => stringify_numeric_style_value(property_name, value.value),
        Expr::Lit(Lit::Bool(value)) => value.value.to_string(),
        Expr::Lit(Lit::Null(_)) => "revert-layer".to_string(),
        _ => panic!(
            "[nanocss] {} style values must be static string, number, boolean, null, variable, constant, keyframes, firstThatWorks, or hook values.",
            api_name
        ),
    }
}

fn validate_dynamic_style_function(arrow: &ArrowExpr) -> &ObjectLit {
    if arrow
        .params
        .iter()
        .any(|param| !matches!(param, Pat::Ident(_)))
    {
        panic!(
            "[nanocss] css.create(...) dynamic style function parameters must be simple identifiers."
        );
    }

    let BlockStmtOrExpr::Expr(body) = &*arrow.body else {
        panic!("[nanocss] css.create(...) dynamic style function bodies must be object literals.");
    };
    let Some(style) = get_object_expression(body) else {
        panic!("[nanocss] css.create(...) dynamic style function bodies must be object literals.");
    };

    style
}

fn get_object_expression(expression: &Expr) -> Option<&ObjectLit> {
    match expression {
        Expr::Object(object) => Some(object),
        Expr::Paren(paren) => get_object_expression(&paren.expr),
        _ => None,
    }
}

fn get_object_expression_mut(expression: &mut Expr) -> Option<&mut ObjectLit> {
    match expression {
        Expr::Object(object) => Some(object),
        Expr::Paren(paren) => get_object_expression_mut(&mut paren.expr),
        _ => None,
    }
}

fn validate_style_object(object: &ObjectLit) {
    for property in &object.props {
        let PropOrSpread::Prop(property) = property else {
            panic!("[nanocss] Style objects cannot contain spreads when using the compiler.");
        };
        let Some(property_name) = get_style_property_name(property) else {
            continue;
        };
        if !property_name.starts_with("--") && is_shorthand_property(&property_name) {
            panic!(
                "[nanocss] CSS shorthand property {:?} is not supported by the compiler. Use longhand properties instead.",
                property_name
            );
        }
    }
}

fn get_style_property_name(property: &Prop) -> Option<String> {
    match property {
        Prop::KeyValue(property) => {
            if matches!(property.key, PropName::Computed(_)) {
                return None;
            }
            prop_name_to_string(&property.key)
        }
        Prop::Shorthand(identifier) => Some(identifier.sym.to_string()),
        _ => None,
    }
}

fn compile_style_object(
    object: &mut ObjectLit,
    css_names: &HashSet<String>,
    variable_groups: &HashMap<String, HashMap<String, String>>,
    imported_variable_group_names: &HashSet<String>,
    const_groups: &ConstGroups,
    hook_compiler: &mut HookCompiler,
    file_identity: &str,
    dynamic_hook_id: &mut usize,
    debug: bool,
    env: &serde_json::Value,
) {
    let mut next_properties = Vec::new();

    for mut property_or_spread in object.props.drain(..) {
        let PropOrSpread::Prop(property) = &mut property_or_spread else {
            next_properties.push(property_or_spread);
            continue;
        };
        let Prop::KeyValue(property) = &mut **property else {
            next_properties.push(property_or_spread);
            continue;
        };
        if let PropName::Computed(ComputedPropName { expr, .. }) = &mut property.key {
            replace_env_references(expr, css_names, env);
        }
        replace_env_references(&mut property.value, css_names, env);

        let mut is_imported_variable_token_key = false;
        if let PropName::Computed(computed) = &property.key
            && let Some(value) = resolve_constant_token(&computed.expr, const_groups)
        {
            property.key = const_value_to_property_name(&value);
        } else if let PropName::Computed(computed) = &property.key
            && matches!(
                &*computed.expr,
                Expr::Lit(Lit::Str(_)) | Expr::Lit(Lit::Num(_))
            )
        {
            match &*computed.expr {
                Expr::Lit(Lit::Str(value)) => {
                    property.key = PropName::Str(Str {
                        span: DUMMY_SP,
                        value: value.value.clone(),
                        raw: None,
                    });
                }
                Expr::Lit(Lit::Num(value)) => {
                    property.key = PropName::Num(value.clone());
                }
                _ => unreachable!(),
            }
        } else if let PropName::Computed(computed) = &property.key
            && let Some(custom_property_name) =
                resolve_variable_token(&computed.expr, variable_groups)
        {
            property.key = PropName::Str(Str {
                span: DUMMY_SP,
                value: custom_property_name.into(),
                raw: None,
            });
        } else if let PropName::Computed(computed) = &property.key
            && resolve_imported_variable_token(&computed.expr, imported_variable_group_names)
                .is_some()
        {
            is_imported_variable_token_key = true;
        } else if matches!(property.key, PropName::Computed(_)) {
            panic!("[nanocss] Style property keys must be statically known.");
        }

        let property_name = prop_name_to_string(&property.key).or_else(|| {
            is_imported_variable_token_key
                .then(|| IMPORTED_VARIABLE_TOKEN_PROPERTY_NAME.to_string())
        });

        if let Some((property_name, hook_value, dynamic_values)) =
            property_name.as_ref().and_then(|property_name| {
                parse_style_hook_value(
                    property_name,
                    &property.value,
                    css_names,
                    variable_groups,
                    imported_variable_group_names,
                    const_groups,
                    file_identity,
                    dynamic_hook_id,
                    debug,
                )
                .map(|(hook_value, dynamic_values)| {
                    (property_name.clone(), hook_value, dynamic_values)
                })
            })
        {
            for (name, expression) in dynamic_values {
                next_properties.push(PropOrSpread::Prop(Box::new(Prop::KeyValue(
                    swc_core::ecma::ast::KeyValueProp {
                        key: PropName::Str(Str {
                            span: DUMMY_SP,
                            value: name.into(),
                            raw: None,
                        }),
                        value: Box::new(match expression {
                            DynamicHookExpression::PropertyValue(expression) => {
                                create_dynamic_hook_value_expression(&property_name, expression)
                            }
                            DynamicHookExpression::CssValue(expression) => expression,
                        }),
                    },
                ))));
            }
            for (compiled_property_name, compiled_value) in
                hook_compiler.compile_property(&property_name, &hook_value)
            {
                next_properties.push(PropOrSpread::Prop(Box::new(Prop::KeyValue(
                    swc_core::ecma::ast::KeyValueProp {
                        key: if compiled_property_name == property_name {
                            property.key.clone()
                        } else {
                            PropName::Str(Str {
                                span: DUMMY_SP,
                                value: compiled_property_name.into(),
                                raw: None,
                            })
                        },
                        value: Box::new(Expr::Lit(Lit::Str(Str {
                            span: DUMMY_SP,
                            value: compiled_value.into(),
                            raw: None,
                        }))),
                    },
                ))));
            }
            continue;
        }

        if let Some(custom_property_name) = resolve_variable_token(&property.value, variable_groups)
        {
            property.value = Box::new(create_local_css_variable_value(
                &custom_property_name,
                debug,
            ));
        } else if let Some(token) =
            resolve_imported_variable_token(&property.value, imported_variable_group_names)
        {
            property.value = Box::new(create_imported_css_variable_value(token, debug));
        } else if let Some(value) = resolve_constant_token(&property.value, const_groups) {
            property.value = Box::new(const_value_to_expression(&value));
        }

        next_properties.push(property_or_spread);
    }

    object.props = next_properties;
}

fn parse_style_hook_object(
    property_name: &str,
    object: &ObjectLit,
    css_names: &HashSet<String>,
    variable_groups: &HashMap<String, HashMap<String, String>>,
    imported_variable_group_names: &HashSet<String>,
    const_groups: &ConstGroups,
    file_identity: &str,
    dynamic_hook_id: &mut usize,
    debug: bool,
) -> Option<(HookValue, Vec<DynamicHookValue>)> {
    let mut dynamic_values = Vec::new();
    let value = parse_hook_object_with_dynamic(
        property_name,
        object,
        css_names,
        variable_groups,
        imported_variable_group_names,
        const_groups,
        file_identity,
        dynamic_hook_id,
        debug,
        &mut dynamic_values,
    )?;
    Some((value, dynamic_values))
}

fn parse_style_hook_value(
    property_name: &str,
    expression: &Expr,
    css_names: &HashSet<String>,
    variable_groups: &HashMap<String, HashMap<String, String>>,
    imported_variable_group_names: &HashSet<String>,
    const_groups: &ConstGroups,
    file_identity: &str,
    dynamic_hook_id: &mut usize,
    debug: bool,
) -> Option<(HookValue, Vec<DynamicHookValue>)> {
    match expression {
        Expr::Object(value) => parse_style_hook_object(
            property_name,
            value,
            css_names,
            variable_groups,
            imported_variable_group_names,
            const_groups,
            file_identity,
            dynamic_hook_id,
            debug,
        ),
        Expr::Call(call) if is_first_that_works_call(css_names, call) => Some((
            parse_first_that_works_call(property_name, call, const_groups),
            Vec::new(),
        )),
        _ => None,
    }
}

pub(crate) fn parse_hook_object_with_consts(
    object: &ObjectLit,
    const_groups: &ConstGroups,
) -> Option<HookValue> {
    parse_hook_object_for_property("", &HashSet::new(), object, const_groups)
}

fn parse_hook_object_for_property(
    property_name: &str,
    css_names: &HashSet<String>,
    object: &ObjectLit,
    const_groups: &ConstGroups,
) -> Option<HookValue> {
    let mut entries = Vec::new();
    let mut has_hook_key = false;
    let mut has_default = false;

    for property in &object.props {
        let PropOrSpread::Prop(property) = property else {
            panic!("[nanocss] Hook objects cannot contain spreads.");
        };
        let Prop::KeyValue(property) = &**property else {
            panic!("[nanocss] Hook object values must be expressions.");
        };
        let Some(key) = hook_key_to_string(&property.key, const_groups) else {
            panic!("[nanocss] Hook object keys must be statically known.");
        };
        if !is_hook_key(&key) {
            if has_hook_key {
                panic!(
                    "[nanocss] {:?} is not a valid hook name. {}",
                    key,
                    valid_hook_name_description()
                );
            }
            return None;
        }
        has_hook_key = true;
        if key == "default" {
            has_default = true;
        } else if !is_hook_name(&key) {
            panic!(
                "[nanocss] {:?} is not a valid hook name. {}",
                key,
                valid_hook_name_description()
            );
        }
        entries.push((
            key,
            parse_hook_value(property_name, css_names, &property.value, const_groups),
        ));
    }

    if !has_hook_key {
        return None;
    }
    if !has_default {
        panic!("[nanocss] Hook objects must include a default value.");
    }
    Some(HookValue::Object(entries))
}

fn hook_key_to_string(name: &PropName, const_groups: &ConstGroups) -> Option<String> {
    match name {
        PropName::Computed(computed) => resolve_constant_token(&computed.expr, const_groups)
            .map(|value| const_value_to_string(&value)),
        _ => prop_name_to_string(name),
    }
}

fn parse_hook_object_with_dynamic(
    property_name: &str,
    object: &ObjectLit,
    css_names: &HashSet<String>,
    variable_groups: &HashMap<String, HashMap<String, String>>,
    imported_variable_group_names: &HashSet<String>,
    const_groups: &ConstGroups,
    file_identity: &str,
    dynamic_hook_id: &mut usize,
    debug: bool,
    dynamic_values: &mut Vec<DynamicHookValue>,
) -> Option<HookValue> {
    let mut entries = Vec::new();
    let mut has_hook_key = false;
    let mut has_default = false;

    for property in &object.props {
        let PropOrSpread::Prop(property) = property else {
            panic!("[nanocss] Hook objects cannot contain spreads.");
        };
        let Prop::KeyValue(property) = &**property else {
            panic!("[nanocss] Hook object values must be expressions.");
        };
        let Some(key) = hook_key_to_string(&property.key, const_groups) else {
            panic!("[nanocss] Hook object keys must be statically known.");
        };
        if !is_hook_key(&key) {
            if has_hook_key {
                panic!(
                    "[nanocss] {:?} is not a valid hook name. {}",
                    key,
                    valid_hook_name_description()
                );
            }
            return None;
        }
        has_hook_key = true;
        if key == "default" {
            has_default = true;
        } else if !is_hook_name(&key) {
            panic!(
                "[nanocss] {:?} is not a valid hook name. {}",
                key,
                valid_hook_name_description()
            );
        }
        entries.push((
            key,
            parse_hook_value_with_dynamic(
                property_name,
                css_names,
                &property.value,
                variable_groups,
                imported_variable_group_names,
                const_groups,
                file_identity,
                dynamic_hook_id,
                debug,
                dynamic_values,
            ),
        ));
    }

    if !has_hook_key {
        return None;
    }
    if !has_default {
        panic!("[nanocss] Hook objects must include a default value.");
    }
    Some(HookValue::Object(entries))
}

fn parse_hook_value(
    property_name: &str,
    css_names: &HashSet<String>,
    expression: &Expr,
    const_groups: &ConstGroups,
) -> HookValue {
    match expression {
        Expr::Lit(Lit::Str(value)) => HookValue::String(
            value
                .value
                .as_str()
                .map(ToString::to_string)
                .unwrap_or_default(),
        ),
        Expr::Lit(Lit::Num(value)) => HookValue::Number(value.value),
        Expr::Lit(Lit::Bool(value)) => HookValue::Boolean(value.value),
        Expr::Lit(Lit::Null(_)) => HookValue::Null,
        Expr::Object(object) => {
            parse_hook_object_for_property(property_name, css_names, object, const_groups)
                .unwrap_or_else(|| {
                    panic!("[nanocss] Nested style objects must use declared hooks or default.")
                })
        }
        Expr::Call(call) if is_first_that_works_call(css_names, call) => {
            parse_first_that_works_call(property_name, call, const_groups)
        }
        expression => {
            if let Some(value) = resolve_constant_token(expression, const_groups) {
                return match value {
                    crate::define_consts::ConstValue::String(value) => HookValue::String(value),
                    crate::define_consts::ConstValue::Number(value) => HookValue::Number(value),
                };
            }
            panic!("[nanocss] Hook values must be static literals.")
        }
    }
}

fn parse_hook_value_with_dynamic(
    property_name: &str,
    css_names: &HashSet<String>,
    expression: &Expr,
    variable_groups: &HashMap<String, HashMap<String, String>>,
    imported_variable_group_names: &HashSet<String>,
    const_groups: &ConstGroups,
    file_identity: &str,
    dynamic_hook_id: &mut usize,
    debug: bool,
    dynamic_values: &mut Vec<DynamicHookValue>,
) -> HookValue {
    match expression {
        Expr::Lit(Lit::Str(value)) => HookValue::String(
            value
                .value
                .as_str()
                .map(ToString::to_string)
                .unwrap_or_default(),
        ),
        Expr::Lit(Lit::Num(value)) => HookValue::Number(value.value),
        Expr::Lit(Lit::Bool(value)) => HookValue::Boolean(value.value),
        Expr::Lit(Lit::Null(_)) => HookValue::Null,
        Expr::Object(object) => parse_hook_object_with_dynamic(
            property_name,
            object,
            css_names,
            variable_groups,
            imported_variable_group_names,
            const_groups,
            file_identity,
            dynamic_hook_id,
            debug,
            dynamic_values,
        )
        .unwrap_or_else(|| {
            panic!("[nanocss] Nested style objects must use declared hooks or default.")
        }),
        Expr::Call(call) if is_first_that_works_call(css_names, call) => {
            parse_first_that_works_call(property_name, call, const_groups)
        }
        expression => {
            if let Some(value) = resolve_constant_token(expression, const_groups) {
                return match value {
                    crate::define_consts::ConstValue::String(value) => HookValue::String(value),
                    crate::define_consts::ConstValue::Number(value) => HookValue::Number(value),
                };
            }
            if let Some(custom_property_name) = resolve_variable_token(expression, variable_groups)
            {
                return HookValue::String(create_local_css_variable_value_string(
                    &custom_property_name,
                    debug,
                ));
            }
            if let Some(token) =
                resolve_imported_variable_token(expression, imported_variable_group_names)
            {
                let name = create_dynamic_hook_name(file_identity, *dynamic_hook_id, debug);
                *dynamic_hook_id += 1;
                dynamic_values.push((
                    name.clone(),
                    DynamicHookExpression::CssValue(create_imported_css_variable_value(
                        token, debug,
                    )),
                ));
                return HookValue::Dynamic(name);
            }
            let name = create_dynamic_hook_name(file_identity, *dynamic_hook_id, debug);
            *dynamic_hook_id += 1;
            dynamic_values.push((
                name.clone(),
                DynamicHookExpression::PropertyValue(expression.clone()),
            ));
            HookValue::Dynamic(name)
        }
    }
}

fn parse_first_that_works_call(
    property_name: &str,
    call: &CallExpr,
    const_groups: &ConstGroups,
) -> HookValue {
    if call.args.is_empty() || call.args.iter().any(|arg| arg.spread.is_some()) {
        panic!(
            "[nanocss] css.firstThatWorks(...) must be called with static string or number arguments."
        );
    }

    let mut values = Vec::new();
    for arg in &call.args {
        values.push(parse_first_that_works_value(
            property_name,
            &arg.expr,
            const_groups,
        ));
    }
    let fallback = values
        .last()
        .expect("expected firstThatWorks fallback")
        .clone();
    let mut entries = vec![("default".to_string(), HookValue::String(fallback))];
    for value in values.iter().rev().skip(1) {
        entries.push((
            format!("@supports ({property_name}: {value})"),
            HookValue::String(value.clone()),
        ));
    }
    HookValue::Object(entries)
}

fn parse_first_that_works_value(
    property_name: &str,
    expression: &Expr,
    const_groups: &ConstGroups,
) -> String {
    if let Some(value) = resolve_constant_token(expression, const_groups) {
        return match value {
            crate::define_consts::ConstValue::String(value) => value,
            crate::define_consts::ConstValue::Number(value) => {
                stringify_numeric_style_value(property_name, value)
            }
        };
    }

    match expression {
        Expr::Lit(Lit::Str(value)) => value
            .value
            .as_str()
            .map(ToString::to_string)
            .unwrap_or_default(),
        Expr::Lit(Lit::Num(value)) => stringify_numeric_style_value(property_name, value.value),
        _ => {
            panic!(
                "[nanocss] css.firstThatWorks(...) must be called with static string or number arguments."
            );
        }
    }
}

fn stringify_numeric_style_value(property_name: &str, value: f64) -> String {
    let mut formatted = format_number(value);
    if value != 0.0 && !is_unitless_number(property_name) {
        formatted.push_str("px");
    }
    formatted
}

fn is_first_that_works_call(css_names: &HashSet<String>, call: &CallExpr) -> bool {
    is_css_member_call(css_names, &call.callee, "firstThatWorks")
}

fn resolve_variable_token(
    expression: &Expr,
    variable_groups: &HashMap<String, HashMap<String, String>>,
) -> Option<String> {
    let Expr::Member(member) = expression else {
        return None;
    };
    resolve_variable_member(member, variable_groups)
}

fn resolve_variable_member(
    member: &MemberExpr,
    variable_groups: &HashMap<String, HashMap<String, String>>,
) -> Option<String> {
    let Expr::Ident(group_name) = &*member.obj else {
        return None;
    };
    let token_name = match &member.prop {
        MemberProp::Ident(token_name) => token_name.sym.to_string(),
        MemberProp::Computed(ComputedPropName { expr, .. }) => match &**expr {
            Expr::Lit(Lit::Str(token_name)) => token_name.value.as_str()?.to_string(),
            _ => return None,
        },
        _ => return None,
    };
    variable_groups
        .get(&group_name.sym.to_string())
        .and_then(|group| group.get(&token_name))
        .cloned()
}

fn resolve_imported_variable_token(
    expression: &Expr,
    imported_variable_group_names: &HashSet<String>,
) -> Option<Expr> {
    let Expr::Member(member) = expression else {
        return None;
    };
    let Expr::Ident(group_name) = &*member.obj else {
        return None;
    };
    if !imported_variable_group_names.contains(&group_name.sym.to_string()) {
        return None;
    }
    Some(Expr::Member(member.clone()))
}

fn create_local_css_variable_value(custom_property_name: &str, debug: bool) -> Expr {
    Expr::Lit(Lit::Str(Str {
        span: DUMMY_SP,
        value: create_local_css_variable_value_string(custom_property_name, debug).into(),
        raw: None,
    }))
}

fn create_local_css_variable_value_string(custom_property_name: &str, debug: bool) -> String {
    format!(
        "var({}, var({}))",
        custom_property_name,
        create_variable_default_name(custom_property_name, debug)
    )
}

fn create_imported_css_variable_value(token: Expr, debug: bool) -> Expr {
    let default_suffix = if debug { "--n-default" } else { "--nd" };
    add_expr(
        add_expr(
            add_expr(
                add_expr(string_expr("var("), token.clone()),
                string_expr(", var("),
            ),
            add_expr(token, string_expr(default_suffix)),
        ),
        string_expr("))"),
    )
}

fn create_dynamic_hook_name(file_identity: &str, id: usize, debug: bool) -> String {
    let prefix = if debug {
        "--_nanocss_dynamic_"
    } else {
        "--nd-"
    };
    format!(
        "{prefix}{}",
        hash(&format!("{:?}", format!("{file_identity}:{id}")))
    )
}

fn create_dynamic_hook_value_expression(property_name: &str, expression: Expr) -> Expr {
    let unit = if crate::constants::is_unitless_number(property_name) {
        ""
    } else {
        "px"
    };
    Expr::Cond(CondExpr {
        span: DUMMY_SP,
        test: Box::new(Expr::Bin(BinExpr {
            span: DUMMY_SP,
            op: BinaryOp::EqEqEq,
            left: Box::new(Expr::Unary(UnaryExpr {
                span: DUMMY_SP,
                op: UnaryOp::TypeOf,
                arg: Box::new(expression.clone()),
            })),
            right: Box::new(string_expr("number")),
        })),
        cons: Box::new(if unit.is_empty() {
            expression.clone()
        } else {
            add_expr(expression.clone(), string_expr(unit))
        }),
        alt: Box::new(expression),
    })
}

fn add_expr(left: Expr, right: Expr) -> Expr {
    Expr::Bin(BinExpr {
        span: DUMMY_SP,
        op: BinaryOp::Add,
        left: Box::new(left),
        right: Box::new(right),
    })
}

fn string_expr(value: &str) -> Expr {
    Expr::Lit(Lit::Str(Str {
        span: DUMMY_SP,
        value: value.into(),
        raw: None,
    }))
}
