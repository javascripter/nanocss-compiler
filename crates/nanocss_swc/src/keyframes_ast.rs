use swc_core::ecma::ast::{Expr, Lit, ObjectLit, Prop, PropOrSpread};

use crate::{
    ast::prop_name_to_string,
    keyframes::{KeyframesFrames, KeyframesValue},
};

pub(crate) fn parse_keyframes_arg(expression: &Expr) -> KeyframesFrames {
    let Expr::Object(object) = expression else {
        panic!("[nanocss] css.keyframes(...) must be called with a static object expression.");
    };

    let mut frames = Vec::new();
    for property in &object.props {
        let PropOrSpread::Prop(property) = property else {
            panic!("[nanocss] css.keyframes(...) objects cannot contain spreads.");
        };
        let Prop::KeyValue(property) = &**property else {
            panic!("[nanocss] css.keyframes(...) values must be expressions.");
        };
        let Expr::Object(frame) = &*property.value else {
            panic!("[nanocss] css.keyframes(...) frames must be object expressions.");
        };
        frames.push((
            prop_name_to_string(&property.key).unwrap_or_else(|| {
                panic!("[nanocss] css.keyframes(...) frame keys must be statically known.")
            }),
            parse_keyframe_object(frame),
        ));
    }
    frames
}

fn parse_keyframe_object(object: &ObjectLit) -> Vec<(String, KeyframesValue)> {
    let mut properties = Vec::new();
    for property in &object.props {
        let PropOrSpread::Prop(property) = property else {
            panic!("[nanocss] keyframe style objects cannot contain spreads.");
        };
        let Prop::KeyValue(property) = &**property else {
            panic!("[nanocss] keyframe style values must be expressions.");
        };
        properties.push((
            prop_name_to_string(&property.key).unwrap_or_else(|| {
                panic!("[nanocss] keyframe style property keys must be statically known.")
            }),
            parse_static_value(&property.value),
        ));
    }
    properties
}

fn parse_static_value(expression: &Expr) -> KeyframesValue {
    match expression {
        Expr::Lit(Lit::Str(str_)) => KeyframesValue::String(
            str_.value
                .as_str()
                .map(ToString::to_string)
                .unwrap_or_default(),
        ),
        Expr::Lit(Lit::Num(number)) => KeyframesValue::Number(number.value),
        _ => {
            panic!("[nanocss] css.keyframes(...) values must be static string or number literals.")
        }
    }
}
