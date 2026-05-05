use swc_core::{
    common::DUMMY_SP,
    ecma::ast::{
        BinaryOp, Expr, ExprOrSpread, Lit, ObjectLit, PropOrSpread, SpreadElement, UnaryOp,
    },
};

pub(crate) fn create_style_object_from_props_args_with_resolver(
    args: &[ExprOrSpread],
    resolve_style: &mut impl FnMut(&Expr) -> Option<Expr>,
) -> Expr {
    if let [arg] = args
        && arg.spread.is_none()
        && let Some(style) = single_direct_style_expression(&arg.expr)
    {
        return resolve_style(style).unwrap_or_else(|| style.clone());
    }

    let mut style_properties = Vec::new();
    for arg in args {
        if arg.spread.is_some() {
            panic!("[nanocss] css.props(...) does not accept spread arguments.");
        }
        append_style_spreads_from_expr_with_resolver(
            &mut style_properties,
            &arg.expr,
            resolve_style,
        );
    }

    Expr::Object(ObjectLit {
        span: DUMMY_SP,
        props: style_properties,
    })
}

pub(crate) fn append_style_spreads_from_expr_with_resolver(
    properties: &mut Vec<PropOrSpread>,
    expression: &Expr,
    resolve_style: &mut impl FnMut(&Expr) -> Option<Expr>,
) {
    if let Expr::Array(array) = expression {
        for element in &array.elems {
            let Some(element) = element else {
                continue;
            };
            if element.spread.is_some() {
                panic!("[nanocss] css.props(...) style arrays cannot contain spreads.");
            }
            append_style_spreads_from_expr_with_resolver(properties, &element.expr, resolve_style);
        }
        return;
    }

    if is_falsy_style_expression(expression) {
        return;
    }

    properties.push(PropOrSpread::Spread(SpreadElement {
        dot3_token: DUMMY_SP,
        expr: Box::new(resolve_style(expression).unwrap_or_else(|| expression.clone())),
    }));
}

fn can_use_style_expression_directly(expression: &Expr) -> bool {
    match expression {
        Expr::Ident(_) | Expr::Member(_) | Expr::Call(_) => true,
        Expr::Cond(_) => true,
        Expr::Bin(binary)
            if matches!(binary.op, BinaryOp::LogicalOr | BinaryOp::NullishCoalescing) =>
        {
            true
        }
        Expr::Paren(expression) => can_use_style_expression_directly(&expression.expr),
        _ => false,
    }
}

fn single_direct_style_expression(expression: &Expr) -> Option<&Expr> {
    let mut expressions = Vec::new();
    collect_non_falsy_style_expressions(expression, &mut expressions);
    let [expression] = expressions.as_slice() else {
        return None;
    };
    can_use_style_expression_directly(expression).then_some(*expression)
}

fn collect_non_falsy_style_expressions<'a>(expression: &'a Expr, expressions: &mut Vec<&'a Expr>) {
    if let Expr::Array(array) = expression {
        for element in &array.elems {
            let Some(element) = element else {
                continue;
            };
            if element.spread.is_some() {
                panic!("[nanocss] css.props(...) style arrays cannot contain spreads.");
            }
            collect_non_falsy_style_expressions(&element.expr, expressions);
        }
        return;
    }

    if !is_falsy_style_expression(expression) {
        expressions.push(expression);
    }
}

fn is_falsy_style_expression(expression: &Expr) -> bool {
    match expression {
        Expr::Lit(Lit::Null(_)) => true,
        Expr::Lit(Lit::Bool(value)) => !value.value,
        Expr::Ident(identifier) => identifier.sym.as_ref() == "undefined",
        Expr::Unary(unary) if unary.op == UnaryOp::Void => true,
        Expr::Paren(expression) => is_falsy_style_expression(&expression.expr),
        _ => false,
    }
}
