use std::collections::HashSet;

use swc_core::{
    common::DUMMY_SP,
    ecma::ast::{
        Callee, ComputedPropName, Expr, IdentName, Lit, MemberExpr, MemberProp, PropName, Str,
    },
};

pub(crate) fn is_css_member_call(
    css_names: &HashSet<String>,
    callee: &Callee,
    member_name: &str,
) -> bool {
    let Callee::Expr(callee) = callee else {
        return false;
    };
    let Expr::Member(member) = &**callee else {
        return false;
    };
    let Expr::Ident(object) = &*member.obj else {
        return false;
    };
    css_names.contains(&object.sym.to_string()) && member.prop.is_ident_with(member_name)
}

pub(crate) fn prop_name_to_string(name: &PropName) -> Option<String> {
    match name {
        PropName::Ident(ident) => Some(ident.sym.to_string()),
        PropName::Str(str_) => str_.value.as_str().map(ToString::to_string),
        PropName::Num(number) => Some(format_number(number.value)),
        _ => None,
    }
}

pub(crate) fn format_number(value: f64) -> String {
    if !value.is_finite() {
        panic!("[nanocss] Numeric CSS values must be finite.");
    }
    if value.fract() == 0.0 {
        format!("{}", value as i64)
    } else {
        value.to_string()
    }
}

pub(crate) fn css_property_name(property_name: &str) -> String {
    if property_name.starts_with("--") {
        return property_name.to_string();
    }

    let mut name = String::new();
    for character in property_name.chars() {
        if character.is_ascii_uppercase() {
            name.push('-');
            name.push(character.to_ascii_lowercase());
        } else {
            name.push(character);
        }
    }
    if name.starts_with("ms-") {
        name.insert(0, '-');
    }
    name
}

#[cfg(test)]
mod tests {
    use super::{css_property_name, format_number};

    #[test]
    #[should_panic(expected = "[nanocss] Numeric CSS values must be finite.")]
    fn rejects_non_finite_numbers() {
        format_number(f64::NAN);
    }

    #[test]
    fn hyphenates_css_property_names() {
        assert_eq!(css_property_name("backgroundColor"), "background-color");
        assert_eq!(css_property_name("WebkitTransform"), "-webkit-transform");
        assert_eq!(css_property_name("msTransform"), "-ms-transform");
        assert_eq!(css_property_name("--progress"), "--progress");
    }
}

pub(crate) fn create_property_member_expr(object: Expr, property: &str) -> Expr {
    let is_identifier = property.chars().next().is_some_and(|character| {
        character == '_' || character == '$' || character.is_ascii_alphabetic()
    }) && property
        .chars()
        .all(|character| character == '_' || character == '$' || character.is_ascii_alphanumeric());

    Expr::Member(MemberExpr {
        span: DUMMY_SP,
        obj: Box::new(object),
        prop: if is_identifier {
            MemberProp::Ident(IdentName {
                span: DUMMY_SP,
                sym: property.into(),
            })
        } else {
            MemberProp::Computed(ComputedPropName {
                span: DUMMY_SP,
                expr: Box::new(Expr::Lit(Lit::Str(Str {
                    span: DUMMY_SP,
                    value: property.into(),
                    raw: None,
                }))),
            })
        },
    })
}
