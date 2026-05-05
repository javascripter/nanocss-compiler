use std::collections::HashSet;

use serde_json::{Map, Value};
use swc_core::{
    common::DUMMY_SP,
    ecma::ast::{
        ArrayLit, ArrowExpr, BinExpr, BlockStmtOrExpr, CallExpr, Callee, ComputedPropName,
        CondExpr, Expr, ExprOrSpread, Lit, MemberExpr, MemberProp, Number, ObjectLit, ParenExpr,
        Pat, Prop, PropName, PropOrSpread, Str, Tpl, UnaryExpr,
    },
};

pub(crate) fn replace_env_references(
    expression: &mut Expr,
    css_names: &HashSet<String>,
    env: &Value,
) {
    replace_env_references_with_names(expression, css_names, env);
}

fn replace_env_references_with_names(
    expression: &mut Expr,
    css_names: &HashSet<String>,
    env: &Value,
) {
    if let Expr::Member(member) = expression
        && let Some(value) = resolve_env_member(member, css_names, env)
    {
        *expression = env_value_to_expression(value);
        return;
    }

    match expression {
        Expr::Object(object) => replace_env_references_in_object(object, css_names, env),
        Expr::Array(array) => {
            for element in array.elems.iter_mut().flatten() {
                replace_env_references_with_names(&mut element.expr, css_names, env);
            }
        }
        Expr::Call(call) => replace_env_references_in_call(call, css_names, env),
        Expr::Member(member) => {
            replace_env_references_with_names(&mut member.obj, css_names, env);
            if let MemberProp::Computed(ComputedPropName { expr, .. }) = &mut member.prop {
                replace_env_references_with_names(expr, css_names, env);
            }
        }
        Expr::Arrow(arrow) => replace_env_references_in_arrow(arrow, css_names, env),
        Expr::Bin(BinExpr { left, right, .. }) => {
            replace_env_references_with_names(left, css_names, env);
            replace_env_references_with_names(right, css_names, env);
        }
        Expr::Tpl(template) => replace_env_references_in_template(template, css_names, env),
        Expr::Paren(ParenExpr { expr, .. }) => {
            replace_env_references_with_names(expr, css_names, env);
        }
        Expr::Cond(CondExpr {
            test, cons, alt, ..
        }) => {
            replace_env_references_with_names(test, css_names, env);
            replace_env_references_with_names(cons, css_names, env);
            replace_env_references_with_names(alt, css_names, env);
        }
        Expr::Unary(UnaryExpr { arg, .. }) => {
            replace_env_references_with_names(arg, css_names, env);
        }
        _ => {}
    }
}

fn replace_env_references_in_object(
    object: &mut ObjectLit,
    css_names: &HashSet<String>,
    env: &Value,
) {
    for property in &mut object.props {
        let PropOrSpread::Prop(property) = property else {
            continue;
        };
        let Prop::KeyValue(property) = &mut **property else {
            continue;
        };
        if let PropName::Computed(ComputedPropName { expr, .. }) = &mut property.key {
            replace_env_references_with_names(expr, css_names, env);
            match &**expr {
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
                _ => {}
            }
        }
        replace_env_references_with_names(&mut property.value, css_names, env);
    }
}

fn replace_env_references_in_call(call: &mut CallExpr, css_names: &HashSet<String>, env: &Value) {
    if let Callee::Expr(callee) = &call.callee
        && let Expr::Member(member) = &**callee
        && env_member_path(member, css_names).is_some()
    {
        panic!("[nanocss] css.env values cannot be called.");
    }

    if let Callee::Expr(callee) = &mut call.callee {
        replace_env_references_with_names(callee, css_names, env);
    }
    for arg in &mut call.args {
        replace_env_references_with_names(&mut arg.expr, css_names, env);
    }
}

