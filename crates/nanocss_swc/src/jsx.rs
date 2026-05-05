use std::collections::HashSet;

use crate::scope::{
    collect_shadowed_css_names, collect_shadowed_css_names_from_statement,
    collect_shadowed_var_names_from_function_body,
};
use swc_core::ecma::{
    ast::{
        ArrowExpr, BlockStmt, CatchClause, Class, Expr, ForHead, ForInStmt, ForOfStmt, ForStmt,
        Function, JSXAttr, JSXAttrName, JSXAttrOrSpread, JSXAttrValue, JSXExpr, MemberExpr, Pat,
        Prop, VarDecl,
    },
    visit::{Visit, VisitWith},
};

pub(crate) fn collapse_duplicate_style_attributes(attributes: &mut Vec<JSXAttrOrSpread>) {
    let Some(last_style_index) = attributes.iter().rposition(is_jsx_style_attribute) else {
        return;
    };
    let mut style_index = 0;
    attributes.retain(|attribute| {
        let keep = !is_jsx_style_attribute(attribute) || style_index == last_style_index;
        style_index += 1;
        keep
    });
}

pub(crate) fn is_jsx_style_attribute(attribute: &JSXAttrOrSpread) -> bool {
    matches!(
        attribute,
        JSXAttrOrSpread::JSXAttr(JSXAttr {
            name: JSXAttrName::Ident(name),
            ..
        }) if name.sym.as_str() == "style"
    )
}

pub(crate) fn is_jsx_style_attr(attribute: &JSXAttr) -> bool {
    matches!(
        attribute,
        JSXAttr {
            name: JSXAttrName::Ident(name),
            ..
        } if name.sym.as_str() == "style"
    )
}

pub(crate) fn get_jsx_attribute_expression(attribute: &JSXAttr) -> Option<&Expr> {
    let Some(JSXAttrValue::JSXExprContainer(container)) = &attribute.value else {
        return None;
    };
    let JSXExpr::Expr(expression) = &container.expr else {
        return None;
    };
    Some(expression)
}

pub(crate) fn contains_style_group_member_reference(
    style_group_names: &HashSet<String>,
    expression: &Expr,
) -> bool {
    let mut visitor = StyleReferenceVisitor {
        style_group_names,
        shadowed_style_group_names: Vec::new(),
        found: false,
    };
    expression.visit_with(&mut visitor);
    visitor.found
}

struct StyleReferenceVisitor<'a> {
    style_group_names: &'a HashSet<String>,
    shadowed_style_group_names: Vec<String>,
    found: bool,
}

impl StyleReferenceVisitor<'_> {
    fn is_style_group_name(&self, name: &str) -> bool {
        self.style_group_names.contains(name)
            && !self
                .shadowed_style_group_names
                .iter()
                .any(|shadowed| shadowed == name)
    }

    fn collect_shadowed_names_from_patterns(&self, patterns: &[Pat]) -> HashSet<String> {
        let mut shadowed = HashSet::new();
        for pattern in patterns {
            collect_shadowed_css_names(pattern, self.style_group_names, &mut shadowed);
        }
        shadowed
    }

    fn with_shadowed_names(&mut self, shadowed: HashSet<String>, visit: impl FnOnce(&mut Self)) {
        let previous_len = self.shadowed_style_group_names.len();
        self.shadowed_style_group_names.extend(shadowed);
        visit(self);
        self.shadowed_style_group_names.truncate(previous_len);
    }
}

