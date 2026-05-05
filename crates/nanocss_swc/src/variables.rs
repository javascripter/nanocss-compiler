use std::collections::{HashMap, HashSet};

use crate::{
    ast::{format_number, prop_name_to_string},
    define_consts::{ConstGroups, const_value_to_string, resolve_constant_token},
    generated_strings::GeneratedString,
    hash::hash,
    hooks::{HookCompiler, HookValue, is_hook_key, is_hook_name},
};

use swc_core::{
    common::DUMMY_SP,
    ecma::ast::{
        ArrowExpr, BinaryOp, BlockStmtOrExpr, Callee, Expr, Lit, MemberExpr, MemberProp, Number,
        Prop, PropOrSpread, Str, Tpl, UnaryExpr, UnaryOp,
    },
};

#[derive(Debug, Clone, PartialEq)]
pub(crate) enum VariableValue {
    String(String),
    Boolean(bool),
    Null,
    Hook(HookValue),
    Typed(TypedVariableValue),
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct TypedVariableValue {
    pub syntax: &'static str,
    pub initial_value: String,
    pub value: Box<VariableValue>,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct CompiledVariableDefault {
    pub custom_property_name: String,
    pub value: String,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct CompiledVariableProperty {
    pub custom_property_name: String,
    pub syntax: &'static str,
    pub initial_value: String,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct CompiledVariableToken {
    pub token_name: String,
    pub custom_property_name: String,
    pub defaults: Vec<CompiledVariableDefault>,
    pub property: Option<CompiledVariableProperty>,
}

pub(crate) type VariableTokens = Vec<(String, VariableValue)>;

pub(crate) const VARIABLE_DEFAULTS_PROPERTY_NAME: &str = "$$defaults";

struct ParsedDefineVarToken<'a> {
    name: String,
    value: ParsedDefineVarValue<'a>,
}

enum ParsedDefineVarValue<'a> {
    Expression(&'a Expr),
    GeneratedString(String),
}

struct DerivedValueContext<'a> {
    group_name: &'a str,
    self_group: &'a HashMap<String, String>,
    variable_groups: &'a HashMap<String, HashMap<String, String>>,
    imported_variable_group_names: &'a HashSet<String>,
    const_groups: &'a ConstGroups,
    generated_string_names: &'a HashMap<String, GeneratedString>,
    debug: bool,
    dependencies: Vec<String>,
}

pub(crate) fn parse_define_vars_arg(
    group_name: &str,
    expression: &Expr,
    css_names: &HashSet<String>,
    file_identity: &str,
    variable_groups: &HashMap<String, HashMap<String, String>>,
    imported_variable_group_names: &HashSet<String>,
    const_groups: &ConstGroups,
    generated_string_names: &HashMap<String, GeneratedString>,
    debug: bool,
) -> VariableTokens {
    let Expr::Object(object) = expression else {
        panic!("[nanocss] css.defineVars(...) must be called with a static object expression.");
    };

    let mut parsed_tokens = Vec::new();
    for property in &object.props {
        let PropOrSpread::Prop(property) = property else {
            panic!("[nanocss] css.defineVars(...) objects cannot contain spreads.");
        };
        let (token_name, value) = match &**property {
            Prop::KeyValue(property) => {
                let Some(token_name) = prop_name_to_string(&property.key) else {
                    panic!("[nanocss] css.defineVars(...) keys must be statically known.");
                };
                (
                    token_name,
                    ParsedDefineVarValue::Expression(&property.value),
                )
            }
            Prop::Shorthand(identifier) => (
                identifier.sym.to_string(),
                ParsedDefineVarValue::GeneratedString(identifier.sym.to_string()),
            ),
            _ => {
                panic!("[nanocss] css.defineVars(...) values must be expressions.");
            }
        };
        if token_name == VARIABLE_DEFAULTS_PROPERTY_NAME {
            panic!(
                "[nanocss] css.defineVars(...) token names cannot use the reserved \"$$defaults\" key."
            );
        }
        parsed_tokens.push(ParsedDefineVarToken {
            name: token_name,
            value,
        });
    }

    let token_order = parsed_tokens
        .iter()
        .map(|token| token.name.clone())
        .collect::<Vec<_>>();
    let self_group = parsed_tokens
        .iter()
        .map(|token| {
            (
                token.name.clone(),
                create_generated_variable_name(file_identity, group_name, &token.name, debug),
            )
        })
        .collect::<HashMap<_, _>>();

    let mut dependency_map = HashMap::new();
    let mut tokens = Vec::new();
    for token in parsed_tokens {
        let mut context = DerivedValueContext {
            group_name,
            self_group: &self_group,
            variable_groups,
            imported_variable_group_names,
            const_groups,
            generated_string_names,
            debug,
            dependencies: Vec::new(),
        };
        let value = match token.value {
            ParsedDefineVarValue::Expression(expression) => {
                parse_define_var_value(expression, css_names, &mut context)
            }
            ParsedDefineVarValue::GeneratedString(name) => {
                parse_generated_string_var_value(&name, generated_string_names)
            }
        };
        dependency_map.insert(token.name.clone(), context.dependencies);
        tokens.push((token.name, value));
    }

    assert_no_define_vars_cycles(&dependency_map, &token_order);

    tokens
}

fn parse_define_var_value(
    expression: &Expr,
    css_names: &HashSet<String>,
    context: &mut DerivedValueContext,
) -> VariableValue {
    if let Some(value) = parse_typed_variable_value(expression, css_names) {
        return VariableValue::Typed(value);
    }

    match expression {
        Expr::Arrow(arrow) => parse_define_var_function_value(arrow, css_names, context),
        Expr::Lit(Lit::Str(value)) => VariableValue::String(
            value
                .value
                .as_str()
                .map(ToString::to_string)
                .unwrap_or_default(),
        ),
        Expr::Lit(Lit::Bool(value)) => VariableValue::Boolean(value.value),
        Expr::Lit(Lit::Null(_)) => VariableValue::Null,
        Expr::Object(object) => VariableValue::Hook(parse_variable_hook_object(object)),
        Expr::Ident(identifier) => parse_generated_string_var_value(
            &identifier.sym.to_string(),
            context.generated_string_names,
        ),
        Expr::Lit(Lit::Num(_)) => panic!(
            "[nanocss] css.defineVars(...) numeric defaults are not supported. Use strings such as \"4px\" or \"0.5\" instead."
        ),
        _ => panic!("[nanocss] css.defineVars(...) failed to compile a variable fallback."),
    }
}

fn parse_define_var_function_value(
    arrow: &ArrowExpr,
    css_names: &HashSet<String>,
    context: &mut DerivedValueContext,
) -> VariableValue {
    if !arrow.params.is_empty() {
        panic!(
            "[nanocss] css.defineVars(...) function values must be zero-argument expression functions."
        );
    }
    let BlockStmtOrExpr::Expr(body) = &*arrow.body else {
        panic!(
            "[nanocss] css.defineVars(...) function values must be zero-argument expression functions."
        );
    };

    if let Some(value) = parse_derived_typed_variable_value(body, css_names, context) {
        return VariableValue::Typed(value);
    }

    match &**body {
        Expr::Object(object) => VariableValue::Hook(parse_derived_variable_hook_object(
            object, css_names, context,
        )),
        expression => VariableValue::String(compile_derived_string_expression(
            expression, css_names, context,
        )),
    }
}

fn parse_derived_typed_variable_value(
    expression: &Expr,
    css_names: &HashSet<String>,
    _context: &mut DerivedValueContext,
) -> Option<TypedVariableValue> {
    parse_typed_variable_value(expression, css_names)
}

fn parse_derived_variable_hook_object(
    object: &swc_core::ecma::ast::ObjectLit,
    css_names: &HashSet<String>,
    context: &mut DerivedValueContext,
) -> HookValue {
    let mut entries = Vec::new();
    let mut has_default = false;

    for property in &object.props {
        let PropOrSpread::Prop(property) = property else {
            panic!("[nanocss] Hook objects cannot contain spreads.");
        };
        let Prop::KeyValue(property) = &**property else {
            panic!("[nanocss] Hook object values must be expressions.");
        };
        let Some(key) = prop_name_to_string(&property.key) else {
            panic!("[nanocss] Hook object keys must be statically known.");
        };
        if !is_hook_key(&key) || (key != "default" && !is_hook_name(&key)) {
            panic!("[nanocss] Nested variable values must use declared hooks or default.");
        }
        if key == "default" {
            has_default = true;
        }
        entries.push((
            key,
            match &*property.value {
                Expr::Object(object) => {
                    parse_derived_variable_hook_object(object, css_names, context)
                }
                expression => HookValue::String(compile_derived_string_expression(
                    expression, css_names, context,
                )),
            },
        ));
    }

    if !has_default {
        panic!("[nanocss] Hook objects must include a default value.");
    }
    HookValue::Object(entries)
}

fn compile_derived_string_expression(
    expression: &Expr,
    css_names: &HashSet<String>,
    context: &mut DerivedValueContext,
) -> String {
    match expression {
        Expr::Lit(Lit::Str(value)) => value
            .value
            .as_str()
            .map(ToString::to_string)
            .unwrap_or_default(),
        Expr::Tpl(template) => compile_derived_template(template, css_names, context),
        Expr::Bin(binary) if binary.op == BinaryOp::Add => {
            let left = compile_derived_interpolation_expression(&binary.left, css_names, context);
            let right = compile_derived_interpolation_expression(&binary.right, css_names, context);
            format!("{left}{right}")
        }
        Expr::Member(member) => resolve_derived_member(member, context),
        Expr::Paren(paren) => compile_derived_string_expression(&paren.expr, css_names, context),
        Expr::Ident(identifier) => {
            match parse_generated_string_var_value(
                &identifier.sym.to_string(),
                context.generated_string_names,
            ) {
                VariableValue::String(value) => value,
                _ => panic!("[nanocss] css.defineVars(...) failed to compile a variable fallback."),
            }
        }
        Expr::Lit(Lit::Num(_)) => panic!(
            "[nanocss] css.defineVars(...) numeric defaults are not supported. Use strings such as \"4px\" or \"0.5\" instead."
        ),
        _ => panic!(
            "[nanocss] css.defineVars(...) function values must return static strings, hook objects, or css.types.*(...) values."
        ),
    }
}

fn compile_derived_interpolation_expression(
    expression: &Expr,
    css_names: &HashSet<String>,
    context: &mut DerivedValueContext,
) -> String {
    match expression {
        Expr::Lit(Lit::Num(value)) => format_number(value.value),
        Expr::Lit(Lit::Bool(value)) => value.value.to_string(),
        Expr::Lit(Lit::Null(_)) => "null".to_string(),
        expression => compile_derived_string_expression(expression, css_names, context),
    }
}

fn compile_derived_template(
    template: &Tpl,
    css_names: &HashSet<String>,
    context: &mut DerivedValueContext,
) -> String {
    let mut value = String::new();
    for (index, quasi) in template.quasis.iter().enumerate() {
        value.push_str(
            quasi
                .cooked
                .as_ref()
                .and_then(|cooked| cooked.as_str())
                .unwrap_or_else(|| quasi.raw.as_str()),
        );
        if let Some(expression) = template.exprs.get(index) {
            value.push_str(&compile_derived_interpolation_expression(
                expression, css_names, context,
            ));
        }
    }
    value
}

fn resolve_derived_member(member: &MemberExpr, context: &mut DerivedValueContext) -> String {
    if let Some(value) = resolve_constant_token(&Expr::Member(member.clone()), context.const_groups)
    {
        return const_value_to_string(&value);
    }

    let Expr::Ident(group_name) = &*member.obj else {
        panic!(
            "[nanocss] css.defineVars(...) function values can only reference same-file css.defineVars or css.defineConsts tokens."
        );
    };
    let Some(token_name) = member_token_name(member) else {
        panic!("[nanocss] css.defineVars(...) function value member references must be static.");
    };
    let group_name = group_name.sym.to_string();

    if group_name == context.group_name {
        let Some(custom_property_name) = context.self_group.get(&token_name) else {
            panic!(
                "[nanocss] Unknown same-group reference \"{}\" in css.defineVars(...) function value.",
                token_name
            );
        };
        if !context.dependencies.contains(&token_name) {
            context.dependencies.push(token_name);
        }
        return create_local_css_variable_value_string(custom_property_name, context.debug);
    }

    if context.imported_variable_group_names.contains(&group_name) {
        panic!(
            "[nanocss] css.defineVars(...) function values cannot reference imported variable groups."
        );
    }

    if let Some(group) = context.variable_groups.get(&group_name) {
        let Some(custom_property_name) = group.get(&token_name) else {
            panic!(
                "[nanocss] \"{}\" is not defined in the variable group referenced by css.defineVars(...).",
                token_name
            );
        };
        return create_local_css_variable_value_string(custom_property_name, context.debug);
    }

    panic!(
        "[nanocss] css.defineVars(...) function values can only reference same-file css.defineVars or css.defineConsts tokens."
    );
}

fn member_token_name(member: &MemberExpr) -> Option<String> {
    match &member.prop {
        MemberProp::Ident(token_name) => Some(token_name.sym.to_string()),
        MemberProp::Computed(computed) => match &*computed.expr {
            Expr::Lit(Lit::Str(token_name)) => token_name.value.as_str().map(ToString::to_string),
            _ => None,
        },
        _ => None,
    }
}

fn create_local_css_variable_value_string(custom_property_name: &str, debug: bool) -> String {
    format!(
        "var({}, var({}))",
        custom_property_name,
        create_variable_default_name(custom_property_name, debug)
    )
}

fn assert_no_define_vars_cycles(
    dependency_map: &HashMap<String, Vec<String>>,
    token_order: &[String],
) {
    fn visit(
        key: &str,
        dependency_map: &HashMap<String, Vec<String>>,
        visited: &mut HashSet<String>,
        in_stack: &mut HashSet<String>,
        stack: &mut Vec<String>,
    ) {
        if in_stack.contains(key) {
            let start = stack.iter().position(|value| value == key).unwrap_or(0);
            let mut cycle = stack[start..].to_vec();
            cycle.push(key.to_string());
            panic!(
                "[nanocss] Cyclic same-group references in css.defineVars(...) are not allowed: {}.",
                cycle.join(" -> ")
            );
        }
        if visited.contains(key) {
            return;
        }

        visited.insert(key.to_string());
        in_stack.insert(key.to_string());
        stack.push(key.to_string());

        for dependency in dependency_map.get(key).into_iter().flatten() {
            if dependency_map.contains_key(dependency) {
                visit(dependency, dependency_map, visited, in_stack, stack);
            }
        }

        stack.pop();
        in_stack.remove(key);
    }

    let mut visited = HashSet::new();
    let mut in_stack = HashSet::new();
    let mut stack = Vec::new();
    for key in token_order {
        visit(key, dependency_map, &mut visited, &mut in_stack, &mut stack);
    }
}

fn parse_generated_string_var_value(
    name: &str,
    generated_string_names: &HashMap<String, GeneratedString>,
) -> VariableValue {
    match generated_string_names.get(name) {
        Some(value) if value.is_css_identifier() => VariableValue::String(value.value.clone()),
        Some(_) => panic!(
            "[nanocss] css.defineVars(...) can only store generated css.keyframes(...) or css.positionTry(...) strings."
        ),
        None => {
            panic!("[nanocss] css.defineVars(...) failed to compile a variable fallback.")
        }
    }
}

fn parse_typed_variable_value(
    expression: &Expr,
    css_names: &HashSet<String>,
) -> Option<TypedVariableValue> {
    let Expr::Call(call) = expression else {
        return None;
    };
    let Some(syntax) = css_type_call_syntax(&call.callee, css_names) else {
        return None;
    };
    if call.args.len() != 1 || call.args[0].spread.is_some() {
        panic!("[nanocss] css.types.*(...) must be called with exactly one static argument.");
    }
    let value = parse_typed_define_var_value(syntax, &call.args[0].expr);
    let initial_value = initial_value_for_typed_variable(&value);
    Some(TypedVariableValue {
        syntax,
        initial_value,
        value: Box::new(value),
    })
}

pub(crate) fn unwrap_css_type_expression(
    expression: &Expr,
    css_names: &HashSet<String>,
) -> Option<Expr> {
    let Expr::Call(call) = expression else {
        return None;
    };
    let Some(syntax) = css_type_call_syntax(&call.callee, css_names) else {
        return None;
    };
    if call.args.len() != 1 || call.args[0].spread.is_some() {
        panic!("[nanocss] css.types.*(...) must be called with exactly one static argument.");
    }
    Some(convert_typed_expression(syntax, &call.args[0].expr))
}

fn css_type_call_syntax(callee: &Callee, css_names: &HashSet<String>) -> Option<&'static str> {
    let Callee::Expr(callee) = callee else {
        return None;
    };
    let Expr::Member(type_member) = &**callee else {
        return None;
    };
    let type_name = type_member.prop.as_ident()?.sym.as_ref();
    let Expr::Member(types_member) = &*type_member.obj else {
        return None;
    };
    if !types_member.prop.is_ident_with("types") {
        return None;
    }
    let Expr::Ident(css_name) = &*types_member.obj else {
        return None;
    };
    if !css_names.contains(&css_name.sym.to_string()) {
        return None;
    }

    match type_name {
        "angle" => Some("<angle>"),
        "color" => Some("<color>"),
        "url" => Some("<url>"),
        "image" => Some("<image>"),
        "integer" => Some("<integer>"),
        "lengthPercentage" => Some("<length-percentage>"),
        "length" => Some("<length>"),
        "percentage" => Some("<percentage>"),
        "number" => Some("<number>"),
        "resolution" => Some("<resolution>"),
        "time" => Some("<time>"),
        "transformFunction" => Some("<transform-function>"),
        "transformList" => Some("<transform-list>"),
        _ => None,
    }
}

fn parse_typed_define_var_value(syntax: &'static str, expression: &Expr) -> VariableValue {
    match expression {
        Expr::Lit(Lit::Str(value)) => VariableValue::String(
            value
                .value
                .as_str()
                .map(ToString::to_string)
                .unwrap_or_default(),
        ),
        Expr::Lit(Lit::Num(value)) => {
            VariableValue::String(format_typed_number(syntax, value.value))
        }
        Expr::Object(object) => {
            VariableValue::Hook(parse_typed_variable_hook_object(syntax, object))
        }
        _ => panic!(
            "[nanocss] css.types.*(...) values must be static string, number, or hook objects."
        ),
    }
}

fn parse_typed_variable_hook_object(
    syntax: &'static str,
    object: &swc_core::ecma::ast::ObjectLit,
) -> HookValue {
    let mut entries = Vec::new();
    let mut has_default = false;

    for property in &object.props {
        let PropOrSpread::Prop(property) = property else {
            panic!("[nanocss] Hook objects cannot contain spreads.");
        };
        let Prop::KeyValue(property) = &**property else {
            panic!("[nanocss] Hook object values must be expressions.");
        };
        let Some(key) = prop_name_to_string(&property.key) else {
            panic!("[nanocss] Hook object keys must be statically known.");
        };
        if !is_hook_key(&key) || (key != "default" && !is_hook_name(&key)) {
            panic!("[nanocss] Nested variable values must use declared hooks or default.");
        }
        if key == "default" {
            has_default = true;
        }
        entries.push((key, parse_typed_hook_value(syntax, &property.value)));
    }

    if !has_default {
        panic!("[nanocss] Hook objects must include a default value.");
    }
    HookValue::Object(entries)
}

fn parse_typed_hook_value(syntax: &'static str, expression: &Expr) -> HookValue {
    match expression {
        Expr::Lit(Lit::Str(value)) => HookValue::String(
            value
                .value
                .as_str()
                .map(ToString::to_string)
                .unwrap_or_default(),
        ),
        Expr::Lit(Lit::Num(value)) => HookValue::String(format_typed_number(syntax, value.value)),
        Expr::Object(object) => parse_typed_variable_hook_object(syntax, object),
        _ => panic!("[nanocss] css.types.*(...) hook values must be static strings or numbers."),
    }
}

fn initial_value_for_typed_variable(value: &VariableValue) -> String {
    match value {
        VariableValue::String(value) => value.clone(),
        VariableValue::Boolean(_) | VariableValue::Null => {
            panic!("[nanocss] css.types.*(...) initial values must be strings or numbers.")
        }
        VariableValue::Hook(value) => initial_value_for_typed_hook(value),
        VariableValue::Typed(_) => {
            panic!("[nanocss] css.types.*(...) calls cannot be nested.")
        }
    }
}

fn initial_value_for_typed_hook(value: &HookValue) -> String {
    match value {
        HookValue::String(value) => value.clone(),
        HookValue::Object(entries) => entries
            .iter()
            .find_map(|(key, value)| {
                if key == "default" {
                    Some(initial_value_for_typed_hook(value))
                } else {
                    None
                }
            })
            .expect("typed hook values must include a default"),
        _ => panic!("[nanocss] css.types.*(...) initial values must be strings or numbers."),
    }
}

fn convert_typed_expression(syntax: &'static str, expression: &Expr) -> Expr {
    match expression {
        Expr::Lit(Lit::Num(value)) => Expr::Lit(Lit::Str(Str {
            span: DUMMY_SP,
            value: format_typed_number(syntax, value.value).into(),
            raw: None,
        })),
        Expr::Object(object) => {
            let mut object = object.clone();
            for property in &mut object.props {
                let PropOrSpread::Prop(property) = property else {
                    panic!("[nanocss] Hook objects cannot contain spreads.");
                };
                let Prop::KeyValue(property) = &mut **property else {
                    panic!("[nanocss] Hook object values must be expressions.");
                };
                property.value = Box::new(convert_typed_expression(syntax, &property.value));
            }
            Expr::Object(object)
        }
        Expr::Lit(Lit::Str(_)) => expression.clone(),
        _ => panic!(
            "[nanocss] css.types.*(...) values must be static string, number, or hook objects."
        ),
    }
}

fn format_typed_number(syntax: &str, value: f64) -> String {
    let formatted = format_number(value);
    match syntax {
        "<length>" | "<length-percentage>" if value != 0.0 => format!("{formatted}px"),
        _ => formatted,
    }
}

pub(crate) fn parse_create_theme_overrides_arg(
    expression: &Expr,
    css_names: &HashSet<String>,
) -> Vec<(String, Expr)> {
    let Expr::Object(object) = expression else {
        panic!("[nanocss] css.createTheme(...) overrides must be a static object expression.");
    };

    let mut overrides = Vec::new();
    for property in &object.props {
        let PropOrSpread::Prop(property) = property else {
            panic!("[nanocss] css.createTheme(...) override objects cannot contain spreads.");
        };
        let Prop::KeyValue(property) = &**property else {
            panic!("[nanocss] css.createTheme(...) values must be expressions.");
        };
        let Some(token_name) = prop_name_to_string(&property.key) else {
            panic!("[nanocss] css.createTheme(...) override keys must be statically known.");
        };
        let value = unwrap_css_type_expression(&property.value, css_names)
            .unwrap_or_else(|| (*property.value).clone());
        reject_numeric_theme_override(&value);
        let value = match value {
            Expr::Lit(Lit::Null(_)) => create_undefined_expression(),
            value => value,
        };
        overrides.push((token_name, value));
    }

    overrides
}

fn create_undefined_expression() -> Expr {
    Expr::Unary(UnaryExpr {
        span: DUMMY_SP,
        op: UnaryOp::Void,
        arg: Box::new(Expr::Lit(Lit::Num(Number {
            span: DUMMY_SP,
            value: 0.0,
            raw: None,
        }))),
    })
}

fn reject_numeric_theme_override(expression: &Expr) {
    match expression {
        Expr::Lit(Lit::Num(_)) => {
            panic!(
                "[nanocss] css.createTheme(...) numeric overrides are not supported. Use strings such as \"4px\" or \"0.5\" instead."
            );
        }
        Expr::Object(object) => {
            for property in &object.props {
                let PropOrSpread::Prop(property) = property else {
                    continue;
                };
                let Prop::KeyValue(property) = &**property else {
                    continue;
                };
                reject_numeric_theme_override(&property.value);
            }
        }
        _ => {}
    }
}

pub(crate) fn create_variable_default_name(custom_property_name: &str, debug: bool) -> String {
    let suffix = if debug { "--n-default" } else { "--nd" };
    format!("{custom_property_name}{suffix}")
}

pub(crate) fn create_generated_variable_name(
    file_identity: &str,
    group_name: &str,
    token_name: &str,
    debug: bool,
) -> String {
    if token_name.starts_with("--") {
        return token_name.to_string();
    }

    let hash = hash(&format!("{file_identity}:{group_name}.{token_name}"));
    if debug {
        return format!(
            "--_nanocss_var_{}_{}_{}",
            debug_variable_name_fragment(group_name),
            debug_variable_name_fragment(token_name),
            hash
        );
    }

    format!("--nv-{hash}")
}

fn debug_variable_name_fragment(value: &str) -> String {
    let mut fragment = String::new();
    let mut previous_was_separator = true;
    let mut previous_was_lowercase_or_digit = false;

    for character in value.chars() {
        if character.is_ascii_alphanumeric() {
            if character.is_ascii_uppercase() {
                if !fragment.is_empty()
                    && !previous_was_separator
                    && previous_was_lowercase_or_digit
                {
                    fragment.push('-');
                }
                fragment.push(character.to_ascii_lowercase());
                previous_was_lowercase_or_digit = false;
            } else {
                fragment.push(character.to_ascii_lowercase());
                previous_was_lowercase_or_digit =
                    character.is_ascii_lowercase() || character.is_ascii_digit();
            }
            previous_was_separator = false;
        } else if !fragment.is_empty() && !previous_was_separator {
            fragment.push('-');
            previous_was_separator = true;
            previous_was_lowercase_or_digit = false;
        }
    }

    let fragment = fragment.trim_matches('-');
    if fragment.is_empty() {
        "token".to_string()
    } else {
        fragment.to_string()
    }
}

pub(crate) fn compile_define_vars(
    group_name: &str,
    tokens: &VariableTokens,
    file_identity: &str,
    hook_compiler: &mut HookCompiler,
    debug: bool,
) -> Vec<CompiledVariableToken> {
    tokens
        .iter()
        .map(|(token_name, value)| {
            let custom_property_name =
                create_generated_variable_name(file_identity, group_name, token_name, debug);
            let default_property_name = create_variable_default_name(&custom_property_name, debug);
            let default_value =
                compile_variable_default_value(&custom_property_name, value, hook_compiler);
            let mut defaults = default_value
                .map(|value| {
                    vec![CompiledVariableDefault {
                        custom_property_name: default_property_name.clone(),
                        value,
                    }]
                })
                .unwrap_or_default();
            let property = match value {
                VariableValue::Typed(value) => {
                    defaults.push(CompiledVariableDefault {
                        custom_property_name: custom_property_name.clone(),
                        value: format!("var({default_property_name})"),
                    });
                    Some(CompiledVariableProperty {
                        custom_property_name: custom_property_name.clone(),
                        syntax: value.syntax,
                        initial_value: value.initial_value.clone(),
                    })
                }
                _ => None,
            };

            CompiledVariableToken {
                token_name: token_name.clone(),
                custom_property_name,
                defaults,
                property,
            }
        })
        .collect()
}

fn compile_variable_default_value(
    custom_property_name: &str,
    value: &VariableValue,
    hook_compiler: &mut HookCompiler,
) -> Option<String> {
    match value {
        VariableValue::String(value) => Some(value.clone()),
        VariableValue::Boolean(value) => Some(value.to_string()),
        VariableValue::Null => None,
        VariableValue::Hook(value) => {
            let Some((_, value)) = hook_compiler
                .compile_property(custom_property_name, value)
                .into_iter()
                .next()
            else {
                return None;
            };
            Some(value)
        }
        VariableValue::Typed(value) => {
            compile_variable_default_value(custom_property_name, &value.value, hook_compiler)
        }
    }
}

fn parse_variable_hook_object(object: &swc_core::ecma::ast::ObjectLit) -> HookValue {
    let mut entries = Vec::new();
    let mut has_default = false;

    for property in &object.props {
        let PropOrSpread::Prop(property) = property else {
            panic!("[nanocss] Hook objects cannot contain spreads.");
        };
        let Prop::KeyValue(property) = &**property else {
            panic!("[nanocss] Hook object values must be expressions.");
        };
        let Some(key) = prop_name_to_string(&property.key) else {
            panic!("[nanocss] Hook object keys must be statically known.");
        };
        if !is_hook_key(&key) || (key != "default" && !is_hook_name(&key)) {
            panic!("[nanocss] Nested variable values must use declared hooks or default.");
        }
        if key == "default" {
            has_default = true;
        }
        entries.push((key, parse_variable_hook_value(&property.value)));
    }

    if !has_default {
        panic!("[nanocss] Hook objects must include a default value.");
    }
    HookValue::Object(entries)
}

fn parse_variable_hook_value(expression: &Expr) -> HookValue {
    match expression {
        Expr::Lit(Lit::Str(value)) => HookValue::String(
            value
                .value
                .as_str()
                .map(ToString::to_string)
                .unwrap_or_default(),
        ),
        Expr::Lit(Lit::Bool(value)) => HookValue::Boolean(value.value),
        Expr::Lit(Lit::Null(_)) => HookValue::Null,
        Expr::Object(object) => parse_variable_hook_object(object),
        Expr::Lit(Lit::Num(_)) => panic!(
            "[nanocss] css.defineVars(...) numeric defaults are not supported. Use strings such as \"4px\" or \"0.5\" instead."
        ),
        _ => panic!("[nanocss] css.defineVars(...) hook values must be static literals."),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compiles_generated_variable_names_and_defaults() {
        let mut hook_compiler = HookCompiler::new(true);
        let tokens = compile_define_vars(
            "colors",
            &vec![(
                "primary".to_string(),
                VariableValue::String("green".to_string()),
            )],
            "src/tokens.css.ts",
            &mut hook_compiler,
            true,
        );

        assert_eq!(tokens[0].token_name, "primary");
        assert_eq!(
            tokens[0].custom_property_name,
            "--_nanocss_var_colors_primary_vec0x7"
        );
        assert_eq!(
            tokens[0].defaults,
            vec![CompiledVariableDefault {
                custom_property_name: "--_nanocss_var_colors_primary_vec0x7--n-default".to_string(),
                value: "green".to_string(),
            }]
        );
    }

    #[test]
    fn debug_variable_names_include_readable_group_and_token_names() {
        assert_eq!(
            create_generated_variable_name(
                "src/tokens.css.ts",
                "themeTokens",
                "surfaceMuted",
                true
            ),
            "--_nanocss_var_theme-tokens_surface-muted_vhde28"
        );
    }

    #[test]
    fn production_variable_names_stay_compact() {
        assert_eq!(
            create_generated_variable_name("src/tokens.css.ts", "colors", "primary", false),
            "--nv-vec0x7"
        );
    }

    #[test]
    fn keeps_explicit_custom_property_names() {
        let mut hook_compiler = HookCompiler::new(false);
        let tokens = compile_define_vars(
            "colors",
            &vec![(
                "--brand".to_string(),
                VariableValue::String("red".to_string()),
            )],
            "src/tokens.css.ts",
            &mut hook_compiler,
            false,
        );

        assert_eq!(tokens[0].custom_property_name, "--brand");
        assert_eq!(tokens[0].defaults[0].custom_property_name, "--brand--nd");
    }
}