fn replace_env_references_in_arrow(
    arrow: &mut ArrowExpr,
    css_names: &HashSet<String>,
    env: &Value,
) {
    let mut css_names = css_names.clone();
    for param in &arrow.params {
        if let Pat::Ident(identifier) = param {
            let name = identifier.id.sym.to_string();
            css_names.remove(&name);
        }
    }
    if let BlockStmtOrExpr::Expr(body) = &mut *arrow.body {
        replace_env_references_with_names(body, &css_names, env);
    }
}

fn replace_env_references_in_template(
    template: &mut Tpl,
    css_names: &HashSet<String>,
    env: &Value,
) {
    for expression in &mut template.exprs {
        replace_env_references_with_names(expression, css_names, env);
    }
}

fn resolve_env_member<'a>(
    member: &MemberExpr,
    css_names: &HashSet<String>,
    env: &'a Value,
) -> Option<&'a Value> {
    let path = env_member_path(member, css_names)?;
    let mut value = env;
    for key in path {
        let Value::Object(object) = value else {
            panic!("[nanocss] css.env reference points through a non-object value.");
        };
        value = object
            .get(&key)
            .unwrap_or_else(|| panic!("[nanocss] Unknown css.env key \"{}\".", key));
    }
    Some(value)
}

fn env_member_path(member: &MemberExpr, css_names: &HashSet<String>) -> Option<Vec<String>> {
    let mut path = Vec::new();
    let mut current = member;

    loop {
        path.push(member_prop_name(&current.prop)?);
        match &*current.obj {
            Expr::Member(parent) => {
                if parent.prop.is_ident_with("env")
                    && let Expr::Ident(css_name) = &*parent.obj
                    && css_names.contains(&css_name.sym.to_string())
                {
                    path.reverse();
                    return Some(path);
                }
                current = parent;
            }
            _ => return None,
        }
    }
}

fn member_prop_name(prop: &MemberProp) -> Option<String> {
    match prop {
        MemberProp::Ident(identifier) => Some(identifier.sym.to_string()),
        MemberProp::Computed(ComputedPropName { expr, .. }) => match &**expr {
            Expr::Lit(Lit::Str(value)) => value.value.as_str().map(ToString::to_string),
            Expr::Lit(Lit::Num(value)) => Some(value.value.to_string()),
            _ => None,
        },
        _ => None,
    }
}

fn env_value_to_expression(value: &Value) -> Expr {
    match value {
        Value::String(value) => Expr::Lit(Lit::Str(Str {
            span: DUMMY_SP,
            value: value.clone().into(),
            raw: None,
        })),
        Value::Number(value) => Expr::Lit(Lit::Num(Number {
            span: DUMMY_SP,
            value: value
                .as_f64()
                .unwrap_or_else(|| panic!("[nanocss] css.env numeric values must be finite.")),
            raw: None,
        })),
        Value::Bool(value) => Expr::Lit(Lit::Bool(swc_core::ecma::ast::Bool {
            span: DUMMY_SP,
            value: *value,
        })),
        Value::Null => Expr::Lit(Lit::Null(swc_core::ecma::ast::Null { span: DUMMY_SP })),
        Value::Object(object) => Expr::Object(env_object_to_expression(object)),
        Value::Array(values) => Expr::Array(ArrayLit {
            span: DUMMY_SP,
            elems: values
                .iter()
                .map(|value| {
                    Some(ExprOrSpread {
                        spread: None,
                        expr: Box::new(env_value_to_expression(value)),
                    })
                })
                .collect(),
        }),
    }
}

fn env_object_to_expression(object: &Map<String, Value>) -> ObjectLit {
    ObjectLit {
        span: DUMMY_SP,
        props: object
            .iter()
            .map(|(key, value)| {
                PropOrSpread::Prop(Box::new(Prop::KeyValue(
                    swc_core::ecma::ast::KeyValueProp {
                        key: PropName::Str(Str {
                            span: DUMMY_SP,
                            value: key.clone().into(),
                            raw: None,
                        }),
                        value: Box::new(env_value_to_expression(value)),
                    },
                )))
            })
            .collect(),
    }
}
