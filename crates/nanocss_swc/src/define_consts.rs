use std::collections::BTreeMap;

use swc_core::{
    common::DUMMY_SP,
    ecma::ast::{Expr, Lit, Number, Prop, PropName, PropOrSpread, Str},
};

use crate::ast::{format_number, prop_name_to_string};

#[derive(Clone, Debug, PartialEq)]
pub enum ConstValue {
    String(String),
    Number(f64),
}

pub type ConstGroup = BTreeMap<String, ConstValue>;
pub type ConstGroups = BTreeMap<String, ConstGroup>;

pub(crate) fn parse_define_consts_arg(expression: &Expr) -> ConstGroup {
    let Expr::Object(object) = expression else {
        panic!("[nanocss] css.defineConsts(...) must be called with a static object expression.");
    };

    let mut constants = BTreeMap::new();
    for property in &object.props {
        let PropOrSpread::Prop(property) = property else {
            panic!("[nanocss] css.defineConsts(...) objects cannot contain spreads.");
        };
        let Prop::KeyValue(property) = &**property else {
            panic!("[nanocss] css.defineConsts(...) values must be expressions.");
        };
        let Some(name) = prop_name_to_string(&property.key) else {
            panic!("[nanocss] css.defineConsts(...) keys must be statically known.");
        };
        let value = match &*property.value {
            Expr::Lit(Lit::Str(value)) => ConstValue::String(
                value
                    .value
                    .as_str()
                    .map(ToString::to_string)
                    .unwrap_or_default(),
            ),
            Expr::Lit(Lit::Num(value)) => {
                if !value.value.is_finite() {
                    panic!("[nanocss] css.defineConsts(...) numeric values must be finite.");
                }
                ConstValue::Number(value.value)
            }
            _ => panic!(
                "[nanocss] css.defineConsts(...) values must be static string or number literals."
            ),
        };
        constants.insert(name, value);
    }

    constants
}

pub(crate) fn resolve_constant_token(
    expression: &Expr,
    const_groups: &ConstGroups,
) -> Option<ConstValue> {
    let Expr::Member(member) = expression else {
        return None;
    };
    let Expr::Ident(group_name) = &*member.obj else {
        return None;
    };
    let token_name = match &member.prop {
        swc_core::ecma::ast::MemberProp::Ident(token_name) => token_name.sym.to_string(),
        swc_core::ecma::ast::MemberProp::Computed(computed) => match &*computed.expr {
            Expr::Lit(Lit::Str(token_name)) => token_name.value.as_str()?.to_string(),
            _ => return None,
        },
        _ => return None,
    };
    const_groups
        .get(&group_name.sym.to_string())
        .and_then(|group| group.get(&token_name))
        .cloned()
}

pub(crate) fn const_value_to_expression(value: &ConstValue) -> Expr {
    match value {
        ConstValue::String(value) => Expr::Lit(Lit::Str(Str {
            span: DUMMY_SP,
            value: value.clone().into(),
            raw: None,
        })),
        ConstValue::Number(value) => Expr::Lit(Lit::Num(Number {
            span: DUMMY_SP,
            value: *value,
            raw: None,
        })),
    }
}

pub(crate) fn const_value_to_property_name(value: &ConstValue) -> PropName {
    match value {
        ConstValue::String(value) => PropName::Str(Str {
            span: DUMMY_SP,
            value: value.clone().into(),
            raw: None,
        }),
        ConstValue::Number(value) => PropName::Num(Number {
            span: DUMMY_SP,
            value: *value,
            raw: None,
        }),
    }
}

pub(crate) fn const_value_to_string(value: &ConstValue) -> String {
    match value {
        ConstValue::String(value) => value.clone(),
        ConstValue::Number(value) => format_number(*value),
    }
}
