use std::collections::HashSet;

use swc_core::ecma::{
    ast::{
        ArrowExpr, BindingIdent, Class, ClassDecl, ClassExpr, Decl, FnDecl, FnExpr, Function,
        ImportSpecifier, Module, ObjectPatProp, Pat, Stmt, VarDecl, VarDeclKind,
    },
    visit::{Visit, VisitWith},
};

pub(crate) fn collect_shadowed_css_names(
    pattern: &Pat,
    css_names: &HashSet<String>,
    shadowed: &mut HashSet<String>,
) {
    match pattern {
        Pat::Ident(binding) => {
            let name = binding.id.sym.to_string();
            if css_names.contains(&name) {
                shadowed.insert(name);
            }
        }
        Pat::Array(array) => {
            for element in array.elems.iter().flatten() {
                collect_shadowed_css_names(element, css_names, shadowed);
            }
        }
        Pat::Rest(rest) => {
            collect_shadowed_css_names(&rest.arg, css_names, shadowed);
        }
        Pat::Object(object) => {
            for property in &object.props {
                match property {
                    ObjectPatProp::KeyValue(property) => {
                        collect_shadowed_css_names(&property.value, css_names, shadowed);
                    }
                    ObjectPatProp::Assign(property) => {
                        let name = property.key.id.sym.to_string();
                        if css_names.contains(&name) {
                            shadowed.insert(name);
                        }
                    }
                    ObjectPatProp::Rest(property) => {
                        collect_shadowed_css_names(&property.arg, css_names, shadowed);
                    }
                    #[cfg(swc_ast_unknown)]
                    _ => panic!("[nanocss] Unknown SWC object pattern property."),
                }
            }
        }
        Pat::Assign(assign) => {
            collect_shadowed_css_names(&assign.left, css_names, shadowed);
        }
        Pat::Invalid(_) | Pat::Expr(_) => {}
        #[cfg(swc_ast_unknown)]
        _ => panic!("[nanocss] Unknown SWC pattern."),
    }
}

pub(crate) fn collect_shadowed_css_names_from_statement(
    statement: &Stmt,
    css_names: &HashSet<String>,
    shadowed: &mut HashSet<String>,
) {
    let Stmt::Decl(declaration) = statement else {
        return;
    };

    match declaration {
        Decl::Var(declaration) => {
            for declarator in &declaration.decls {
                collect_shadowed_css_names(&declarator.name, css_names, shadowed);
            }
        }
        Decl::Fn(declaration) => {
            let name = declaration.ident.sym.to_string();
            if css_names.contains(&name) {
                shadowed.insert(name);
            }
        }
        Decl::Class(declaration) => {
            let name = declaration.ident.sym.to_string();
            if css_names.contains(&name) {
                shadowed.insert(name);
            }
        }
        _ => {}
    }
}

pub(crate) fn collect_shadowed_var_names_from_function_body(
    body: &swc_core::ecma::ast::BlockStmt,
    css_names: &HashSet<String>,
    shadowed: &mut HashSet<String>,
) {
    body.visit_with(&mut VarNameCollector {
        css_names,
        shadowed,
    });
}

pub(crate) fn collect_binding_names_from_module(module: &Module) -> HashSet<String> {
    let mut collector = BindingNameCollector {
        names: HashSet::new(),
    };
    module.visit_with(&mut collector);
    collector.names
}

struct VarNameCollector<'a> {
    css_names: &'a HashSet<String>,
    shadowed: &'a mut HashSet<String>,
}

impl Visit for VarNameCollector<'_> {
    fn visit_var_decl(&mut self, declaration: &VarDecl) {
        if declaration.kind == VarDeclKind::Var {
            for declarator in &declaration.decls {
                collect_shadowed_css_names(&declarator.name, self.css_names, self.shadowed);
            }
        }
    }

    fn visit_function(&mut self, _function: &Function) {}

    fn visit_arrow_expr(&mut self, _arrow: &ArrowExpr) {}

    fn visit_class(&mut self, _class: &Class) {}
}

struct BindingNameCollector {
    names: HashSet<String>,
}

impl Visit for BindingNameCollector {
    fn visit_binding_ident(&mut self, binding: &BindingIdent) {
        self.names.insert(binding.id.sym.to_string());
    }

    fn visit_import_specifier(&mut self, specifier: &ImportSpecifier) {
        self.names.insert(specifier.local().sym.to_string());
    }

    fn visit_fn_decl(&mut self, declaration: &FnDecl) {
        self.names.insert(declaration.ident.sym.to_string());
        declaration.visit_children_with(self);
    }

    fn visit_fn_expr(&mut self, expression: &FnExpr) {
        if let Some(ident) = &expression.ident {
            self.names.insert(ident.sym.to_string());
        }
        expression.visit_children_with(self);
    }

    fn visit_class_decl(&mut self, declaration: &ClassDecl) {
        self.names.insert(declaration.ident.sym.to_string());
        declaration.visit_children_with(self);
    }

    fn visit_class_expr(&mut self, expression: &ClassExpr) {
        if let Some(ident) = &expression.ident {
            self.names.insert(ident.sym.to_string());
        }
        expression.visit_children_with(self);
    }
}
