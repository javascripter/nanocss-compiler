use swc_core::{
    common::DUMMY_SP,
    ecma::ast::{
        AssignExpr, AssignOp, AssignTarget, BindingIdent, Bool, Expr, Ident, JSXAttr, JSXAttrName,
        JSXAttrOrSpread, JSXAttrValue, JSXElementName, JSXExpr, JSXExprContainer, JSXObject, Lit,
        MemberExpr, MemberProp, Null, Number, ObjectLit, OptChainBase, OptChainExpr, Prop,
        PropName, PropOrSpread, SimpleAssignTarget, SpreadElement, Str,
    },
};

use crate::jsx::{get_jsx_attribute_expression, is_jsx_style_attr};
use crate::options::HtmlDefaults;
use crate::props::append_style_spreads_from_expr_with_resolver;

pub(crate) fn html_default_style_id(
    tag_name: &str,
    html_defaults: &HtmlDefaults,
) -> Option<String> {
    html_default_style(tag_name, html_defaults)
        .map(|_| format!("_html{}DefaultStyle", capitalize(tag_name)))
}

pub(crate) fn html_default_style(
    tag_name: &str,
    html_defaults: &HtmlDefaults,
) -> Option<ObjectLit> {
    let properties = html_defaults.get(tag_name)?;
    Some(create_style_from_json(tag_name, properties))
}

pub(crate) fn get_html_tag_name(
    html_names: &std::collections::HashSet<String>,
    name: &JSXElementName,
) -> Option<String> {
    let JSXElementName::JSXMemberExpr(member) = name else {
        return None;
    };
    let JSXObject::Ident(object) = &member.obj else {
        return None;
    };
    if !html_names.contains(&object.sym.to_string()) {
        return None;
    }
    Some(member.prop.sym.to_string())
}

pub(crate) fn create_jsx_element_name(tag_name: &str) -> JSXElementName {
    JSXElementName::Ident(swc_core::ecma::ast::Ident::new(
        tag_name.into(),
        DUMMY_SP,
        Default::default(),
    ))
}

pub(crate) fn apply_html_default_style(
    attributes: &mut Vec<JSXAttrOrSpread>,
    default_style_id: Option<&str>,
    resolve_style: &mut impl FnMut(&Expr) -> Option<Expr>,
    create_spread_temp: &mut impl FnMut() -> String,
) {
    let default_style =
        default_style_id.map(|id| Expr::Ident(Ident::new(id.into(), DUMMY_SP, Default::default())));
    let mut style_properties = default_style
        .clone()
        .map(|default_style| {
            vec![PropOrSpread::Spread(SpreadElement {
                dot3_token: DUMMY_SP,
                expr: Box::new(default_style),
            })]
        })
        .unwrap_or_default();
    let mut next_attributes = Vec::with_capacity(attributes.len() + 1);

    for attribute in attributes.drain(..) {
        match attribute {
            JSXAttrOrSpread::JSXAttr(attribute) if is_jsx_style_attr(&attribute) => {
                if let Some(style) = get_jsx_attribute_expression(&attribute) {
                    append_style_spreads_from_expr_with_resolver(
                        &mut style_properties,
                        style,
                        resolve_style,
                    );
                }
            }
            JSXAttrOrSpread::SpreadElement(mut spread) => {
                let style_object = if needs_html_spread_temp(&spread.expr) {
                    let temp = create_spread_temp();
                    let original = (*spread.expr).clone();
                    spread.expr = Box::new(create_assignment_expr(&temp, original));
                    Expr::Ident(Ident::new(temp.into(), DUMMY_SP, Default::default()))
                } else {
                    (*spread.expr).clone()
                };
                style_properties.push(PropOrSpread::Spread(SpreadElement {
                    dot3_token: DUMMY_SP,
                    expr: Box::new(create_optional_style_member_expr(style_object)),
                }));
                next_attributes.push(JSXAttrOrSpread::SpreadElement(spread));
            }
            attribute => next_attributes.push(attribute),
        }
    }

    if style_properties.is_empty() {
        *attributes = next_attributes;
        return;
    }

    let style = if style_properties.len() == 1 {
        match style_properties.remove(0) {
            PropOrSpread::Spread(spread) => *spread.expr,
            prop => Expr::Object(ObjectLit {
                span: DUMMY_SP,
                props: vec![prop],
            }),
        }
    } else {
        Expr::Object(ObjectLit {
            span: DUMMY_SP,
            props: style_properties,
        })
    };
    next_attributes.push(create_jsx_style_attribute(style));
    *attributes = next_attributes;
}