impl Visit for StyleReferenceVisitor<'_> {
    fn visit_expr(&mut self, expression: &Expr) {
        if self.found {
            return;
        }

        expression.visit_children_with(self);
    }

    fn visit_member_expr(&mut self, member: &MemberExpr) {
        if self.found {
            return;
        }
        if let Expr::Ident(object) = &*member.obj
            && self.is_style_group_name(object.sym.as_ref())
        {
            self.found = true;
            return;
        }

        member.visit_children_with(self);
    }

    fn visit_prop(&mut self, property: &Prop) {
        if self.found {
            return;
        }
        if let Prop::Shorthand(ident) = property
            && self.is_style_group_name(ident.sym.as_ref())
        {
            self.found = true;
            return;
        }

        property.visit_children_with(self);
    }

    fn visit_function(&mut self, function: &Function) {
        let mut shadowed = HashSet::new();
        let params = function
            .params
            .iter()
            .map(|param| param.pat.clone())
            .collect::<Vec<_>>();
        shadowed.extend(self.collect_shadowed_names_from_patterns(&params));
        if let Some(body) = &function.body {
            collect_shadowed_var_names_from_function_body(
                body,
                self.style_group_names,
                &mut shadowed,
            );
        }
        self.with_shadowed_names(shadowed, |visitor| {
            function.visit_children_with(visitor);
        });
    }

    fn visit_arrow_expr(&mut self, arrow: &ArrowExpr) {
        let mut shadowed = self.collect_shadowed_names_from_patterns(&arrow.params);
        if let swc_core::ecma::ast::BlockStmtOrExpr::BlockStmt(body) = &*arrow.body {
            collect_shadowed_var_names_from_function_body(
                body,
                self.style_group_names,
                &mut shadowed,
            );
        }
        self.with_shadowed_names(shadowed, |visitor| {
            arrow.visit_children_with(visitor);
        });
    }

    fn visit_block_stmt(&mut self, block: &BlockStmt) {
        let mut shadowed = HashSet::new();
        for statement in &block.stmts {
            collect_shadowed_css_names_from_statement(
                statement,
                self.style_group_names,
                &mut shadowed,
            );
        }
        self.with_shadowed_names(shadowed, |visitor| {
            block.visit_children_with(visitor);
        });
    }

    fn visit_catch_clause(&mut self, catch: &CatchClause) {
        let mut shadowed = HashSet::new();
        if let Some(param) = &catch.param {
            collect_shadowed_css_names(param, self.style_group_names, &mut shadowed);
        }
        self.with_shadowed_names(shadowed, |visitor| {
            catch.visit_children_with(visitor);
        });
    }

    fn visit_for_stmt(&mut self, statement: &ForStmt) {
        let mut shadowed = HashSet::new();
        if let Some(swc_core::ecma::ast::VarDeclOrExpr::VarDecl(declaration)) = &statement.init {
            collect_shadowed_names_from_var_decl(
                declaration,
                self.style_group_names,
                &mut shadowed,
            );
        }
        self.with_shadowed_names(shadowed, |visitor| {
            statement.visit_children_with(visitor);
        });
    }

    fn visit_for_in_stmt(&mut self, statement: &ForInStmt) {
        let mut shadowed = HashSet::new();
        collect_shadowed_names_from_for_head(
            &statement.left,
            self.style_group_names,
            &mut shadowed,
        );
        self.with_shadowed_names(shadowed, |visitor| {
            statement.visit_children_with(visitor);
        });
    }

    fn visit_for_of_stmt(&mut self, statement: &ForOfStmt) {
        let mut shadowed = HashSet::new();
        collect_shadowed_names_from_for_head(
            &statement.left,
            self.style_group_names,
            &mut shadowed,
        );
        self.with_shadowed_names(shadowed, |visitor| {
            statement.visit_children_with(visitor);
        });
    }

    fn visit_class(&mut self, _class: &Class) {}
}

fn collect_shadowed_names_from_var_decl(
    declaration: &VarDecl,
    style_group_names: &HashSet<String>,
    shadowed: &mut HashSet<String>,
) {
    for declarator in &declaration.decls {
        collect_shadowed_css_names(&declarator.name, style_group_names, shadowed);
    }
}

fn collect_shadowed_names_from_for_head(
    head: &ForHead,
    style_group_names: &HashSet<String>,
    shadowed: &mut HashSet<String>,
) {
    match head {
        ForHead::VarDecl(declaration) => {
            collect_shadowed_names_from_var_decl(declaration, style_group_names, shadowed);
        }
        ForHead::Pat(pattern) => {
            collect_shadowed_css_names(pattern, style_group_names, shadowed);
        }
        ForHead::UsingDecl(declaration) => {
            for declarator in &declaration.decls {
                collect_shadowed_css_names(&declarator.name, style_group_names, shadowed);
            }
        }
        #[cfg(swc_ast_unknown)]
        _ => panic!("[nanocss] Unknown SWC for head."),
    }
}