pub(crate) fn html_spread_temp_count(attributes: &[JSXAttrOrSpread]) -> usize {
    attributes
        .iter()
        .filter(|attribute| {
            matches!(
                attribute,
                JSXAttrOrSpread::SpreadElement(spread) if needs_html_spread_temp(&spread.expr)
            )
        })
        .count()
}

fn needs_html_spread_temp(expression: &Expr) -> bool {
    !matches!(expression, Expr::Ident(_))
}

fn create_assignment_expr(name: &str, value: Expr) -> Expr {
    Expr::Assign(AssignExpr {
        span: DUMMY_SP,
        op: AssignOp::Assign,
        left: AssignTarget::Simple(SimpleAssignTarget::Ident(BindingIdent::from(Ident::new(
            name.into(),
            DUMMY_SP,
            Default::default(),
        )))),
        right: Box::new(value),
    })
}

fn create_jsx_style_attribute(style: Expr) -> JSXAttrOrSpread {
    JSXAttrOrSpread::JSXAttr(JSXAttr {
        span: DUMMY_SP,
        name: JSXAttrName::Ident("style".into()),
        value: Some(JSXAttrValue::JSXExprContainer(JSXExprContainer {
            span: DUMMY_SP,
            expr: JSXExpr::Expr(Box::new(style)),
        })),
    })
}

fn create_optional_style_member_expr(object: Expr) -> Expr {
    Expr::OptChain(OptChainExpr {
        span: DUMMY_SP,
        optional: true,
        base: Box::new(OptChainBase::Member(MemberExpr {
            span: DUMMY_SP,
            obj: Box::new(object),
            prop: MemberProp::Ident("style".into()),
        })),
    })
}

fn create_style_from_json(
    tag_name: &str,
    properties: &std::collections::BTreeMap<String, serde_json::Value>,
) -> ObjectLit {
    ObjectLit {
        span: DUMMY_SP,
        props: properties
            .iter()
            .map(|(name, value)| {
                PropOrSpread::Prop(Box::new(Prop::KeyValue(
                    swc_core::ecma::ast::KeyValueProp {
                        key: create_html_default_property_name(name),
                        value: Box::new(html_default_value_expr(tag_name, name, value)),
                    },
                )))
            })
            .collect(),
    }
}

fn create_html_default_property_name(name: &str) -> PropName {
    if is_identifier(name) {
        PropName::Ident(name.into())
    } else {
        PropName::Str(Str {
            span: DUMMY_SP,
            value: name.into(),
            raw: None,
        })
    }
}

fn is_identifier(value: &str) -> bool {
    value.chars().next().is_some_and(|character| {
        character == '_' || character == '$' || character.is_ascii_alphabetic()
    }) && value
        .chars()
        .all(|character| character == '_' || character == '$' || character.is_ascii_alphanumeric())
}

fn html_default_value_expr(tag_name: &str, property_name: &str, value: &serde_json::Value) -> Expr {
    match value {
        serde_json::Value::String(value) => Expr::Lit(Lit::Str(Str {
            span: DUMMY_SP,
            value: value.clone().into(),
            raw: None,
        })),
        serde_json::Value::Number(value) => {
            let Some(value) = value.as_f64().filter(|value| value.is_finite()) else {
                panic!(
                    "[nanocss] htmlDefaults.{tag_name}.{property_name} must be a finite number."
                );
            };
            Expr::Lit(Lit::Num(Number {
                span: DUMMY_SP,
                value,
                raw: None,
            }))
        }
        serde_json::Value::Bool(value) => Expr::Lit(Lit::Bool(Bool {
            span: DUMMY_SP,
            value: *value,
        })),
        serde_json::Value::Null => Expr::Lit(Lit::Null(Null { span: DUMMY_SP })),
        _ => panic!(
            "[nanocss] htmlDefaults.{tag_name}.{property_name} must be a string, number, boolean, or null."
        ),
    }
}

fn capitalize(value: &str) -> String {
    let mut chars = value.chars();
    let Some(first) = chars.next() else {
        return String::new();
    };
    first.to_uppercase().chain(chars).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    #[test]
    fn returns_configured_default_styles() {
        let mut defaults = BTreeMap::new();
        defaults.insert(
            "div".to_string(),
            BTreeMap::from([
                (
                    "boxSizing".to_string(),
                    serde_json::Value::String("border-box".to_string()),
                ),
                ("marginTop".to_string(), serde_json::Value::Number(0.into())),
            ]),
        );

        assert_eq!(html_default_style("div", &defaults).unwrap().props.len(), 2);
    }

    #[test]
    fn returns_none_when_default_style_is_missing() {
        let defaults = BTreeMap::new();
        assert!(html_default_style("span", &defaults).is_none());
    }
}
