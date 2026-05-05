use std::collections::{BTreeSet, HashMap, HashSet};

use swc_core::{
    common::{DUMMY_SP, SourceMapper},
    ecma::{
        ast::{
            ArrowExpr, BinExpr, BinaryOp, BindingIdent, BlockStmt, BlockStmtOrExpr, CallExpr,
            Callee, CatchClause, Class, CondExpr, Decl, Expr, ExprOrSpread, ForHead, ForInStmt,
            ForOfStmt, ForStmt, Function, Ident, JSXAttr, JSXAttrName, JSXAttrOrSpread,
            JSXAttrValue, JSXClosingElement, JSXElementChild, JSXExpr, JSXExprContainer,
            JSXOpeningElement, KeyValueProp, Lit, MemberExpr, MemberProp, Module, ModuleDecl,
            ModuleItem, ObjectLit, ParenExpr, Pat, Prop, PropName, PropOrSpread, ReturnStmt, Stmt,
            Str, UnaryOp, VarDecl, VarDeclKind, VarDeclOrExpr, VarDeclarator,
        },
        visit::{Visit, VisitMut, VisitMutWith, VisitWith},
    },
};
use swc_sourcemap::DecodedMap;

use crate::ast::{is_css_member_call, prop_name_to_string};
use crate::declarations::{compile_create_theme_call, compile_define_vars_declaration};
use crate::define_consts::{ConstGroups, parse_define_consts_arg};
use crate::env::replace_env_references;
use crate::generated_strings::{GeneratedString, GeneratedStringKind};
use crate::hooks::HookCompiler;
use crate::html::{
    apply_html_default_style, create_jsx_element_name, get_html_tag_name, html_default_style,
    html_default_style_id, html_spread_temp_count,
};
use crate::imports::{get_named_import, remove_unused_nanocss_imports};
use crate::jsx::{
    collapse_duplicate_style_attributes, contains_style_group_member_reference,
    get_jsx_attribute_expression, is_jsx_style_attr,
};
use crate::keyframes::{CompiledKeyframes, compile_keyframes};
use crate::keyframes_ast::parse_keyframes_arg;
use crate::metadata::create_style_sheet;
use crate::options::TransformOptions;
use crate::position_try::{CompiledPositionTry, compile_position_try};
use crate::props::create_style_object_from_props_args_with_resolver;
use crate::scope::{
    collect_binding_names_from_module, collect_shadowed_css_names,
    collect_shadowed_css_names_from_statement, collect_shadowed_var_names_from_function_body,
};
use crate::styles::parse_create_arg;
use crate::variables::{CompiledVariableDefault, CompiledVariableProperty};
use crate::view_transition::{CompiledViewTransitionClass, compile_view_transition_class};

#[derive(Clone)]
struct StaticStyleRef {
    group_name: String,
    style_name: String,
}

struct PendingStyleDeclaration {
    id: String,
    init: Expr,
}

#[derive(Clone)]
struct DynamicStyleMember {
    helper_id: Option<String>,
    init: Expr,
}

pub(crate) struct NanoCssTransform<'a> {
    options: TransformOptions,
    file_identity: String,
    source_map: Option<&'a dyn SourceMapper>,
    input_source_map: Option<DecodedMap>,
    css_names: HashSet<String>,
    html_names: HashSet<String>,
    style_group_names: HashSet<String>,
    static_style_groups: HashMap<String, HashMap<String, Expr>>,
    dynamic_style_groups: HashMap<String, HashMap<String, DynamicStyleMember>>,
    merged_style_ids: HashMap<String, String>,
    pending_style_declarations: HashMap<String, Vec<PendingStyleDeclaration>>,
    reserved_helper_names: HashSet<String>,
    simple_constants: HashMap<String, Expr>,
    html_default_style_ids: HashMap<String, String>,
    html_spread_temp_stack: Vec<Vec<String>>,
    imported_variable_group_names: HashSet<String>,
    exported_const_group_names: HashSet<String>,
    pending_html_default_styles: BTreeSet<String>,
    non_top_level_depth: usize,
    hook_compiler: HookCompiler,
    dynamic_hook_id: usize,
    variable_groups: HashMap<String, HashMap<String, String>>,
    const_groups: ConstGroups,
    generated_string_names: HashMap<String, GeneratedString>,
    pub keyframes: Vec<CompiledKeyframes>,
    pub position_tries: Vec<CompiledPositionTry>,
    pub view_transition_classes: Vec<CompiledViewTransitionClass>,
    pub variable_defaults: Vec<CompiledVariableDefault>,
    pub variable_properties: Vec<CompiledVariableProperty>,
}

impl<'a> NanoCssTransform<'a> {
    pub fn new(options: TransformOptions, file_identity: String) -> Self {
        Self::new_with_source_map(options, file_identity, None)
    }

    pub fn new_with_source_map(
        options: TransformOptions,
        file_identity: String,
        source_map: Option<&'a dyn SourceMapper>,
    ) -> Self {
        let debug = options.debug;
        let input_source_map = parse_input_source_map(options.input_source_map.as_ref());

        Self {
            options,
            file_identity,
            source_map,
            input_source_map,
            css_names: HashSet::new(),
            html_names: HashSet::new(),
            style_group_names: HashSet::new(),
            static_style_groups: HashMap::new(),
            dynamic_style_groups: HashMap::new(),
            merged_style_ids: HashMap::new(),
            pending_style_declarations: HashMap::new(),
            reserved_helper_names: HashSet::new(),
            simple_constants: HashMap::new(),
            html_default_style_ids: HashMap::new(),
            html_spread_temp_stack: Vec::new(),
            imported_variable_group_names: HashSet::new(),
            exported_const_group_names: HashSet::new(),
            pending_html_default_styles: BTreeSet::new(),
            non_top_level_depth: 0,
            hook_compiler: HookCompiler::new(debug),
            dynamic_hook_id: 0,
            variable_groups: HashMap::new(),
            const_groups: ConstGroups::new(),
            generated_string_names: HashMap::new(),
            keyframes: Vec::new(),
            position_tries: Vec::new(),
            view_transition_classes: Vec::new(),
            variable_defaults: Vec::new(),
            variable_properties: Vec::new(),
        }
    }

    pub fn style_sheet(&self) -> String {
        create_style_sheet(
            &self.hook_compiler.style_sheet(),
            &self.keyframes,
            &self.position_tries,
            &self.view_transition_classes,
            &self.variable_properties,
            &self.variable_defaults,
            self.options.debug,
        )
    }

    fn collect_imports(&mut self, module: &Module) {
        for item in &module.body {
            let ModuleItem::ModuleDecl(ModuleDecl::Import(import_decl)) = item else {
                continue;
            };
            let Some(source) = import_decl.src.value.as_str() else {
                continue;
            };
            if !self
                .options
                .import_sources
                .iter()
                .any(|import_source| import_source == source)
            {
                if is_compiled_css_module_source(source) {
                    for specifier in &import_decl.specifiers {
                        let Some((_, local)) = get_named_import(specifier) else {
                            continue;
                        };
                        self.imported_variable_group_names.insert(local);
                    }
                }
                continue;
            }

            for specifier in &import_decl.specifiers {
                let Some((imported, local)) = get_named_import(specifier) else {
                    continue;
                };
                if imported == "css" {
                    self.css_names.insert(local);
                } else if imported == "html" {
                    self.html_names.insert(local);
                }
            }
        }
    }

    fn validate_exported_css_module_declarations(&mut self, module: &Module) {
        for item in &module.body {
            match item {
                ModuleItem::ModuleDecl(ModuleDecl::ExportDecl(export_decl)) => {
                    let Decl::Var(var_decl) = &export_decl.decl else {
                        continue;
                    };
                    for declarator in &var_decl.decls {
                        let Some(init) = &declarator.init else {
                            continue;
                        };
                        let Expr::Call(call) = &**init else {
                            continue;
                        };
                        if is_css_member_call(&self.css_names, &call.callee, "defineVars")
                            && !is_css_source_file(&self.file_identity)
                        {
                            panic!(
                                "[nanocss] Exported css.defineVars(...) declarations must be in *.css.ts files."
                            );
                        }
                        if is_css_member_call(&self.css_names, &call.callee, "createTheme")
                            && !is_css_source_file(&self.file_identity)
                        {
                            panic!(
                                "[nanocss] Exported css.createTheme(...) declarations must be in *.css.ts files."
                            );
                        }
                        if is_css_member_call(&self.css_names, &call.callee, "defineConsts") {
                            if !is_css_source_file(&self.file_identity) {
                                panic!(
                                    "[nanocss] Exported css.defineConsts(...) declarations must be in *.css.ts files."
                                );
                            }
                            if let Some(name) = declarator
                                .name
                                .as_ident()
                                .map(|binding| binding.id.sym.to_string())
                            {
                                self.exported_const_group_names.insert(name);
                            }
                        }
                        if is_css_member_call(&self.css_names, &call.callee, "keyframes") {
                            panic!(
                                "[nanocss] Exported css.keyframes(...) declarations must be wrapped in css.defineVars(...) for cross-file style use."
                            );
                        }
                        if is_css_member_call(&self.css_names, &call.callee, "positionTry") {
                            panic!(
                                "[nanocss] Exported css.positionTry(...) declarations must be wrapped in css.defineVars(...) for cross-file style use."
                            );
                        }
                    }
                }
                ModuleItem::Stmt(Stmt::Decl(Decl::Var(var_decl))) => {
                    for declarator in &var_decl.decls {
                        let Some(init) = &declarator.init else {
                            continue;
                        };
                        let Expr::Call(call) = &**init else {
                            continue;
                        };
                        if is_css_member_call(&self.css_names, &call.callee, "defineConsts") {
                            panic!(
                                "[nanocss] css.defineConsts(...) declarations must be exported from *.css.ts files."
                            );
                        }
                    }
                }
                _ => {}
            }
        }
    }

    fn insert_pending_html_default_styles(&mut self, module: &mut Module) {
        if self.pending_html_default_styles.is_empty() {
            return;
        }

        let declarations = self
            .pending_html_default_styles
            .iter()
            .filter_map(|tag_name| {
                let id = self.html_default_style_ids.get(tag_name)?.clone();
                let init = html_default_style(tag_name, &self.options.html_defaults)?;
                Some(ModuleItem::Stmt(Stmt::Decl(Decl::Var(Box::new(VarDecl {
                    span: DUMMY_SP,
                    ctxt: Default::default(),
                    kind: VarDeclKind::Const,
                    declare: false,
                    decls: vec![VarDeclarator {
                        span: DUMMY_SP,
                        name: Pat::Ident(BindingIdent::from(Ident::new(
                            id.into(),
                            DUMMY_SP,
                            Default::default(),
                        ))),
                        init: Some(Box::new(Expr::Object(init))),
                        definite: false,
                    }],
                })))))
            })
            .collect::<Vec<_>>();

        let insert_at = module
            .body
            .iter()
            .rposition(|item| matches!(item, ModuleItem::ModuleDecl(ModuleDecl::Import(_))))
            .map_or(0, |index| index + 1);
        module.body.splice(insert_at..insert_at, declarations);
    }

    fn insert_pending_style_declarations(&mut self, module: &mut Module) {
        if self.pending_style_declarations.is_empty() {
            return;
        }

        let mut inserts = Vec::new();
        for (index, item) in module.body.iter().enumerate() {
            let Some(group_name) = style_group_name_from_module_item(item) else {
                continue;
            };
            let Some(declarations) = self.pending_style_declarations.remove(&group_name) else {
                continue;
            };
            inserts.push((index + 1, declarations));
        }

        for (index, declarations) in inserts.into_iter().rev() {
            module.body.splice(
                index..index,
                declarations
                    .into_iter()
                    .map(style_declaration_to_module_item),
            );
        }
    }

    fn insert_pending_style_declarations_in_statements(&mut self, statements: &mut Vec<Stmt>) {
        if self.pending_style_declarations.is_empty() {
            return;
        }

        let mut inserts = Vec::new();
        for (index, statement) in statements.iter().enumerate() {
            let Some(group_name) = style_group_name_from_statement(statement) else {
                continue;
            };
            let Some(declarations) = self.pending_style_declarations.remove(&group_name) else {
                continue;
            };
            inserts.push((index + 1, declarations));
        }

        for (index, declarations) in inserts.into_iter().rev() {
            statements.splice(
                index..index,
                declarations.into_iter().map(style_declaration_to_statement),
            );
        }
    }

    fn remove_unused_style_declarations(&self, module: &mut Module) {
        if self.style_group_names.is_empty() {
            return;
        }

        let used = collect_used_style_identifiers(module, &self.style_group_names);
        let mut remove_indices = Vec::new();
        for (index, item) in module.body.iter().enumerate() {
            let ModuleItem::Stmt(Stmt::Decl(Decl::Var(var_decl))) = item else {
                continue;
            };
            if var_decl.decls.len() != 1 {
                continue;
            }
            let Some(name) = var_decl.decls[0]
                .name
                .as_ident()
                .map(|binding| binding.id.sym.to_string())
            else {
                continue;
            };
            if !self.style_group_names.contains(&name) {
                continue;
            }

            if !used.contains(&name) {
                remove_indices.push(index);
            }
        }

        for index in remove_indices.into_iter().rev() {
            module.body.remove(index);
        }
    }

    fn remove_unused_style_declarations_from_statements(&self, statements: &mut Vec<Stmt>) {
        if self.style_group_names.is_empty() {
            return;
        }

        let used =
            collect_used_style_identifiers_in_statements(statements, &self.style_group_names);
        let mut remove_indices = Vec::new();
        for (index, statement) in statements.iter().enumerate() {
            let Stmt::Decl(Decl::Var(var_decl)) = statement else {
                continue;
            };
            if var_decl.decls.len() != 1 {
                continue;
            }
            let Some(name) = var_decl.decls[0]
                .name
                .as_ident()
                .map(|binding| binding.id.sym.to_string())
            else {
                continue;
            };
            if !self.style_group_names.contains(&name) {
                continue;
            }

            if !used.contains(&name) {
                remove_indices.push(index);
            }
        }

        for index in remove_indices.into_iter().rev() {
            statements.remove(index);
        }
    }

    fn remove_unused_simple_constant_declarations(&self, module: &mut Module) {
        if self.simple_constants.is_empty() {
            return;
        }

        let simple_constant_names = self
            .simple_constants
            .keys()
            .cloned()
            .collect::<HashSet<_>>();
        let used = collect_used_simple_constant_identifiers(module, &simple_constant_names);
        let mut remove_indices = Vec::new();
        for (index, item) in module.body.iter().enumerate() {
            let ModuleItem::Stmt(Stmt::Decl(Decl::Var(var_decl))) = item else {
                continue;
            };
            if var_decl.kind != VarDeclKind::Const || var_decl.decls.len() != 1 {
                continue;
            }
            let Some(name) = var_decl.decls[0]
                .name
                .as_ident()
                .map(|binding| binding.id.sym.to_string())
            else {
                continue;
            };
            if simple_constant_names.contains(&name) && !used.contains(&name) {
                remove_indices.push(index);
            }
        }

        for index in remove_indices.into_iter().rev() {
            module.body.remove(index);
        }
    }

    fn resolve_static_style_ref(&self, expression: &Expr) -> Option<StaticStyleRef> {
        let Expr::Member(member) = expression else {
            return None;
        };
        let Expr::Ident(object) = &*member.obj else {
            return None;
        };
        let group_name = object.sym.to_string();
        if !self.style_group_names.contains(&group_name) {
            return None;
        }
        let style_name = member_prop_to_string(&member.prop)?;
        self.static_style_groups
            .get(&group_name)?
            .get(&style_name)?;
        Some(StaticStyleRef {
            group_name,
            style_name,
        })
    }

    fn resolve_static_style_member(&mut self, expression: &Expr) -> Option<Expr> {
        let style_ref = self.resolve_static_style_ref(expression)?;
        Some(self.register_static_style_segment(&[style_ref]))
    }

    fn static_style_cache_key(&self, styles: &[StaticStyleRef]) -> String {
        styles
            .iter()
            .map(|style| format!("{}.{}", style.group_name, style.style_name))
            .collect::<Vec<_>>()
            .join("\0")
    }

    fn next_style_id(&mut self, styles: &[StaticStyleRef]) -> String {
        if let Some(base) = self.debug_static_style_id_base(styles) {
            return self.allocate_numbered_helper_name(&base);
        }

        self.allocate_numbered_helper_name("_styles")
    }

    fn debug_static_style_id_base(&self, styles: &[StaticStyleRef]) -> Option<String> {
        if !self.options.debug {
            return None;
        }

        let first = styles.first()?;
        let base = if styles
            .iter()
            .all(|style| style.group_name == first.group_name)
        {
            let suffix = styles
                .iter()
                .map(|style| helper_name_fragment(&style.style_name))
                .collect::<String>();
            if suffix.is_empty() {
                return None;
            }
            format!("_{}{suffix}", first.group_name)
        } else {
            let suffix = styles
                .iter()
                .map(|style| {
                    format!(
                        "{}{}",
                        style.group_name,
                        helper_name_fragment(&style.style_name)
                    )
                })
                .collect::<Vec<_>>()
                .join("_");
            if suffix.is_empty() {
                return None;
            }
            format!("_{suffix}")
        };

        if base.len() > 80 {
            return None;
        }

        Some(base)
    }

    fn html_default_style_id_for(&mut self, tag_name: &str) -> Option<String> {
        if let Some(id) = self.html_default_style_ids.get(tag_name) {
            return Some(id.clone());
        }

        let base = html_default_style_id(tag_name, &self.options.html_defaults)?;
        let id = self.allocate_numbered_helper_name(&base);
        self.html_default_style_ids
            .insert(tag_name.to_string(), id.clone());
        Some(id)
    }

    fn next_html_spread_temp_id(&mut self) -> String {
        self.allocate_numbered_helper_name("_htmlProps")
    }

    fn next_dynamic_style_id(&mut self, group_name: &str, style_name: &str) -> String {
        self.allocate_numbered_helper_name(&format!(
            "_{}{}",
            group_name,
            helper_name_fragment(style_name)
        ))
    }

    fn allocate_numbered_helper_name(&mut self, base: &str) -> String {
        for index in 1.. {
            let candidate = if index == 1 {
                base.to_string()
            } else {
                format!("{base}{index}")
            };
            if self.reserved_helper_names.insert(candidate.clone()) {
                return candidate;
            }
        }
        unreachable!()
    }

    fn register_static_style_segment(&mut self, styles: &[StaticStyleRef]) -> Expr {
        let cache_key = self.static_style_cache_key(styles);
        if let Some(id) = self.merged_style_ids.get(&cache_key) {
            return Expr::Ident(Ident::new(id.clone().into(), DUMMY_SP, Default::default()));
        }

        let id = self.next_style_id(styles);
        self.merged_style_ids.insert(cache_key, id.clone());
        let init = self.merge_static_styles(styles);
        if let Some(first_style) = styles.first() {
            self.pending_style_declarations
                .entry(first_style.group_name.clone())
                .or_default()
                .push(PendingStyleDeclaration {
                    id: id.clone(),
                    init,
                });
        }

        Expr::Ident(Ident::new(id.into(), DUMMY_SP, Default::default()))
    }

    fn merge_static_styles(&self, styles: &[StaticStyleRef]) -> Expr {
        struct MergedProperty {
            keep: bool,
            key: Option<String>,
            prop: PropOrSpread,
        }

        let mut props = Vec::<MergedProperty>::new();
        let mut prop_by_key = HashMap::<String, usize>::new();
        let mut computed_index = 0;

        for style_ref in styles {
            let Some(Expr::Object(style)) = self
                .static_style_groups
                .get(&style_ref.group_name)
                .and_then(|group| group.get(&style_ref.style_name))
            else {
                continue;
            };

            for prop in &style.props {
                let PropOrSpread::Prop(property) = prop else {
                    continue;
                };
                let key = match &**property {
                    Prop::KeyValue(property) => {
                        merge_property_key(&property.key, &mut computed_index)
                    }
                    Prop::Shorthand(identifier) => Some(identifier.sym.to_string()),
                    _ => None,
                };
                if let Some(key) = key.as_ref()
                    && let Some(previous_index) = prop_by_key.get(key).copied()
                    && !prop_or_spread_may_have_side_effects(&props[previous_index].prop)
                {
                    props[previous_index].keep = false;
                }
                let index = props.len();
                if let Some(key) = key.as_ref() {
                    prop_by_key.insert(key.clone(), index);
                }
                props.push(MergedProperty {
                    keep: true,
                    key,
                    prop: prop.clone(),
                });
            }
        }

        Expr::Object(ObjectLit {
            span: DUMMY_SP,
            props: props
                .into_iter()
                .filter(|property| property.keep || property.key.is_none())
                .map(|property| property.prop)
                .collect(),
        })
    }

    fn resolve_static_props_args(
        &mut self,
        args: &[swc_core::ecma::ast::ExprOrSpread],
    ) -> Option<Expr> {
        let mut styles = Vec::new();
        for arg in args {
            if arg.spread.is_some() {
                return None;
            }
            collect_static_style_refs(&arg.expr, self, &mut styles)?;
        }
        if styles.is_empty() {
            return None;
        }
        let first_group = &styles[0].group_name;
        if styles.iter().any(|style| style.group_name != *first_group) {
            return None;
        }
        Some(self.register_static_style_segment(&styles))
    }

    fn create_props_style_expression(
        &mut self,
        args: &[swc_core::ecma::ast::ExprOrSpread],
    ) -> Expr {
        if let Some(style) = self.resolve_static_props_args(args) {
            return style;
        }

        let mut resolve = |expression: &Expr| self.resolve_style_expression(expression);
        create_style_object_from_props_args_with_resolver(args, &mut resolve)
    }

    fn create_props_object_expression(
        &mut self,
        args: &[swc_core::ecma::ast::ExprOrSpread],
    ) -> Expr {
        Expr::Object(ObjectLit {
            span: DUMMY_SP,
            props: vec![PropOrSpread::Prop(Box::new(Prop::KeyValue(
                swc_core::ecma::ast::KeyValueProp {
                    key: PropName::Ident("style".into()),
                    value: Box::new(self.create_props_style_expression(args)),
                },
            )))],
        })
    }

    fn wrap_html_spread_temp_expression(&self, expression: Expr, temp_names: Vec<String>) -> Expr {
        if temp_names.is_empty() {
            return expression;
        }

        Expr::Call(CallExpr {
            span: DUMMY_SP,
            ctxt: Default::default(),
            callee: Callee::Expr(Box::new(Expr::Paren(ParenExpr {
                span: DUMMY_SP,
                expr: Box::new(Expr::Arrow(ArrowExpr {
                    span: DUMMY_SP,
                    ctxt: Default::default(),
                    params: Vec::new(),
                    body: Box::new(BlockStmtOrExpr::BlockStmt(BlockStmt {
                        span: DUMMY_SP,
                        ctxt: Default::default(),
                        stmts: vec![
                            Stmt::Decl(Decl::Var(Box::new(VarDecl {
                                span: DUMMY_SP,
                                ctxt: Default::default(),
                                kind: VarDeclKind::Let,
                                declare: false,
                                decls: temp_names
                                    .into_iter()
                                    .map(|name| VarDeclarator {
                                        span: DUMMY_SP,
                                        name: Pat::Ident(BindingIdent::from(Ident::new(
                                            name.into(),
                                            DUMMY_SP,
                                            Default::default(),
                                        ))),
                                        init: None,
                                        definite: false,
                                    })
                                    .collect(),
                            }))),
                            Stmt::Return(ReturnStmt {
                                span: DUMMY_SP,
                                arg: Some(Box::new(expression)),
                            }),
                        ],
                    })),
                    is_async: false,
                    is_generator: false,
                    type_params: None,
                    return_type: None,
                })),
            }))),
            args: Vec::new(),
            type_args: None,
        })
    }

    fn resolve_style_expression(&mut self, expression: &Expr) -> Option<Expr> {
        if let Some(style) = self.resolve_static_style_member(expression) {
            return Some(style);
        }
        if let Some(style) = self.resolve_dynamic_style_call(expression) {
            return Some(style);
        }

        match expression {
            Expr::Paren(paren) => self.resolve_style_expression(&paren.expr),
            Expr::Cond(conditional) => {
                let consequent = self.resolve_style_expression(&conditional.cons);
                let alternate = self.resolve_style_expression(&conditional.alt);
                if consequent.is_none() && alternate.is_none() {
                    return None;
                }
                Some(Expr::Cond(CondExpr {
                    span: conditional.span,
                    test: conditional.test.clone(),
                    cons: Box::new(consequent.unwrap_or_else(|| (*conditional.cons).clone())),
                    alt: Box::new(alternate.unwrap_or_else(|| (*conditional.alt).clone())),
                }))
            }
            Expr::Bin(binary) if is_style_composition_operator(binary.op) => {
                let left = self.resolve_style_expression(&binary.left);
                let right = self.resolve_style_expression(&binary.right);
                if left.is_none() && right.is_none() {
                    return None;
                }
                Some(Expr::Bin(BinExpr {
                    span: binary.span,
                    op: binary.op,
                    left: Box::new(left.unwrap_or_else(|| (*binary.left).clone())),
                    right: Box::new(right.unwrap_or_else(|| (*binary.right).clone())),
                }))
            }
            _ => None,
        }
    }

    fn resolve_dynamic_style_call(&mut self, expression: &Expr) -> Option<Expr> {
        let Expr::Call(call) = expression else {
            return None;
        };
        let Callee::Expr(callee) = &call.callee else {
            return None;
        };
        let Expr::Member(member) = &**callee else {
            return None;
        };
        let Expr::Ident(object) = &*member.obj else {
            return None;
        };
        let group_name = object.sym.to_string();
        if !self.style_group_names.contains(&group_name) {
            return None;
        }
        let style_name = member_prop_to_string(&member.prop)?;
        let (dynamic_style_id, dynamic_style_init) = {
            let dynamic_style = self
                .dynamic_style_groups
                .get(&group_name)?
                .get(&style_name)?;
            (dynamic_style.helper_id.clone(), dynamic_style.init.clone())
        };
        let dynamic_style_id = if let Some(id) = dynamic_style_id {
            id
        } else {
            let id = self.next_dynamic_style_id(&group_name, &style_name);
            self.pending_style_declarations
                .entry(group_name.clone())
                .or_default()
                .push(PendingStyleDeclaration {
                    id: id.clone(),
                    init: dynamic_style_init,
                });
            if let Some(dynamic_style) = self
                .dynamic_style_groups
                .get_mut(&group_name)
                .and_then(|group| group.get_mut(&style_name))
            {
                dynamic_style.helper_id = Some(id.clone());
            }
            id
        };

        Some(Expr::Call(CallExpr {
            span: call.span,
            ctxt: call.ctxt,
            callee: Callee::Expr(Box::new(Expr::Ident(Ident::new(
                dynamic_style_id.clone().into(),
                DUMMY_SP,
                Default::default(),
            )))),
            args: call.args.clone(),
            type_args: call.type_args.clone(),
        }))
    }

    fn create_html_source_attribute(
        &self,
        span: swc_core::common::Span,
    ) -> Option<JSXAttrOrSpread> {
        if !self.options.debug {
            return None;
        }
        let location = self
            .source_map
            .map(|source_map| source_map.lookup_char_pos(span.lo))?;
        let line = self
            .input_source_map
            .as_ref()
            .and_then(|source_map| {
                source_map.lookup_token(
                    location.line.saturating_sub(1) as u32,
                    location.col.0 as u32,
                )
            })
            .map(|token| token.get_src_line())
            .filter(|line| *line != u32::MAX)
            .map(|line| line as usize + 1)
            .unwrap_or(location.line);
        Some(JSXAttrOrSpread::JSXAttr(JSXAttr {
            span: DUMMY_SP,
            name: JSXAttrName::Ident("data-element-src".into()),
            value: Some(JSXAttrValue::Str(Str {
                span: DUMMY_SP,
                value: format!("{}:{line}", self.file_identity).into(),
                raw: None,
            })),
        }))
    }

    fn variable_group_names(&self) -> HashSet<String> {
        self.variable_groups
            .keys()
            .chain(self.imported_variable_group_names.iter())
            .cloned()
            .collect()
    }

    fn const_group_names(&self) -> HashSet<String> {
        self.const_groups.keys().cloned().collect()
    }

    fn with_shadowed_names(
        &mut self,
        shadowed_css: &HashSet<String>,
        shadowed_html: &HashSet<String>,
        shadowed_styles: &HashSet<String>,
        shadowed_variables: &HashSet<String>,
        shadowed_consts: &HashSet<String>,
        visit: impl FnOnce(&mut Self),
    ) {
        let css_names = self.css_names.clone();
        let html_names = self.html_names.clone();
        let style_group_names = self.style_group_names.clone();
        let static_style_groups = self.static_style_groups.clone();
        let dynamic_style_groups = self.dynamic_style_groups.clone();
        let variable_group_names = self.variable_groups.keys().cloned().collect::<HashSet<_>>();
        let imported_variable_group_names = self.imported_variable_group_names.clone();
        let const_group_names = self.const_groups.keys().cloned().collect::<HashSet<_>>();

        for name in shadowed_css {
            self.css_names.remove(name);
        }
        for name in shadowed_html {
            self.html_names.remove(name);
        }
        for name in shadowed_styles {
            self.style_group_names.remove(name);
            self.static_style_groups.remove(name);
            self.dynamic_style_groups.remove(name);
        }
        let mut shadowed_variable_groups = Vec::new();
        for name in shadowed_variables {
            if let Some(group) = self.variable_groups.remove(name) {
                shadowed_variable_groups.push((name.clone(), group));
            }
            self.imported_variable_group_names.remove(name);
        }
        let mut shadowed_const_groups = Vec::new();
        for name in shadowed_consts {
            if let Some(group) = self.const_groups.remove(name) {
                shadowed_const_groups.push((name.clone(), group));
            }
        }

        visit(self);

        self.css_names = css_names;
        self.html_names = html_names;
        self.style_group_names = style_group_names;
        self.static_style_groups = static_style_groups;
        self.dynamic_style_groups = dynamic_style_groups;
        self.variable_groups
            .retain(|name, _| variable_group_names.contains(name));
        for (name, group) in shadowed_variable_groups {
            self.variable_groups.insert(name, group);
        }
        self.imported_variable_group_names = imported_variable_group_names;
        self.const_groups
            .retain(|name, _| const_group_names.contains(name));
        for (name, group) in shadowed_const_groups {
            self.const_groups.insert(name, group);
        }
    }

    fn with_non_top_level(&mut self, visit: impl FnOnce(&mut Self)) {
        self.non_top_level_depth += 1;
        visit(self);
        self.non_top_level_depth -= 1;
    }

    fn record_simple_constant_declarator(&mut self, declarator: &VarDeclarator) {
        let Some(name) = declarator
            .name
            .as_ident()
            .map(|binding| binding.id.sym.to_string())
        else {
            return;
        };
        if self.css_names.contains(&name)
            || self.html_names.contains(&name)
            || self.style_group_names.contains(&name)
            || self.variable_groups.contains_key(&name)
            || self.const_groups.contains_key(&name)
            || self.generated_string_names.contains_key(&name)
        {
            return;
        }
        let Some(init) = declarator.init.as_deref() else {
            return;
        };
        let Some(value) = simple_constant_expression(init) else {
            self.simple_constants.remove(&name);
            return;
        };
        self.simple_constants.insert(name, value);
    }
}

impl VisitMut for NanoCssTransform<'_> {
    fn visit_mut_module(&mut self, module: &mut Module) {
        self.reserved_helper_names
            .extend(collect_binding_names_from_module(module));
        self.collect_imports(module);
        self.validate_exported_css_module_declarations(module);
        module.visit_mut_children_with(self);
        self.insert_pending_html_default_styles(module);
        self.insert_pending_style_declarations(module);
        self.remove_unused_style_declarations(module);
        self.remove_unused_simple_constant_declarations(module);
        remove_unused_nanocss_imports(module, &self.options.import_sources);
    }

    fn visit_mut_function(&mut self, function: &mut Function) {
        let mut shadowed_css = HashSet::new();
        let mut shadowed_html = HashSet::new();
        let mut shadowed_styles = HashSet::new();
        let mut shadowed_variables = HashSet::new();
        let mut shadowed_consts = HashSet::new();
        let variable_group_names = self.variable_group_names();
        let const_group_names = self.const_group_names();
        for param in &function.params {
            collect_shadowed_css_names(&param.pat, &self.css_names, &mut shadowed_css);
            collect_shadowed_css_names(&param.pat, &self.html_names, &mut shadowed_html);
            collect_shadowed_css_names(&param.pat, &self.style_group_names, &mut shadowed_styles);
            collect_shadowed_css_names(&param.pat, &variable_group_names, &mut shadowed_variables);
            collect_shadowed_css_names(&param.pat, &const_group_names, &mut shadowed_consts);
        }
        if let Some(body) = &function.body {
            collect_shadowed_var_names_from_function_body(body, &self.css_names, &mut shadowed_css);
            collect_shadowed_var_names_from_function_body(
                body,
                &self.html_names,
                &mut shadowed_html,
            );
            collect_shadowed_var_names_from_function_body(
                body,
                &self.style_group_names,
                &mut shadowed_styles,
            );
            collect_shadowed_var_names_from_function_body(
                body,
                &variable_group_names,
                &mut shadowed_variables,
            );
            collect_shadowed_var_names_from_function_body(
                body,
                &const_group_names,
                &mut shadowed_consts,
            );
        }
        self.with_shadowed_names(
            &shadowed_css,
            &shadowed_html,
            &shadowed_styles,
            &shadowed_variables,
            &shadowed_consts,
            |transform| {
                transform.with_non_top_level(|transform| {
                    function.visit_mut_children_with(transform);
                });
            },
        );
    }

    fn visit_mut_arrow_expr(&mut self, arrow: &mut ArrowExpr) {
        let mut shadowed_css = HashSet::new();
        let mut shadowed_html = HashSet::new();
        let mut shadowed_styles = HashSet::new();
        let mut shadowed_variables = HashSet::new();
        let mut shadowed_consts = HashSet::new();
        let variable_group_names = self.variable_group_names();
        let const_group_names = self.const_group_names();
        for param in &arrow.params {
            collect_shadowed_css_names(param, &self.css_names, &mut shadowed_css);
            collect_shadowed_css_names(param, &self.html_names, &mut shadowed_html);
            collect_shadowed_css_names(param, &self.style_group_names, &mut shadowed_styles);
            collect_shadowed_css_names(param, &variable_group_names, &mut shadowed_variables);
            collect_shadowed_css_names(param, &const_group_names, &mut shadowed_consts);
        }
        if let swc_core::ecma::ast::BlockStmtOrExpr::BlockStmt(body) = &*arrow.body {
            collect_shadowed_var_names_from_function_body(body, &self.css_names, &mut shadowed_css);
            collect_shadowed_var_names_from_function_body(
                body,
                &self.html_names,
                &mut shadowed_html,
            );
            collect_shadowed_var_names_from_function_body(
                body,
                &self.style_group_names,
                &mut shadowed_styles,
            );
            collect_shadowed_var_names_from_function_body(
                body,
                &variable_group_names,
                &mut shadowed_variables,
            );
            collect_shadowed_var_names_from_function_body(
                body,
                &const_group_names,
                &mut shadowed_consts,
            );
        }
        self.with_shadowed_names(
            &shadowed_css,
            &shadowed_html,
            &shadowed_styles,
            &shadowed_variables,
            &shadowed_consts,
            |transform| {
                transform.with_non_top_level(|transform| {
                    arrow.visit_mut_children_with(transform);
                });
            },
        );
    }

    fn visit_mut_block_stmt(&mut self, block: &mut BlockStmt) {
        let mut shadowed_css = HashSet::new();
        let mut shadowed_html = HashSet::new();
        let mut shadowed_styles = HashSet::new();
        let mut shadowed_variables = HashSet::new();
        let mut shadowed_consts = HashSet::new();
        let variable_group_names = self.variable_group_names();
        let const_group_names = self.const_group_names();
        for statement in &block.stmts {
            collect_shadowed_css_names_from_statement(
                statement,
                &self.css_names,
                &mut shadowed_css,
            );
            collect_shadowed_css_names_from_statement(
                statement,
                &self.html_names,
                &mut shadowed_html,
            );
            collect_shadowed_css_names_from_statement(
                statement,
                &self.style_group_names,
                &mut shadowed_styles,
            );
            collect_shadowed_css_names_from_statement(
                statement,
                &variable_group_names,
                &mut shadowed_variables,
            );
            collect_shadowed_css_names_from_statement(
                statement,
                &const_group_names,
                &mut shadowed_consts,
            );
        }
        self.with_shadowed_names(
            &shadowed_css,
            &shadowed_html,
            &shadowed_styles,
            &shadowed_variables,
            &shadowed_consts,
            |transform| {
                transform.with_non_top_level(|transform| {
                    block.visit_mut_children_with(transform);
                    transform.insert_pending_style_declarations_in_statements(&mut block.stmts);
                    transform.remove_unused_style_declarations_from_statements(&mut block.stmts);
                });
            },
        );
    }

    fn visit_mut_class(&mut self, class: &mut Class) {
        self.with_non_top_level(|transform| {
            class.visit_mut_children_with(transform);
        });
    }

    fn visit_mut_catch_clause(&mut self, catch: &mut CatchClause) {
        let mut shadowed_css = HashSet::new();
        let mut shadowed_html = HashSet::new();
        let mut shadowed_styles = HashSet::new();
        let mut shadowed_variables = HashSet::new();
        let mut shadowed_consts = HashSet::new();
        let variable_group_names = self.variable_group_names();
        let const_group_names = self.const_group_names();
        if let Some(param) = &catch.param {
            collect_shadowed_css_names(param, &self.css_names, &mut shadowed_css);
            collect_shadowed_css_names(param, &self.html_names, &mut shadowed_html);
            collect_shadowed_css_names(param, &self.style_group_names, &mut shadowed_styles);
            collect_shadowed_css_names(param, &variable_group_names, &mut shadowed_variables);
            collect_shadowed_css_names(param, &const_group_names, &mut shadowed_consts);
        }
        self.with_shadowed_names(
            &shadowed_css,
            &shadowed_html,
            &shadowed_styles,
            &shadowed_variables,
            &shadowed_consts,
            |transform| {
                catch.visit_mut_children_with(transform);
            },
        );
    }

    fn visit_mut_for_stmt(&mut self, statement: &mut ForStmt) {
        let mut shadowed_css = HashSet::new();
        let mut shadowed_html = HashSet::new();
        let mut shadowed_styles = HashSet::new();
        let mut shadowed_variables = HashSet::new();
        let mut shadowed_consts = HashSet::new();
        let variable_group_names = self.variable_group_names();
        let const_group_names = self.const_group_names();
        if let Some(VarDeclOrExpr::VarDecl(declaration)) = &statement.init {
            collect_shadowed_names_from_var_decl(declaration, &self.css_names, &mut shadowed_css);
            collect_shadowed_names_from_var_decl(declaration, &self.html_names, &mut shadowed_html);
            collect_shadowed_names_from_var_decl(
                declaration,
                &self.style_group_names,
                &mut shadowed_styles,
            );
            collect_shadowed_names_from_var_decl(
                declaration,
                &variable_group_names,
                &mut shadowed_variables,
            );
            collect_shadowed_names_from_var_decl(
                declaration,
                &const_group_names,
                &mut shadowed_consts,
            );
        }
        self.with_shadowed_names(
            &shadowed_css,
            &shadowed_html,
            &shadowed_styles,
            &shadowed_variables,
            &shadowed_consts,
            |transform| {
                transform.with_non_top_level(|transform| {
                    statement.visit_mut_children_with(transform);
                });
            },
        );
    }

    fn visit_mut_for_in_stmt(&mut self, statement: &mut ForInStmt) {
        let mut shadowed_css = HashSet::new();
        let mut shadowed_html = HashSet::new();
        let mut shadowed_styles = HashSet::new();
        let mut shadowed_variables = HashSet::new();
        let mut shadowed_consts = HashSet::new();
        let variable_group_names = self.variable_group_names();
        let const_group_names = self.const_group_names();
        collect_shadowed_names_from_for_head(&statement.left, &self.css_names, &mut shadowed_css);
        collect_shadowed_names_from_for_head(&statement.left, &self.html_names, &mut shadowed_html);
        collect_shadowed_names_from_for_head(
            &statement.left,
            &self.style_group_names,
            &mut shadowed_styles,
        );
        collect_shadowed_names_from_for_head(
            &statement.left,
            &variable_group_names,
            &mut shadowed_variables,
        );
        collect_shadowed_names_from_for_head(
            &statement.left,
            &const_group_names,
            &mut shadowed_consts,
        );
        self.with_shadowed_names(
            &shadowed_css,
            &shadowed_html,
            &shadowed_styles,
            &shadowed_variables,
            &shadowed_consts,
            |transform| {
                transform.with_non_top_level(|transform| {
                    statement.visit_mut_children_with(transform);
                });
            },
        );
    }

    fn visit_mut_for_of_stmt(&mut self, statement: &mut ForOfStmt) {
        let mut shadowed_css = HashSet::new();
        let mut shadowed_html = HashSet::new();
        let mut shadowed_styles = HashSet::new();
        let mut shadowed_variables = HashSet::new();
        let mut shadowed_consts = HashSet::new();
        let variable_group_names = self.variable_group_names();
        let const_group_names = self.const_group_names();
        collect_shadowed_names_from_for_head(&statement.left, &self.css_names, &mut shadowed_css);
        collect_shadowed_names_from_for_head(&statement.left, &self.html_names, &mut shadowed_html);
        collect_shadowed_names_from_for_head(
            &statement.left,
            &self.style_group_names,
            &mut shadowed_styles,
        );
        collect_shadowed_names_from_for_head(
            &statement.left,
            &variable_group_names,
            &mut shadowed_variables,
        );
        collect_shadowed_names_from_for_head(
            &statement.left,
            &const_group_names,
            &mut shadowed_consts,
        );
        self.with_shadowed_names(
            &shadowed_css,
            &shadowed_html,
            &shadowed_styles,
            &shadowed_variables,
            &shadowed_consts,
            |transform| {
                transform.with_non_top_level(|transform| {
                    statement.visit_mut_children_with(transform);
                });
            },
        );
    }

    fn visit_mut_var_decl(&mut self, declaration: &mut VarDecl) {
        if declaration.decls.len() > 1 {
            for declarator in &declaration.decls {
                let Some(init) = &declarator.init else {
                    continue;
                };
                let Expr::Call(call) = &**init else {
                    continue;
                };
                if is_css_member_call(&self.css_names, &call.callee, "create") {
                    panic!(
                        "[nanocss] css.create(...) declarations must not share a variable declaration with other declarators."
                    );
                }
                if is_css_member_call(&self.css_names, &call.callee, "keyframes") {
                    panic!(
                        "[nanocss] css.keyframes(...) declarations must not share a variable declaration with other declarators."
                    );
                }
                if is_css_member_call(&self.css_names, &call.callee, "positionTry") {
                    panic!(
                        "[nanocss] css.positionTry(...) declarations must not share a variable declaration with other declarators."
                    );
                }
                if is_css_member_call(&self.css_names, &call.callee, "viewTransitionClass") {
                    panic!(
                        "[nanocss] css.viewTransitionClass(...) declarations must not share a variable declaration with other declarators."
                    );
                }
                if is_css_member_call(&self.css_names, &call.callee, "defineConsts") {
                    panic!(
                        "[nanocss] css.defineConsts(...) declarations must not share a variable declaration with other declarators."
                    );
                }
            }
        }

        for declarator in &mut declaration.decls {
            declarator.visit_mut_with(self);
            if self.non_top_level_depth == 0 && declaration.kind == VarDeclKind::Const {
                self.record_simple_constant_declarator(declarator);
            }
        }
    }

    fn visit_mut_var_declarator(&mut self, declarator: &mut VarDeclarator) {
        let binding_name = declarator
            .name
            .as_ident()
            .map(|binding| binding.id.sym.to_string())
            .unwrap_or_else(|| "unknown".to_string());
        let Some(init) = &mut declarator.init else {
            declarator.visit_mut_children_with(self);
            return;
        };
        let Expr::Call(call) = &**init else {
            declarator.visit_mut_children_with(self);
            return;
        };

        if is_css_member_call(&self.css_names, &call.callee, "defineVars") {
            if self.non_top_level_depth > 0 {
                panic!("[nanocss] css.defineVars(...) declarations must be at the top level.");
            }
            let replacement = compile_define_vars_declaration(
                binding_name,
                call,
                &self.css_names,
                &self.file_identity,
                &self.variable_groups,
                &self.imported_variable_group_names,
                &self.const_groups,
                &self.generated_string_names,
                &mut self.hook_compiler,
                &self.options.env,
                self.options.debug,
            );
            self.variable_defaults.extend(replacement.defaults);
            self.variable_properties.extend(replacement.properties);
            self.variable_groups
                .insert(replacement.group_name, replacement.group);

            **init = Expr::Object(replacement.init);
            return;
        }

        if is_css_member_call(&self.css_names, &call.callee, "defineConsts") {
            if self.non_top_level_depth > 0 {
                panic!("[nanocss] css.defineConsts(...) declarations must be at the top level.");
            }
            if !self.exported_const_group_names.contains(&binding_name) {
                panic!(
                    "[nanocss] css.defineConsts(...) declarations must be exported from *.css.ts files."
                );
            }
            if call.args.len() != 1 || call.args[0].spread.is_some() {
                panic!(
                    "[nanocss] css.defineConsts(...) must be called with a static object expression."
                );
            }
            let mut consts_arg = (*call.args[0].expr).clone();
            replace_env_references(&mut consts_arg, &self.css_names, &self.options.env);
            let group = parse_define_consts_arg(&consts_arg);
            self.const_groups.insert(binding_name, group.clone());
            **init = create_object_freeze_call(consts_arg);
            return;
        }

        if is_css_member_call(&self.css_names, &call.callee, "keyframes") {
            if self.non_top_level_depth > 0 {
                panic!("[nanocss] css.keyframes(...) declarations must be at the top level.");
            }
            if call.args.len() != 1 || call.args[0].spread.is_some() {
                panic!(
                    "[nanocss] css.keyframes(...) must be called with a static object expression."
                );
            };
            let mut keyframes_arg = (*call.args[0].expr).clone();
            replace_env_references(&mut keyframes_arg, &self.css_names, &self.options.env);
            let frames = parse_keyframes_arg(&keyframes_arg);
            let compiled = compile_keyframes(&frames, self.options.debug);
            let name = compiled.name.clone();
            self.keyframes.push(compiled);
            self.generated_string_names.insert(
                binding_name,
                GeneratedString::new(name.clone(), GeneratedStringKind::Keyframes),
            );
            **init = Expr::Lit(Lit::Str(Str {
                span: DUMMY_SP,
                value: name.into(),
                raw: None,
            }));
            return;
        }

        if is_css_member_call(&self.css_names, &call.callee, "positionTry") {
            if self.non_top_level_depth > 0 {
                panic!("[nanocss] css.positionTry(...) declarations must be at the top level.");
            }
            let compiled = compile_position_try(
                call,
                &self.css_names,
                &self.variable_groups,
                &self.imported_variable_group_names,
                &self.const_groups,
                &self.generated_string_names,
                &mut self.hook_compiler,
                &self.file_identity,
                &mut self.dynamic_hook_id,
                self.options.debug,
                &self.options.env,
            );
            let name = compiled.name.clone();
            self.position_tries.push(compiled);
            self.generated_string_names.insert(
                binding_name,
                GeneratedString::new(name.clone(), GeneratedStringKind::PositionTry),
            );
            **init = Expr::Lit(Lit::Str(Str {
                span: DUMMY_SP,
                value: name.into(),
                raw: None,
            }));
            return;
        }

        if is_css_member_call(&self.css_names, &call.callee, "viewTransitionClass") {
            if self.non_top_level_depth > 0 {
                panic!(
                    "[nanocss] css.viewTransitionClass(...) declarations must be at the top level."
                );
            }
            let compiled = compile_view_transition_class(
                call,
                &self.css_names,
                &self.variable_groups,
                &self.imported_variable_group_names,
                &self.const_groups,
                &self.generated_string_names,
                &mut self.hook_compiler,
                &self.file_identity,
                &mut self.dynamic_hook_id,
                self.options.debug,
                &self.options.env,
            );
            let name = compiled.name.clone();
            self.view_transition_classes.push(compiled);
            self.generated_string_names.insert(
                binding_name,
                GeneratedString::new(name.clone(), GeneratedStringKind::ViewTransitionClass),
            );
            **init = Expr::Lit(Lit::Str(Str {
                span: DUMMY_SP,
                value: name.into(),
                raw: None,
            }));
            return;
        }

        if is_css_member_call(&self.css_names, &call.callee, "createTheme") {
            if self.non_top_level_depth > 0 {
                panic!("[nanocss] css.createTheme(...) declarations must be at the top level.");
            }
            if call.args.len() != 2 || call.args.iter().any(|arg| arg.spread.is_some()) {
                panic!("[nanocss] css.createTheme(...) must be called with exactly two arguments.");
            }
            let group_arg = &call.args[0].expr;
            let local_group = match &**group_arg {
                Expr::Ident(group) => self.variable_groups.get(&group.sym.to_string()),
                _ => None,
            };
            **init = Expr::Object(compile_create_theme_call(
                group_arg,
                call,
                &self.css_names,
                local_group,
                &self.const_groups,
                &mut self.hook_compiler,
                &self.options.env,
            ));
            return;
        }

        if is_css_member_call(&self.css_names, &call.callee, "create") {
            if self.non_top_level_depth > 0 {
                panic!("[nanocss] css.create(...) declarations must be at the top level.");
            }
            if call.args.len() != 1 || call.args[0].spread.is_some() {
                panic!("[nanocss] css.create(...) must be called with a static object expression.");
            }

            let mut create_arg = (*call.args[0].expr).clone();
            inline_simple_constants(&mut create_arg, &self.simple_constants);
            self.style_group_names.insert(binding_name.clone());
            let compiled = parse_create_arg(
                &create_arg,
                &self.css_names,
                &self.variable_groups,
                &self.imported_variable_group_names,
                &self.const_groups,
                &mut self.hook_compiler,
                &self.file_identity,
                &mut self.dynamic_hook_id,
                self.options.debug,
                &self.options.env,
            );
            self.static_style_groups.insert(
                binding_name.clone(),
                collect_static_style_members(&compiled),
            );
            let dynamic_style_members = collect_dynamic_style_members(&compiled);
            let mut dynamic_style_members_by_name = HashMap::new();
            for (style_name, style) in dynamic_style_members {
                dynamic_style_members_by_name.insert(
                    style_name,
                    DynamicStyleMember {
                        helper_id: None,
                        init: style,
                    },
                );
            }
            self.dynamic_style_groups
                .insert(binding_name, dynamic_style_members_by_name);
            **init = Expr::Object(compiled);
            return;
        }

        declarator.visit_mut_children_with(self);
    }

    fn visit_mut_jsx_opening_element(&mut self, opening: &mut JSXOpeningElement) {
        let html_tag_name = get_html_tag_name(&self.html_names, &opening.name);
        let html_default_style = html_tag_name
            .as_ref()
            .and_then(|tag_name| self.html_default_style_id_for(tag_name));
        if let Some(tag_name) = &html_tag_name {
            opening.name = create_jsx_element_name(tag_name);
        }
        let is_html_element = html_tag_name.is_some();

        for attr in &mut opening.attrs {
            let replacement = match attr {
                JSXAttrOrSpread::SpreadElement(spread) => {
                    if let Expr::Call(call) = &*spread.expr
                        && is_css_member_call(&self.css_names, &call.callee, "props")
                    {
                        Some(JSXAttrOrSpread::JSXAttr(JSXAttr {
                            span: DUMMY_SP,
                            name: JSXAttrName::Ident("style".into()),
                            value: Some(JSXAttrValue::JSXExprContainer(JSXExprContainer {
                                span: DUMMY_SP,
                                expr: JSXExpr::Expr(Box::new(
                                    self.create_props_style_expression(&call.args),
                                )),
                            })),
                        }))
                    } else {
                        None
                    }
                }
                JSXAttrOrSpread::JSXAttr(attribute)
                    if !is_html_element && is_jsx_style_attr(attribute) =>
                {
                    if let Some(expression) = get_jsx_attribute_expression(attribute)
                        && contains_style_group_member_reference(
                            &self.style_group_names,
                            expression,
                        )
                    {
                        panic!(
                            "[nanocss] Compiled style objects cannot be referenced directly. Pass styles only to css.props(...)."
                        );
                    }
                    None
                }
                _ => None,
            };

            if let Some(replacement) = replacement {
                *attr = replacement;
            } else {
                attr.visit_mut_children_with(self);
            }
        }

        if let Some(tag_name) = html_tag_name {
            let html_spread_temp_names = (0..html_spread_temp_count(&opening.attrs))
                .map(|_| self.next_html_spread_temp_id())
                .collect::<Vec<_>>();
            let mut html_spread_temp_names_iter = html_spread_temp_names.iter().cloned();
            apply_html_default_style(
                &mut opening.attrs,
                html_default_style.as_deref(),
                &mut |expression| self.resolve_style_expression(expression),
                &mut || {
                    html_spread_temp_names_iter
                        .next()
                        .expect("expected html spread temp")
                },
            );
            if let Some(current_temps) = self.html_spread_temp_stack.last_mut() {
                current_temps.extend(html_spread_temp_names);
            }
            if let Some(source_attribute) = self.create_html_source_attribute(opening.span) {
                opening.attrs.insert(0, source_attribute);
            }
            if html_default_style.is_some() {
                self.pending_html_default_styles.insert(tag_name);
            }
        } else {
            collapse_duplicate_style_attributes(&mut opening.attrs);
        }
    }

    fn visit_mut_jsx_closing_element(&mut self, closing: &mut JSXClosingElement) {
        if let Some(tag_name) = get_html_tag_name(&self.html_names, &closing.name) {
            closing.name = create_jsx_element_name(&tag_name);
        }
    }

    fn visit_mut_jsx_element_child(&mut self, child: &mut JSXElementChild) {
        let replacement = match child {
            JSXElementChild::JSXElement(element) => {
                self.html_spread_temp_stack.push(Vec::new());
                element.visit_mut_children_with(self);
                let temp_names = self
                    .html_spread_temp_stack
                    .pop()
                    .expect("expected html spread temp scope");
                if temp_names.is_empty() {
                    None
                } else {
                    Some(JSXElementChild::JSXExprContainer(JSXExprContainer {
                        span: element.span,
                        expr: JSXExpr::Expr(Box::new(self.wrap_html_spread_temp_expression(
                            Expr::JSXElement(element.clone()),
                            temp_names,
                        ))),
                    }))
                }
            }
            JSXElementChild::JSXFragment(fragment) => {
                self.html_spread_temp_stack.push(Vec::new());
                fragment.visit_mut_children_with(self);
                let temp_names = self
                    .html_spread_temp_stack
                    .pop()
                    .expect("expected html spread temp scope");
                if temp_names.is_empty() {
                    None
                } else {
                    Some(JSXElementChild::JSXExprContainer(JSXExprContainer {
                        span: fragment.span,
                        expr: JSXExpr::Expr(Box::new(self.wrap_html_spread_temp_expression(
                            Expr::JSXFragment(fragment.clone()),
                            temp_names,
                        ))),
                    }))
                }
            }
            _ => {
                child.visit_mut_children_with(self);
                None
            }
        };

        if let Some(replacement) = replacement {
            *child = replacement;
        }
    }

    fn visit_mut_expr(&mut self, expression: &mut Expr) {
        let tracks_html_spread_temps =
            matches!(expression, Expr::JSXElement(_) | Expr::JSXFragment(_));
        if tracks_html_spread_temps {
            self.html_spread_temp_stack.push(Vec::new());
        }

        expression.visit_mut_children_with(self);

        if tracks_html_spread_temps {
            let temp_names = self
                .html_spread_temp_stack
                .pop()
                .expect("expected html spread temp scope");
            if !temp_names.is_empty() {
                *expression = self.wrap_html_spread_temp_expression(expression.clone(), temp_names);
                return;
            }
        }

        let Expr::Call(call) = expression else {
            return;
        };
        if is_css_member_call(&self.css_names, &call.callee, "create") {
            panic!("[nanocss] css.create(...) must be assigned to a variable declaration.");
        }
        if is_css_member_call(&self.css_names, &call.callee, "defineVars") {
            panic!("[nanocss] css.defineVars(...) must be assigned to a variable declaration.");
        }
        if is_css_member_call(&self.css_names, &call.callee, "defineConsts") {
            panic!("[nanocss] css.defineConsts(...) must be assigned to a variable declaration.");
        }
        if is_css_member_call(&self.css_names, &call.callee, "createTheme") {
            panic!("[nanocss] css.createTheme(...) must be assigned to a variable declaration.");
        }
        if is_css_member_call(&self.css_names, &call.callee, "positionTry") {
            panic!("[nanocss] css.positionTry(...) must be assigned to a variable declaration.");
        }
        if is_css_member_call(&self.css_names, &call.callee, "viewTransitionClass") {
            panic!(
                "[nanocss] css.viewTransitionClass(...) must be assigned to a variable declaration."
            );
        }
        if !is_css_member_call(&self.css_names, &call.callee, "keyframes") {
            if is_css_member_call(&self.css_names, &call.callee, "props") {
                *expression = self.create_props_object_expression(&call.args);
            }
            return;
        }
        if self.non_top_level_depth > 0 {
            panic!("[nanocss] css.keyframes(...) declarations must be at the top level.");
        }
        if call.args.len() != 1 || call.args[0].spread.is_some() {
            panic!("[nanocss] css.keyframes(...) must be called with a static object expression.");
        };
        let mut keyframes_arg = (*call.args[0].expr).clone();
        replace_env_references(&mut keyframes_arg, &self.css_names, &self.options.env);
        let frames = parse_keyframes_arg(&keyframes_arg);
        let compiled = compile_keyframes(&frames, self.options.debug);
        let name = compiled.name.clone();
        self.keyframes.push(compiled);
        *expression = Expr::Lit(Lit::Str(Str {
            span: DUMMY_SP,
            value: name.into(),
            raw: None,
        }));
    }

    fn visit_mut_prop(&mut self, property: &mut Prop) {
        let Prop::KeyValue(key_value) = property else {
            property.visit_mut_children_with(self);
            return;
        };

        key_value.visit_mut_children_with(self);

        if prop_name_to_string(&key_value.key).as_deref() != Some("style") {
            return;
        }

        if let Some(style) = self.resolve_style_expression(&key_value.value) {
            key_value.value = Box::new(style);
        }
    }
}

fn parse_input_source_map(value: Option<&serde_json::Value>) -> Option<DecodedMap> {
    let value = value?;
    let bytes = match value {
        serde_json::Value::String(source_map) => source_map.as_bytes().to_vec(),
        source_map => serde_json::to_vec(source_map).ok()?,
    };
    DecodedMap::from_reader(bytes.as_slice()).ok()
}

fn create_object_freeze_call(expression: Expr) -> Expr {
    Expr::Call(CallExpr {
        span: DUMMY_SP,
        ctxt: Default::default(),
        callee: Callee::Expr(Box::new(Expr::Member(MemberExpr {
            span: DUMMY_SP,
            obj: Box::new(Expr::Ident(Ident::new(
                "Object".into(),
                DUMMY_SP,
                Default::default(),
            ))),
            prop: MemberProp::Ident("freeze".into()),
        }))),
        args: vec![ExprOrSpread {
            spread: None,
            expr: Box::new(expression),
        }],
        type_args: None,
    })
}

fn simple_constant_expression(expression: &Expr) -> Option<Expr> {
    match expression {
        Expr::Lit(Lit::Str(_))
        | Expr::Lit(Lit::Num(_))
        | Expr::Lit(Lit::Bool(_))
        | Expr::Lit(Lit::Null(_)) => Some(expression.clone()),
        Expr::Paren(paren) => simple_constant_expression(&paren.expr),
        _ => None,
    }
}

fn inline_simple_constants(expression: &mut Expr, constants: &HashMap<String, Expr>) {
    if constants.is_empty() {
        return;
    }
    expression.visit_mut_with(&mut SimpleConstantInliner {
        constants,
        shadowed: Vec::new(),
    });
}

struct SimpleConstantInliner<'a> {
    constants: &'a HashMap<String, Expr>,
    shadowed: Vec<String>,
}

impl SimpleConstantInliner<'_> {
    fn is_visible_constant(&self, name: &str) -> bool {
        self.constants.contains_key(name) && !self.shadowed.iter().any(|shadowed| shadowed == name)
    }

    fn with_shadowed_names(&mut self, shadowed: HashSet<String>, visit: impl FnOnce(&mut Self)) {
        let previous_len = self.shadowed.len();
        self.shadowed.extend(shadowed);
        visit(self);
        self.shadowed.truncate(previous_len);
    }
}

impl VisitMut for SimpleConstantInliner<'_> {
    fn visit_mut_expr(&mut self, expression: &mut Expr) {
        if let Expr::Ident(identifier) = expression
            && self.is_visible_constant(identifier.sym.as_ref())
            && let Some(value) = self.constants.get(identifier.sym.as_ref())
        {
            *expression = value.clone();
            return;
        }

        match expression {
            Expr::Member(_) => {}
            _ => expression.visit_mut_children_with(self),
        }
    }

    fn visit_mut_call_expr(&mut self, call: &mut CallExpr) {
        for arg in &mut call.args {
            arg.expr.visit_mut_with(self);
        }
    }

    fn visit_mut_key_value_prop(&mut self, property: &mut KeyValueProp) {
        property.value.visit_mut_with(self);
    }

    fn visit_mut_prop(&mut self, property: &mut Prop) {
        if let Prop::Shorthand(identifier) = property
            && self.is_visible_constant(identifier.sym.as_ref())
            && let Some(value) = self.constants.get(identifier.sym.as_ref())
        {
            *property = Prop::KeyValue(KeyValueProp {
                key: PropName::Ident(identifier.sym.clone().into()),
                value: Box::new(value.clone()),
            });
            return;
        }

        property.visit_mut_children_with(self);
    }

    fn visit_mut_function(&mut self, function: &mut Function) {
        let mut shadowed = HashSet::new();
        let constant_names = self.constants.keys().cloned().collect::<HashSet<_>>();
        for param in &function.params {
            collect_shadowed_css_names(&param.pat, &constant_names, &mut shadowed);
        }
        if let Some(body) = &function.body {
            collect_shadowed_var_names_from_function_body(body, &constant_names, &mut shadowed);
        }
        self.with_shadowed_names(shadowed, |visitor| {
            function.visit_mut_children_with(visitor);
        });
    }

    fn visit_mut_arrow_expr(&mut self, arrow: &mut ArrowExpr) {
        let mut shadowed = HashSet::new();
        let constant_names = self.constants.keys().cloned().collect::<HashSet<_>>();
        for param in &arrow.params {
            collect_shadowed_css_names(param, &constant_names, &mut shadowed);
        }
        if let BlockStmtOrExpr::BlockStmt(body) = &*arrow.body {
            collect_shadowed_var_names_from_function_body(body, &constant_names, &mut shadowed);
        }
        self.with_shadowed_names(shadowed, |visitor| {
            arrow.visit_mut_children_with(visitor);
        });
    }
}

fn is_compiled_css_module_source(source: &str) -> bool {
    source.ends_with(".css")
        || source.ends_with(".css.ts")
        || source.ends_with(".css.tsx")
        || source.ends_with(".css.js")
        || source.ends_with(".css.jsx")
        || source.ends_with(".css.mjs")
        || source.ends_with(".css.cjs")
        || source.ends_with(".css.mts")
        || source.ends_with(".css.cts")
}

fn style_group_name_from_module_item(item: &ModuleItem) -> Option<String> {
    match item {
        ModuleItem::Stmt(statement) => style_group_name_from_statement(statement),
        ModuleItem::ModuleDecl(ModuleDecl::ExportDecl(export)) => {
            style_group_name_from_decl(&export.decl)
        }
        _ => None,
    }
}

fn style_group_name_from_statement(statement: &Stmt) -> Option<String> {
    let Stmt::Decl(declaration) = statement else {
        return None;
    };
    style_group_name_from_decl(declaration)
}

fn style_group_name_from_decl(declaration: &Decl) -> Option<String> {
    let Decl::Var(var_decl) = declaration else {
        return None;
    };
    if var_decl.decls.len() != 1 {
        return None;
    }
    var_decl.decls[0]
        .name
        .as_ident()
        .map(|binding| binding.id.sym.to_string())
}

fn collect_shadowed_names_from_for_head(
    head: &ForHead,
    names: &HashSet<String>,
    shadowed: &mut HashSet<String>,
) {
    match head {
        ForHead::VarDecl(declaration) => {
            collect_shadowed_names_from_var_decl(declaration, names, shadowed);
        }
        ForHead::UsingDecl(declaration) => {
            for declarator in &declaration.decls {
                collect_shadowed_css_names(&declarator.name, names, shadowed);
            }
        }
        ForHead::Pat(pattern) => {
            collect_shadowed_css_names(pattern, names, shadowed);
        }
        #[cfg(swc_ast_unknown)]
        _ => panic!("[nanocss] Unknown SWC for head."),
    }
}

fn collect_shadowed_names_from_var_decl(
    declaration: &VarDecl,
    names: &HashSet<String>,
    shadowed: &mut HashSet<String>,
) {
    for declarator in &declaration.decls {
        collect_shadowed_css_names(&declarator.name, names, shadowed);
    }
}

fn style_declaration_to_module_item(declaration: PendingStyleDeclaration) -> ModuleItem {
    ModuleItem::Stmt(style_declaration_to_statement(declaration))
}

fn style_declaration_to_statement(declaration: PendingStyleDeclaration) -> Stmt {
    Stmt::Decl(Decl::Var(Box::new(VarDecl {
        span: DUMMY_SP,
        ctxt: Default::default(),
        kind: VarDeclKind::Const,
        declare: false,
        decls: vec![VarDeclarator {
            span: DUMMY_SP,
            name: Pat::Ident(BindingIdent::from(Ident::new(
                declaration.id.into(),
                DUMMY_SP,
                Default::default(),
            ))),
            init: Some(Box::new(declaration.init)),
            definite: false,
        }],
    })))
}

fn collect_static_style_refs(
    expression: &Expr,
    transform: &NanoCssTransform,
    styles: &mut Vec<StaticStyleRef>,
) -> Option<()> {
    if is_falsy_style_expression(expression) {
        return Some(());
    }

    match expression {
        Expr::Member(_) => {
            styles.push(transform.resolve_static_style_ref(expression)?);
            Some(())
        }
        Expr::Array(array) => {
            for element in &array.elems {
                let Some(element) = element else {
                    continue;
                };
                if element.spread.is_some() {
                    return None;
                }
                collect_static_style_refs(&element.expr, transform, styles)?;
            }
            Some(())
        }
        Expr::Paren(paren) => collect_static_style_refs(&paren.expr, transform, styles),
        _ => None,
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

fn merge_property_key(key: &PropName, computed_index: &mut usize) -> Option<String> {
    if matches!(key, PropName::Computed(_)) {
        let key = format!("\0computed:{computed_index}");
        *computed_index += 1;
        return Some(key);
    }

    prop_name_to_string(key)
}

fn prop_or_spread_may_have_side_effects(property: &PropOrSpread) -> bool {
    let PropOrSpread::Prop(property) = property else {
        return true;
    };

    match &**property {
        Prop::KeyValue(property) => expr_may_have_side_effects(&property.value),
        Prop::Shorthand(_) => false,
        _ => true,
    }
}

fn expr_may_have_side_effects(expression: &Expr) -> bool {
    match expression {
        Expr::Lit(_) | Expr::Ident(_) | Expr::This(_) => false,
        Expr::Paren(expression) => expr_may_have_side_effects(&expression.expr),
        Expr::Unary(expression) if expression.op != UnaryOp::Delete => {
            expr_may_have_side_effects(&expression.arg)
        }
        Expr::Bin(expression) => {
            expr_may_have_side_effects(&expression.left)
                || expr_may_have_side_effects(&expression.right)
        }
        Expr::Cond(expression) => {
            expr_may_have_side_effects(&expression.test)
                || expr_may_have_side_effects(&expression.cons)
                || expr_may_have_side_effects(&expression.alt)
        }
        Expr::Tpl(expression) => expression
            .exprs
            .iter()
            .any(|expression| expr_may_have_side_effects(expression)),
        _ => true,
    }
}

fn collect_used_style_identifiers(module: &Module, targets: &HashSet<String>) -> HashSet<String> {
    let mut collector = StyleIdentifierUsageCollector {
        targets,
        used: HashSet::new(),
    };

    for item in &module.body {
        if matches!(item, ModuleItem::ModuleDecl(ModuleDecl::Import(_))) {
            continue;
        }
        item.visit_with(&mut collector);
    }

    collector.used
}

fn collect_used_style_identifiers_in_statements(
    statements: &[Stmt],
    targets: &HashSet<String>,
) -> HashSet<String> {
    let mut collector = StyleIdentifierUsageCollector {
        targets,
        used: HashSet::new(),
    };

    for statement in statements {
        statement.visit_with(&mut collector);
    }

    collector.used
}

struct StyleIdentifierUsageCollector<'a> {
    targets: &'a HashSet<String>,
    used: HashSet<String>,
}

impl Visit for StyleIdentifierUsageCollector<'_> {
    fn visit_binding_ident(&mut self, _binding: &BindingIdent) {}

    fn visit_ident(&mut self, ident: &Ident) {
        let name = ident.sym.to_string();
        if self.targets.contains(&name) {
            self.used.insert(name);
        }
    }
}

fn collect_used_simple_constant_identifiers(
    module: &Module,
    targets: &HashSet<String>,
) -> HashSet<String> {
    let mut collector = SimpleConstantUsageCollector {
        targets,
        used: HashSet::new(),
        shadowed: Vec::new(),
    };

    for item in &module.body {
        if matches!(item, ModuleItem::ModuleDecl(ModuleDecl::Import(_))) {
            continue;
        }
        item.visit_with(&mut collector);
    }

    collector.used
}

struct SimpleConstantUsageCollector<'a> {
    targets: &'a HashSet<String>,
    used: HashSet<String>,
    shadowed: Vec<String>,
}

impl SimpleConstantUsageCollector<'_> {
    fn is_visible_target(&self, name: &str) -> bool {
        self.targets.contains(name) && !self.shadowed.iter().any(|shadowed| shadowed == name)
    }

    fn with_shadowed_names(&mut self, shadowed: HashSet<String>, visit: impl FnOnce(&mut Self)) {
        let previous_len = self.shadowed.len();
        self.shadowed.extend(shadowed);
        visit(self);
        self.shadowed.truncate(previous_len);
    }
}

impl Visit for SimpleConstantUsageCollector<'_> {
    fn visit_binding_ident(&mut self, _binding: &BindingIdent) {}

    fn visit_ident(&mut self, ident: &Ident) {
        let name = ident.sym.to_string();
        if self.is_visible_target(&name) {
            self.used.insert(name);
        }
    }

    fn visit_function(&mut self, function: &Function) {
        let mut shadowed = HashSet::new();
        for param in &function.params {
            collect_shadowed_css_names(&param.pat, self.targets, &mut shadowed);
        }
        if let Some(body) = &function.body {
            collect_shadowed_var_names_from_function_body(body, self.targets, &mut shadowed);
        }
        self.with_shadowed_names(shadowed, |collector| {
            function.visit_children_with(collector);
        });
    }

    fn visit_arrow_expr(&mut self, arrow: &ArrowExpr) {
        let mut shadowed = HashSet::new();
        for param in &arrow.params {
            collect_shadowed_css_names(param, self.targets, &mut shadowed);
        }
        if let BlockStmtOrExpr::BlockStmt(body) = &*arrow.body {
            collect_shadowed_var_names_from_function_body(body, self.targets, &mut shadowed);
        }
        self.with_shadowed_names(shadowed, |collector| {
            arrow.visit_children_with(collector);
        });
    }
}

fn is_css_source_file(filename: &str) -> bool {
    filename.ends_with(".css.ts")
        || filename.ends_with(".css.tsx")
        || filename.ends_with(".css.mts")
        || filename.ends_with(".css.cts")
}

fn collect_static_style_members(styles: &swc_core::ecma::ast::ObjectLit) -> HashMap<String, Expr> {
    let mut members = HashMap::new();
    for property in &styles.props {
        let PropOrSpread::Prop(property) = property else {
            continue;
        };
        let Prop::KeyValue(property) = &**property else {
            continue;
        };
        let Some(style_name) = prop_name_to_string(&property.key) else {
            continue;
        };
        if matches!(&*property.value, Expr::Object(_)) {
            members.insert(style_name, (*property.value).clone());
        }
    }
    members
}

fn collect_dynamic_style_members(styles: &swc_core::ecma::ast::ObjectLit) -> Vec<(String, Expr)> {
    let mut members = Vec::new();
    for property in &styles.props {
        let PropOrSpread::Prop(property) = property else {
            continue;
        };
        let Prop::KeyValue(property) = &**property else {
            continue;
        };
        let Some(style_name) = prop_name_to_string(&property.key) else {
            continue;
        };
        if matches!(&*property.value, Expr::Arrow(_)) {
            members.push((style_name, (*property.value).clone()));
        }
    }
    members
}

fn helper_name_fragment(value: &str) -> String {
    let mut fragment = String::new();
    let mut capitalize_next = true;
    for character in value.chars() {
        if !character.is_ascii_alphanumeric() {
            capitalize_next = true;
            continue;
        }
        if capitalize_next {
            fragment.push(character.to_ascii_uppercase());
            capitalize_next = false;
        } else {
            fragment.push(character);
        }
    }
    if fragment.is_empty() {
        "Dynamic".to_string()
    } else {
        fragment
    }
}

fn member_prop_to_string(property: &MemberProp) -> Option<String> {
    match property {
        MemberProp::Ident(ident) => Some(ident.sym.to_string()),
        MemberProp::Computed(computed) => match &*computed.expr {
            Expr::Lit(Lit::Str(value)) => value.value.as_str().map(ToString::to_string),
            _ => None,
        },
        MemberProp::PrivateName(_) => None,
        #[cfg(swc_ast_unknown)]
        _ => panic!("[nanocss] Unknown SWC member property."),
    }
}

fn is_style_composition_operator(operator: BinaryOp) -> bool {
    matches!(
        operator,
        BinaryOp::LogicalAnd | BinaryOp::LogicalOr | BinaryOp::NullishCoalescing
    )
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use crate::jsx::is_jsx_style_attribute;
    use swc_core::{
        common::{SourceMap, sync::Lrc},
        ecma::{
            ast::{
                Decl, JSXElementName, Lit, ModuleItem, Prop, PropName, PropOrSpread, Stmt,
                VarDeclKind,
            },
            codegen::{Emitter, text_writer::JsWriter},
            parser::{EsSyntax, Parser, StringInput, Syntax},
            visit::VisitMutWith,
        },
    };

    use super::*;

    fn debug_options() -> TransformOptions {
        TransformOptions {
            debug: true,
            ..Default::default()
        }
    }

    fn debug_options_with_html_defaults() -> TransformOptions {
        let mut options = debug_options();
        options.html_defaults.insert(
            "div".to_string(),
            BTreeMap::from([(
                "boxSizing".to_string(),
                serde_json::Value::String("border-box".to_string()),
            )]),
        );
        options
    }

    fn parse_module(source: &str) -> Module {
        let source_map: Lrc<SourceMap> = Default::default();
        let file =
            source_map.new_source_file(swc_core::common::FileName::Anon.into(), source.to_string());
        let input = StringInput::from(&*file);
        let mut parser = Parser::new(
            Syntax::Es(EsSyntax {
                jsx: true,
                ..Default::default()
            }),
            input,
            None,
        );
        parser.parse_module().expect("source should parse")
    }

    fn emit_module(module: &Module) -> String {
        let source_map: Lrc<SourceMap> = Default::default();
        let mut output = Vec::new();
        {
            let writer = JsWriter::new(source_map.clone(), "\n", &mut output, None);
            let mut emitter = Emitter {
                cfg: Default::default(),
                comments: None,
                cm: source_map,
                wr: writer,
            };
            emitter.emit_module(module).expect("module should emit");
        }
        String::from_utf8(output).expect("output should be utf8")
    }

    fn get_var_decl(module: &Module, index: usize) -> &VarDecl {
        module
            .body
            .iter()
            .filter_map(|item| {
                let ModuleItem::Stmt(Stmt::Decl(Decl::Var(var_decl))) = item else {
                    return None;
                };
                Some(&**var_decl)
            })
            .nth(index)
            .unwrap_or_else(|| panic!("expected variable declaration at index {index}"))
    }

    fn get_jsx_var_decl(module: &Module) -> &VarDecl {
        module
            .body
            .iter()
            .filter_map(|item| {
                let ModuleItem::Stmt(Stmt::Decl(Decl::Var(var_decl))) = item else {
                    return None;
                };
                let Some(declarator) = var_decl.decls.first() else {
                    return None;
                };
                let Some(init) = &declarator.init else {
                    return None;
                };
                if matches!(&**init, Expr::JSXElement(_)) {
                    Some(&**var_decl)
                } else {
                    None
                }
            })
            .next()
            .expect("expected jsx variable declaration")
    }

    fn get_fn_decl(module: &Module, index: usize) -> &swc_core::ecma::ast::FnDecl {
        module
            .body
            .iter()
            .filter_map(|item| {
                let ModuleItem::Stmt(Stmt::Decl(Decl::Fn(function))) = item else {
                    return None;
                };
                Some(function)
            })
            .nth(index)
            .unwrap_or_else(|| panic!("expected function declaration at index {index}"))
    }

    fn nanocss_import_specifiers(module: &Module) -> Vec<String> {
        module
            .body
            .iter()
            .filter_map(|item| {
                let ModuleItem::ModuleDecl(ModuleDecl::Import(import_decl)) = item else {
                    return None;
                };
                if import_decl.src.value.as_str() != Some("nanocss-compiler") {
                    return None;
                }
                Some(
                    import_decl
                        .specifiers
                        .iter()
                        .filter_map(|specifier| get_named_import(specifier).map(|(_, local)| local))
                        .collect::<Vec<_>>(),
                )
            })
            .flatten()
            .collect()
    }

    #[test]
    fn replaces_static_keyframes_calls() {
        let mut module = parse_module(
            r#"
              import { css } from 'nanocss-compiler';
              const fadeIn = css.keyframes({
                '0%': { opacity: 0 },
                '100%': { opacity: 1 }
              });
            "#,
        );
        let mut transform = NanoCssTransform::new(debug_options(), "test.tsx".to_string());
        module.visit_mut_with(&mut transform);

        assert_eq!(transform.keyframes.len(), 1);
        assert_eq!(transform.keyframes[0].name, "__nanocss_keyframes-1ii5yk");

        let var_decl = get_var_decl(&module, 0);
        assert_eq!(var_decl.kind, VarDeclKind::Const);
        let init = var_decl.decls[0].init.as_ref().expect("expected init");
        let Expr::Lit(Lit::Str(value)) = &**init else {
            panic!("expected string literal replacement")
        };
        assert_eq!(value.value.as_str(), Some("__nanocss_keyframes-1ii5yk"));
    }

    #[test]
    #[should_panic(
        expected = "[nanocss] css.keyframes(...) declarations must not share a variable declaration with other declarators."
    )]
    fn rejects_keyframes_calls_in_multi_declarator_declarations() {
        let mut module = parse_module(
            r#"
              import { css } from 'nanocss-compiler';
              const fadeIn = css.keyframes({
                from: { opacity: 0 }
              }), other = 1;
            "#,
        );
        let mut transform = NanoCssTransform::new(debug_options(), "test.tsx".to_string());
        module.visit_mut_with(&mut transform);
    }

    #[test]
    #[should_panic(
        expected = "[nanocss] css.keyframes(...) must be called with a static object expression."
    )]
    fn rejects_dynamic_keyframes_arguments() {
        let mut module = parse_module(
            r#"
              import { css } from 'nanocss-compiler';
              const fadeIn = css.keyframes(frames);
            "#,
        );
        let mut transform = NanoCssTransform::new(debug_options(), "test.tsx".to_string());
        module.visit_mut_with(&mut transform);
    }

    #[test]
    #[should_panic(expected = "[nanocss] keyframe style objects cannot contain spreads.")]
    fn rejects_keyframe_style_spreads() {
        let mut module = parse_module(
            r#"
              import { css } from 'nanocss-compiler';
              const fadeIn = css.keyframes({
                from: {
                  ...style
                }
              });
            "#,
        );
        let mut transform = NanoCssTransform::new(debug_options(), "test.tsx".to_string());
        module.visit_mut_with(&mut transform);
    }

    #[test]
    #[should_panic(
        expected = "[nanocss] css.keyframes(...) values must be static string or number literals."
    )]
    fn rejects_dynamic_keyframe_style_values() {
        let mut module = parse_module(
            r#"
              import { css } from 'nanocss-compiler';
              const fadeIn = css.keyframes({
                from: {
                  opacity: value
                }
              });
            "#,
        );
        let mut transform = NanoCssTransform::new(debug_options(), "test.tsx".to_string());
        module.visit_mut_with(&mut transform);
    }

    #[test]
    fn uses_production_keyframe_names_by_default() {
        let mut module = parse_module(
            r#"
              import { css as c } from 'nanocss-compiler';
              const fadeIn = c.keyframes({ from: { opacity: 0 } });
            "#,
        );
        let mut transform =
            NanoCssTransform::new(TransformOptions::default(), "test.tsx".to_string());
        module.visit_mut_with(&mut transform);

        assert_eq!(transform.keyframes.len(), 1);
        assert_eq!(transform.keyframes[0].name, "nk-32hisd");
    }

    #[test]
    fn supports_aliased_css_imports() {
        let mut module = parse_module(
            r#"
              import { css as c } from 'nanocss-compiler';
              const fadeIn = c.keyframes({ from: { opacity: 0 } });
            "#,
        );
        let mut transform = NanoCssTransform::new(debug_options(), "test.tsx".to_string());
        module.visit_mut_with(&mut transform);

        assert_eq!(transform.keyframes.len(), 1);
    }

    #[test]
    fn supports_custom_import_sources() {
        let mut module = parse_module(
            r#"
              import { css } from '@/lib/nanocss';
              const fadeIn = css.keyframes({ from: { opacity: 0 } });
            "#,
        );
        let mut transform = NanoCssTransform::new(
            TransformOptions {
                debug: true,
                import_sources: vec!["@/lib/nanocss".to_string()],
                input_source_map: None,
                html_defaults: Default::default(),
                env: serde_json::Value::Object(Default::default()),
            },
            "test.tsx".to_string(),
        );
        module.visit_mut_with(&mut transform);

        assert_eq!(transform.keyframes.len(), 1);
    }

    #[test]
    fn removes_compiled_away_nanocss_imports() {
        let mut module = parse_module(
            r#"
              import { css, html } from 'nanocss-compiler';
              const styles = css.create({
                root: { opacity: 1 }
              });
              const element = <html.div {...css.props(styles.root)} />;
            "#,
        );
        let mut transform = NanoCssTransform::new(debug_options(), "src/app.tsx".to_string());
        module.visit_mut_with(&mut transform);

        assert!(nanocss_import_specifiers(&module).is_empty());
    }

    #[test]
    fn keeps_referenced_nanocss_import_specifiers() {
        let mut module = parse_module(
            r#"
              import { css, html } from 'nanocss-compiler';
              const value = css;
              const element = <html.div />;
            "#,
        );
        let mut transform = NanoCssTransform::new(debug_options(), "src/app.tsx".to_string());
        module.visit_mut_with(&mut transform);

        assert_eq!(nanocss_import_specifiers(&module), vec!["css"]);
    }

    #[test]
    fn lowers_html_member_elements_to_dom_elements() {
        let mut module = parse_module(
            r#"
              import { html } from 'nanocss-compiler';
              const element = <html.div id="x">Hello</html.div>;
            "#,
        );
        let mut transform = NanoCssTransform::new(debug_options(), "src/app.tsx".to_string());
        module.visit_mut_with(&mut transform);

        let var_decl = get_var_decl(&module, 0);
        let init = var_decl.decls[0].init.as_ref().expect("expected init");
        let Expr::JSXElement(element) = &**init else {
            panic!("expected jsx element")
        };
        let JSXElementName::Ident(opening) = &element.opening.name else {
            panic!("expected opening element to be lowered")
        };
        assert_eq!(opening.sym.as_str(), "div");
        let closing = element.closing.as_ref().expect("expected closing element");
        let JSXElementName::Ident(closing) = &closing.name else {
            panic!("expected closing element to be lowered")
        };
        assert_eq!(closing.sym.as_str(), "div");
        assert!(!element.opening.attrs.iter().any(is_jsx_style_attribute));
    }

    #[test]
    fn supports_aliased_html_imports() {
        let mut module = parse_module(
            r#"
              import { html as h } from 'nanocss-compiler';
              const element = <h.div />;
            "#,
        );
        let mut transform = NanoCssTransform::new(debug_options(), "src/app.tsx".to_string());
        module.visit_mut_with(&mut transform);

        let var_decl = get_var_decl(&module, 0);
        let init = var_decl.decls[0].init.as_ref().expect("expected init");
        let Expr::JSXElement(element) = &**init else {
            panic!("expected jsx element")
        };
        let JSXElementName::Ident(opening) = &element.opening.name else {
            panic!("expected opening element to be lowered")
        };
        assert_eq!(opening.sym.as_str(), "div");
    }

    #[test]
    fn does_not_transform_shadowed_html_parameters() {
        let mut module = parse_module(
            r#"
              import { html } from 'nanocss-compiler';
              function Comp(html) {
                return <html.div />;
              }
            "#,
        );
        let mut transform = NanoCssTransform::new(
            debug_options_with_html_defaults(),
            "src/app.tsx".to_string(),
        );
        module.visit_mut_with(&mut transform);

        let function = get_fn_decl(&module, 0);
        let return_statement = &function
            .function
            .body
            .as_ref()
            .expect("expected function body")
            .stmts[0];
        let Stmt::Return(return_statement) = return_statement else {
            panic!("expected return statement")
        };
        let Expr::JSXElement(element) = &**return_statement
            .arg
            .as_ref()
            .expect("expected return argument")
        else {
            panic!("expected jsx return")
        };
        assert!(matches!(
            element.opening.name,
            JSXElementName::JSXMemberExpr(_)
        ));
    }

    #[test]
    fn merges_html_default_style_with_explicit_style() {
        let mut module = parse_module(
            r#"
              import { html } from 'nanocss-compiler';
              const element = <html.div style={style}>Hello</html.div>;
            "#,
        );
        let mut transform = NanoCssTransform::new(
            debug_options_with_html_defaults(),
            "src/app.tsx".to_string(),
        );
        module.visit_mut_with(&mut transform);

        let var_decl = get_var_decl(&module, 1);
        let init = var_decl.decls[0].init.as_ref().expect("expected init");
        let Expr::JSXElement(element) = &**init else {
            panic!("expected jsx element")
        };
        let Some(JSXAttrOrSpread::JSXAttr(style_attr)) = element.opening.attrs.last() else {
            panic!("expected style attribute")
        };
        let Some(JSXAttrValue::JSXExprContainer(container)) = &style_attr.value else {
            panic!("expected style expression")
        };
        let JSXExpr::Expr(expression) = &container.expr else {
            panic!("expected style expression")
        };
        let Expr::Object(style) = &**expression else {
            panic!("expected merged style object")
        };
        assert_eq!(style.props.len(), 2);
    }

    #[test]
    fn merges_html_default_style_with_spread_props_style() {
        let mut module = parse_module(
            r#"
              import { html } from 'nanocss-compiler';
              const element = <html.div {...extraProps}>Hello</html.div>;
            "#,
        );
        let mut transform = NanoCssTransform::new(
            debug_options_with_html_defaults(),
            "src/app.tsx".to_string(),
        );
        module.visit_mut_with(&mut transform);

        let var_decl = get_var_decl(&module, 1);
        let init = var_decl.decls[0].init.as_ref().expect("expected init");
        let Expr::JSXElement(element) = &**init else {
            panic!("expected jsx element")
        };

        assert!(matches!(
            element.opening.attrs[0],
            JSXAttrOrSpread::SpreadElement(_)
        ));
        let Some(JSXAttrOrSpread::JSXAttr(style_attr)) = element.opening.attrs.last() else {
            panic!("expected final style attribute")
        };
        let Some(JSXAttrValue::JSXExprContainer(container)) = &style_attr.value else {
            panic!("expected style expression")
        };
        let JSXExpr::Expr(expression) = &container.expr else {
            panic!("expected style expression")
        };
        let Expr::Object(style) = &**expression else {
            panic!("expected merged style object")
        };
        assert_eq!(style.props.len(), 2);
    }

    #[test]
    fn flattens_html_style_arrays_with_default_style() {
        let mut module = parse_module(
            r#"
              import { css, html } from 'nanocss-compiler';
              const styles = css.create({
                root: { opacity: 1 },
                active: { color: 'red' }
              });
              const element = <html.div style={[styles.root, styles.active]} />;
            "#,
        );
        let mut transform = NanoCssTransform::new(
            debug_options_with_html_defaults(),
            "src/app.tsx".to_string(),
        );
        module.visit_mut_with(&mut transform);

        let var_decl = get_jsx_var_decl(&module);
        let init = var_decl.decls[0].init.as_ref().expect("expected init");
        let Expr::JSXElement(element) = &**init else {
            panic!("expected jsx element")
        };
        let Some(JSXAttrOrSpread::JSXAttr(style_attr)) = element.opening.attrs.last() else {
            panic!("expected style attribute")
        };
        let Some(JSXAttrValue::JSXExprContainer(container)) = &style_attr.value else {
            panic!("expected style expression")
        };
        let JSXExpr::Expr(expression) = &container.expr else {
            panic!("expected style expression")
        };
        let Expr::Object(style) = &**expression else {
            panic!("expected merged style object")
        };
        assert_eq!(style.props.len(), 3);
    }

    #[test]
    fn emits_valid_html_spread_style_order() {
        let mut module = parse_module(
            r#"
              import { css, html } from 'nanocss-compiler';
              const styles = css.create({
                root: { opacity: 1 }
              });
              const element = <html.div {...extraProps} style={styles.root} {...extraProps2} />;
            "#,
        );
        let mut transform = NanoCssTransform::new(
            debug_options_with_html_defaults(),
            "src/app.tsx".to_string(),
        );
        module.visit_mut_with(&mut transform);

        let output = emit_module(&module);
        assert!(output.contains("const _htmlDivDefaultStyle"));
        assert!(output.contains("<div {...extraProps} {...extraProps2} style={{"));
        assert!(output.contains("...extraProps?.style"));
        assert!(output.contains("...extraProps2?.style"));
        assert!(!output.contains("html.div"));
        assert!(!output.contains("css.create"));
    }

    #[test]
    fn guards_html_spread_props_style_access() {
        let mut module = parse_module(
            r#"
              import { html } from 'nanocss-compiler';
              const element = <html.div {...maybeProps} />;
            "#,
        );
        let mut transform = NanoCssTransform::new(
            debug_options_with_html_defaults(),
            "src/app.tsx".to_string(),
        );
        module.visit_mut_with(&mut transform);

        let output = emit_module(&module);
        assert!(output.contains("...maybeProps"));
        assert!(output.contains("...maybeProps?.style"));
    }

    #[test]
    fn guards_all_html_spread_props_style_accesses() {
        let mut module = parse_module(
            r#"
              import { html } from 'nanocss-compiler';
              const element = <html.div {...firstProps} {...secondProps} />;
            "#,
        );
        let mut transform = NanoCssTransform::new(debug_options(), "src/app.tsx".to_string());
        module.visit_mut_with(&mut transform);

        let output = emit_module(&module);
        assert!(output.contains("...firstProps?.style"));
        assert!(output.contains("...secondProps?.style"));
    }

    #[test]
    fn evaluates_html_spread_call_props_once() {
        let mut module = parse_module(
            r#"
              import { html } from 'nanocss-compiler';
              const element = <html.div {...getProps()} />;
            "#,
        );
        let mut transform = NanoCssTransform::new(debug_options(), "src/app.tsx".to_string());
        module.visit_mut_with(&mut transform);

        let output = emit_module(&module);
        assert!(output.contains("let _htmlProps;"));
        assert!(output.contains("{..._htmlProps = getProps()}"));
        assert!(output.contains("style={_htmlProps?.style}"));
        assert!(!output.contains("getProps()?.style"));
    }

    #[test]
    fn preserves_html_spread_call_evaluation_order() {
        let mut module = parse_module(
            r#"
              import { html } from 'nanocss-compiler';
              const element = <html.div {...first()} id={second()} {...third()} />;
            "#,
        );
        let mut transform = NanoCssTransform::new(debug_options(), "src/app.tsx".to_string());
        module.visit_mut_with(&mut transform);

        let output = emit_module(&module);
        let first = output
            .find("{..._htmlProps = first()}")
            .expect("expected first spread assignment");
        let second = output.find("id={second()}").expect("expected id attribute");
        let third = output
            .find("{..._htmlProps2 = third()}")
            .expect("expected third spread assignment");
        let style = output.find("style={{").expect("expected style attribute");
        assert!(first < second);
        assert!(second < third);
        assert!(third < style);
    }

    #[test]
    fn generated_html_spread_temps_do_not_conflict_with_existing_bindings() {
        let mut module = parse_module(
            r#"
              import { html } from 'nanocss-compiler';
              function Comp(_htmlProps) {
                const _htmlProps2 = {};
                return <html.div {...getProps()} />;
              }
            "#,
        );
        let mut transform = NanoCssTransform::new(debug_options(), "src/app.tsx".to_string());
        module.visit_mut_with(&mut transform);

        let output = emit_module(&module);
        assert!(output.contains("let _htmlProps3;"));
        assert!(output.contains("{..._htmlProps3 = getProps()}"));
        assert!(output.contains("style={_htmlProps3?.style}"));
    }

    #[test]
    fn localizes_html_spread_temps_to_nested_children() {
        let mut module = parse_module(
            r#"
              import { html } from 'nanocss-compiler';
              const element = <html.main><html.a {...getProps()} /></html.main>;
            "#,
        );
        let mut transform = NanoCssTransform::new(debug_options(), "src/app.tsx".to_string());
        module.visit_mut_with(&mut transform);

        let output = emit_module(&module);
        assert!(output.contains("const element = <main"));
        assert!(output.contains("{(()=>"));
        assert!(output.contains("let _htmlProps;"));
        assert!(output.contains("<a"));
        assert!(output.contains("{..._htmlProps = getProps()}"));
        assert!(output.contains("style={_htmlProps?.style}"));
        assert!(!output.contains("const element = (()=>"));
    }

    #[test]
    fn does_not_transform_shadowed_css_function_parameters() {
        let mut module = parse_module(
            r#"
              import { css } from 'nanocss-compiler';
              const styles = css.create({
                root: { opacity: 1 }
              });
              function Comp(css) {
                return css.props(styles.root);
              }
            "#,
        );
        let mut transform = NanoCssTransform::new(debug_options(), "src/app.tsx".to_string());
        module.visit_mut_with(&mut transform);

        let function = get_fn_decl(&module, 0);
        let return_statement = &function
            .function
            .body
            .as_ref()
            .expect("expected function body")
            .stmts[0];
        let Stmt::Return(return_statement) = return_statement else {
            panic!("expected return statement")
        };
        let Expr::Call(_) = &**return_statement
            .arg
            .as_ref()
            .expect("expected return argument")
        else {
            panic!("expected shadowed css.props call to remain")
        };
    }

    #[test]
    fn does_not_transform_shadowed_css_arrow_parameters() {
        let mut module = parse_module(
            r#"
              import { css } from 'nanocss-compiler';
              const styles = css.create({
                root: { opacity: 1 }
              });
              const Comp = css => css.props(styles.root);
            "#,
        );
        let mut transform = NanoCssTransform::new(debug_options(), "src/app.tsx".to_string());
        module.visit_mut_with(&mut transform);

        let var_decl = get_var_decl(&module, 1);
        let init = var_decl.decls[0].init.as_ref().expect("expected init");
        let Expr::Arrow(arrow) = &**init else {
            panic!("expected arrow expression")
        };
        let swc_core::ecma::ast::BlockStmtOrExpr::Expr(body) = &*arrow.body else {
            panic!("expected expression body")
        };
        let Expr::Call(_) = &**body else {
            panic!("expected shadowed css.props call to remain")
        };
    }

    #[test]
    fn does_not_transform_shadowed_css_block_bindings() {
        let mut module = parse_module(
            r#"
              import { css } from 'nanocss-compiler';
              const styles = css.create({
                root: { opacity: 1 }
              });
              function Comp() {
                const css = other;
                return css.props(styles.root);
              }
            "#,
        );
        let mut transform = NanoCssTransform::new(debug_options(), "src/app.tsx".to_string());
        module.visit_mut_with(&mut transform);

        let function = get_fn_decl(&module, 0);
        let return_statement = &function
            .function
            .body
            .as_ref()
            .expect("expected function body")
            .stmts[1];
        let Stmt::Return(return_statement) = return_statement else {
            panic!("expected return statement")
        };
        let Expr::Call(_) = &**return_statement
            .arg
            .as_ref()
            .expect("expected return argument")
        else {
            panic!("expected shadowed css.props call to remain")
        };
    }

    #[test]
    fn inserts_merged_style_declarations_after_exported_create_declarations() {
        let mut module = parse_module(
            r#"
              import { css } from 'nanocss-compiler';
              export const styles = css.create({
                root: { opacity: 1 }
              });
              const rootProps = css.props(styles.root);
            "#,
        );
        let mut transform = NanoCssTransform::new(debug_options(), "src/app.tsx".to_string());
        module.visit_mut_with(&mut transform);

        let output = emit_module(&module);
        assert!(output.contains("export const styles = {"));
        assert!(output.contains("const _stylesRoot = {"));
        assert!(output.contains("style: _stylesRoot"));
    }

    #[test]
    fn generated_style_helpers_do_not_conflict_with_existing_bindings() {
        let mut module = parse_module(
            r#"
              import { css } from 'nanocss-compiler';
              const _stylesRoot = { color: 'blue' };
              const styles = css.create({
                root: { opacity: 1 }
              });
              function Comp() {
                const _stylesRoot2 = { color: 'red' };
                return <div {...css.props(styles.root)} />;
              }
            "#,
        );
        let mut transform = NanoCssTransform::new(debug_options(), "src/app.tsx".to_string());
        module.visit_mut_with(&mut transform);

        let output = emit_module(&module);
        assert!(output.contains("const _stylesRoot3 = {"));
        assert!(output.contains("<div style={_stylesRoot3}/>"));
    }

    #[test]
    fn generated_html_helpers_do_not_conflict_with_existing_bindings() {
        let mut module = parse_module(
            r#"
              import { html } from 'nanocss-compiler';
              const _htmlDivDefaultStyle = { color: 'blue' };
              function Comp() {
                const _htmlDivDefaultStyle2 = { color: 'red' };
                return <html.div />;
              }
            "#,
        );
        let mut transform = NanoCssTransform::new(
            debug_options_with_html_defaults(),
            "src/app.tsx".to_string(),
        );
        module.visit_mut_with(&mut transform);

        let output = emit_module(&module);
        assert!(output.contains("const _htmlDivDefaultStyle3 = {"));
        assert!(output.contains("<div style={_htmlDivDefaultStyle3}/>"));
    }

    #[test]
    fn replaces_static_define_vars_calls() {
        let mut module = parse_module(
            r#"
              import { css } from 'nanocss-compiler';
              const colors = css.defineVars({
                primary: 'green',
                selected: true,
                empty: null,
                '--brand': 'red'
              });
            "#,
        );
        let mut transform = NanoCssTransform::new(debug_options(), "src/tokens.css.ts".to_string());
        module.visit_mut_with(&mut transform);

        assert_eq!(
            transform.variable_defaults,
            vec![
                CompiledVariableDefault {
                    custom_property_name: "--_nanocss_var_colors_primary_vec0x7--n-default"
                        .to_string(),
                    value: "green".to_string(),
                },
                CompiledVariableDefault {
                    custom_property_name: "--_nanocss_var_colors_selected_hxhonm--n-default"
                        .to_string(),
                    value: "true".to_string(),
                },
                CompiledVariableDefault {
                    custom_property_name: "--brand--n-default".to_string(),
                    value: "red".to_string(),
                },
            ]
        );

        let var_decl = get_var_decl(&module, 0);
        let init = var_decl.decls[0].init.as_ref().expect("expected init");
        let Expr::Object(object) = &**init else {
            panic!("expected object literal replacement")
        };
        assert_eq!(object.props.len(), 5);
    }

    #[test]
    #[should_panic(
        expected = "[nanocss] css.defineVars(...) must be assigned to a variable declaration."
    )]
    fn rejects_unassigned_define_vars_calls() {
        let mut module = parse_module(
            r#"
              import { css } from 'nanocss-compiler';
              css.defineVars({
                primary: 'green'
              });
            "#,
        );
        let mut transform = NanoCssTransform::new(debug_options(), "src/tokens.css.ts".to_string());
        module.visit_mut_with(&mut transform);
    }

    #[test]
    fn replaces_local_create_theme_calls() {
        let mut module = parse_module(
            r#"
              import { css } from 'nanocss-compiler';
              const colors = css.defineVars({
                primary: 'green',
                empty: null
              });
              const theme = css.createTheme(colors, {
                primary: 'purple',
                empty: null
              });
            "#,
        );
        let mut transform = NanoCssTransform::new(debug_options(), "src/tokens.css.ts".to_string());
        module.visit_mut_with(&mut transform);

        let var_decl = get_var_decl(&module, 1);
        let init = var_decl.decls[0].init.as_ref().expect("expected init");
        let Expr::Object(object) = &**init else {
            panic!("expected object literal replacement")
        };
        assert_eq!(object.props.len(), 3);
    }

    #[test]
    #[should_panic(
        expected = "[nanocss] Exported css.defineVars(...) declarations must be in *.css.ts files."
    )]
    fn rejects_exported_define_vars_outside_css_source_files() {
        let mut module = parse_module(
            r#"
              import { css } from 'nanocss-compiler';
              export const colors = css.defineVars({
                primary: 'green'
              });
            "#,
        );
        let mut transform = NanoCssTransform::new(debug_options(), "src/app.tsx".to_string());
        module.visit_mut_with(&mut transform);
    }

    #[test]
    fn allows_exported_define_vars_in_css_source_files() {
        let mut module = parse_module(
            r#"
              import { css } from 'nanocss-compiler';
              export const colors = css.defineVars({
                primary: 'green'
              });
            "#,
        );
        let mut transform = NanoCssTransform::new(debug_options(), "src/tokens.css.ts".to_string());
        module.visit_mut_with(&mut transform);

        assert_eq!(transform.variable_defaults.len(), 1);
    }

    #[test]
    fn allows_exported_define_vars_in_css_mts_source_files() {
        let mut module = parse_module(
            r#"
              import { css } from 'nanocss-compiler';
              export const colors = css.defineVars({
                primary: 'green'
              });
            "#,
        );
        let mut transform =
            NanoCssTransform::new(debug_options(), "src/tokens.css.mts".to_string());
        module.visit_mut_with(&mut transform);

        assert_eq!(transform.variable_defaults.len(), 1);
    }

    #[test]
    fn compiles_hook_defaults_in_define_vars_calls() {
        let mut module = parse_module(
            r#"
              import { css } from 'nanocss-compiler';
              export const colors = css.defineVars({
                primary: {
                  default: 'green',
                  ':hover': 'red'
                }
              });
            "#,
        );
        let mut transform = NanoCssTransform::new(debug_options(), "src/tokens.css.ts".to_string());
        module.visit_mut_with(&mut transform);

        assert_eq!(transform.variable_defaults.len(), 1);
        assert_eq!(
            transform.variable_defaults[0].value,
            "var(--_hover-mbscpo-1, red) var(--_hover-mbscpo-0, green)"
        );
        assert!(transform.style_sheet().contains("*:hover"));
    }

    #[test]
    #[should_panic(
        expected = "[nanocss] Exported css.createTheme(...) declarations must be in *.css.ts files."
    )]
    fn rejects_exported_create_theme_outside_css_source_files() {
        let mut module = parse_module(
            r#"
              import { css } from 'nanocss-compiler';
              const colors = css.defineVars({
                primary: 'green'
              });
              export const theme = css.createTheme(colors, {
                primary: 'red'
              });
            "#,
        );
        let mut transform = NanoCssTransform::new(debug_options(), "src/app.tsx".to_string());
        module.visit_mut_with(&mut transform);
    }

    #[test]
    fn allows_exported_create_theme_in_css_cts_source_files() {
        let mut module = parse_module(
            r#"
              import { css } from 'nanocss-compiler';
              const colors = css.defineVars({
                primary: 'green'
              });
              export const theme = css.createTheme(colors, {
                primary: 'red'
              });
            "#,
        );
        let mut transform =
            NanoCssTransform::new(debug_options(), "src/tokens.css.cts".to_string());
        module.visit_mut_with(&mut transform);

        let output = emit_module(&module);
        assert!(output.contains("export const theme = {"));
    }

    #[test]
    fn compiles_local_variable_tokens_in_create_styles() {
        let mut module = parse_module(
            r#"
              import { css } from 'nanocss-compiler';
              const colors = css.defineVars({
                primary: 'green'
              });
              const styles = css.create({
                root: {
                  color: colors.primary,
                  [colors.primary]: 'red'
                }
              });
              const keep = styles;
            "#,
        );
        let mut transform = NanoCssTransform::new(debug_options(), "src/tokens.css.ts".to_string());
        module.visit_mut_with(&mut transform);

        let var_decl = get_var_decl(&module, 1);
        let init = var_decl.decls[0].init.as_ref().expect("expected init");
        let Expr::Object(styles) = &**init else {
            panic!("expected styles object")
        };
        let Some(PropOrSpread::Prop(root)) = styles.props.first() else {
            panic!("expected root style")
        };
        let Prop::KeyValue(root) = &**root else {
            panic!("expected root property")
        };
        let Expr::Object(root_style) = &*root.value else {
            panic!("expected root style object")
        };
        let Some(PropOrSpread::Prop(color)) = root_style.props.first() else {
            panic!("expected color property")
        };
        let Prop::KeyValue(color) = &**color else {
            panic!("expected color key value")
        };
        let Expr::Lit(Lit::Str(color_value)) = &*color.value else {
            panic!("expected color string")
        };
        assert_eq!(
            color_value.value.as_str(),
            Some(
                "var(--_nanocss_var_colors_primary_vec0x7, var(--_nanocss_var_colors_primary_vec0x7--n-default))"
            )
        );

        let Some(PropOrSpread::Prop(theme_value)) = root_style.props.get(1) else {
            panic!("expected custom property override")
        };
        let Prop::KeyValue(theme_value) = &**theme_value else {
            panic!("expected custom property key value")
        };
        assert!(matches!(
            &theme_value.key,
            PropName::Str(name) if name.value.as_str() == Some("--_nanocss_var_colors_primary_vec0x7")
        ));
    }

    #[test]
    fn compiles_imported_variable_tokens_in_create_styles() {
        let mut module = parse_module(
            r#"
              import { css } from 'nanocss-compiler';
              import { colors } from './tokens.css';
              const styles = css.create({
                root: {
                  color: colors.primary
                }
              });
              const keep = styles;
            "#,
        );
        let mut transform = NanoCssTransform::new(debug_options(), "src/app.tsx".to_string());
        module.visit_mut_with(&mut transform);

        let var_decl = get_var_decl(&module, 0);
        let init = var_decl.decls[0].init.as_ref().expect("expected init");
        let Expr::Object(styles) = &**init else {
            panic!("expected styles object")
        };
        let Some(PropOrSpread::Prop(root)) = styles.props.first() else {
            panic!("expected root style")
        };
        let Prop::KeyValue(root) = &**root else {
            panic!("expected root property")
        };
        let Expr::Object(root_style) = &*root.value else {
            panic!("expected root style object")
        };
        let Some(PropOrSpread::Prop(color)) = root_style.props.first() else {
            panic!("expected color property")
        };
        let Prop::KeyValue(color) = &**color else {
            panic!("expected color key value")
        };
        assert!(matches!(&*color.value, Expr::Bin(_)));
    }

    #[test]
    fn does_not_compile_shadowed_local_variable_tokens() {
        let mut module = parse_module(
            r#"
              import { css } from 'nanocss-compiler';
              const colors = css.defineVars({
                primary: 'green'
              });
              const styles = css.create({
                root: colors => ({
                  color: colors.primary
                })
              });
              const props = css.props(styles.root(runtimeColors));
            "#,
        );
        let mut transform = NanoCssTransform::new(debug_options(), "src/tokens.css.ts".to_string());
        module.visit_mut_with(&mut transform);

        let output = emit_module(&module);
        assert!(output.contains("color: colors.primary"));
        assert!(!output.contains("color: \"var(--_nanocss_var_"));
    }

    #[test]
    fn does_not_compile_shadowed_imported_variable_tokens() {
        let mut module = parse_module(
            r#"
              import { css } from 'nanocss-compiler';
              import { colors } from './tokens.css';
              const styles = css.create({
                root: colors => ({
                  color: colors.primary
                })
              });
              const props = css.props(styles.root(runtimeColors));
            "#,
        );
        let mut transform = NanoCssTransform::new(debug_options(), "src/app.tsx".to_string());
        module.visit_mut_with(&mut transform);

        let output = emit_module(&module);
        assert!(output.contains("color: colors.primary"));
        assert!(!output.contains("\"var(\" + colors.primary"));
    }

    #[test]
    fn compiles_local_variable_tokens_inside_hook_values() {
        let mut module = parse_module(
            r#"
              import { css } from 'nanocss-compiler';
              const colors = css.defineVars({
                primary: 'green',
                accent: 'red'
              });
              const styles = css.create({
                root: {
                  color: {
                    default: colors.primary,
                    ':hover': colors.accent
                  }
                }
              });
              const keep = styles;
            "#,
        );
        let mut transform = NanoCssTransform::new(debug_options(), "src/tokens.css.ts".to_string());
        module.visit_mut_with(&mut transform);

        let var_decl = get_var_decl(&module, 1);
        let init = var_decl.decls[0].init.as_ref().expect("expected init");
        let Expr::Object(styles) = &**init else {
            panic!("expected styles object")
        };
        let Some(PropOrSpread::Prop(root)) = styles.props.first() else {
            panic!("expected root style")
        };
        let Prop::KeyValue(root) = &**root else {
            panic!("expected root property")
        };
        let Expr::Object(root_style) = &*root.value else {
            panic!("expected root style object")
        };
        let Some(PropOrSpread::Prop(color)) = root_style.props.first() else {
            panic!("expected color property")
        };
        let Prop::KeyValue(color) = &**color else {
            panic!("expected color key value")
        };
        let Expr::Lit(Lit::Str(color_value)) = &*color.value else {
            panic!("expected color string")
        };
        let color_value = color_value.value.as_str().expect("expected string");
        assert!(color_value.contains("var(--_nanocss_var_"));
        assert!(color_value.contains("--n-default"));
        assert!(!color_value.contains("--_nanocss_dynamic_"));
    }

    #[test]
    fn compiles_imported_variable_tokens_inside_hook_values() {
        let mut module = parse_module(
            r#"
              import { css } from 'nanocss-compiler';
              import { colors } from './tokens.css';
              const styles = css.create({
                root: {
                  color: {
                    default: colors.primary,
                    ':hover': colors.accent
                  }
                }
              });
              const keep = styles;
            "#,
        );
        let mut transform = NanoCssTransform::new(debug_options(), "src/app.tsx".to_string());
        module.visit_mut_with(&mut transform);

        let var_decl = get_var_decl(&module, 0);
        let init = var_decl.decls[0].init.as_ref().expect("expected init");
        let Expr::Object(styles) = &**init else {
            panic!("expected styles object")
        };
        let Some(PropOrSpread::Prop(root)) = styles.props.first() else {
            panic!("expected root style")
        };
        let Prop::KeyValue(root) = &**root else {
            panic!("expected root property")
        };
        let Expr::Object(root_style) = &*root.value else {
            panic!("expected root style object")
        };
        assert_eq!(root_style.props.len(), 3);
        let Some(PropOrSpread::Prop(dynamic_token)) = root_style.props.first() else {
            panic!("expected dynamic token property")
        };
        let Prop::KeyValue(dynamic_token) = &**dynamic_token else {
            panic!("expected dynamic token key value")
        };
        assert!(
            matches!(&dynamic_token.key, PropName::Str(name) if name.value.starts_with("--_nanocss_dynamic_"))
        );
        assert!(matches!(&*dynamic_token.value, Expr::Bin(_)));
    }

    #[test]
    fn compiles_static_hook_values_in_create_styles() {
        let mut module = parse_module(
            r#"
              import { css } from 'nanocss-compiler';
              const styles = css.create({
                root: {
                  color: {
                    default: 'black',
                    ':hover': 'red'
                  }
                }
              });
              const keep = styles;
            "#,
        );
        let mut transform = NanoCssTransform::new(debug_options(), "src/app.tsx".to_string());
        module.visit_mut_with(&mut transform);

        let var_decl = get_var_decl(&module, 0);
        let init = var_decl.decls[0].init.as_ref().expect("expected init");
        let Expr::Object(styles) = &**init else {
            panic!("expected styles object")
        };
        let Some(PropOrSpread::Prop(root)) = styles.props.first() else {
            panic!("expected root style")
        };
        let Prop::KeyValue(root) = &**root else {
            panic!("expected root property")
        };
        let Expr::Object(root_style) = &*root.value else {
            panic!("expected root style object")
        };
        let Some(PropOrSpread::Prop(color)) = root_style.props.first() else {
            panic!("expected color property")
        };
        let Prop::KeyValue(color) = &**color else {
            panic!("expected color key value")
        };
        let Expr::Lit(Lit::Str(color_value)) = &*color.value else {
            panic!("expected color string")
        };
        assert_eq!(
            color_value.value.as_str(),
            Some("var(--_hover-mbscpo-1, red) var(--_hover-mbscpo-0, black)")
        );
        assert!(transform.style_sheet().contains("*:hover"));
    }

    #[test]
    fn compiles_first_that_works_values_in_create_styles() {
        let mut module = parse_module(
            r#"
              import { css } from 'nanocss-compiler';
              const styles = css.create({
                root: {
                  position: css.firstThatWorks('sticky', '-webkit-sticky', 'fixed')
                }
              });
              const keep = styles;
            "#,
        );
        let mut transform = NanoCssTransform::new(debug_options(), "src/app.tsx".to_string());
        module.visit_mut_with(&mut transform);

        let output = emit_module(&module);
        assert!(output.contains("position: \"var(--_supports__position__sticky_"));
        let style_sheet = transform.style_sheet();
        assert!(style_sheet.contains("@supports (position: -webkit-sticky)"));
        assert!(style_sheet.contains("@supports (position: sticky)"));
    }

    #[test]
    fn compiles_dynamic_hook_values_in_create_styles() {
        let mut module = parse_module(
            r#"
              import { css } from 'nanocss-compiler';
              const styles = css.create({
                root: (width, opacity) => ({
                  marginLeft: {
                    default: 0,
                    ':hover': width
                  },
                  opacity: {
                    default: 0,
                    ':hover': opacity
                  }
                })
              });
              const keep = styles;
            "#,
        );
        let mut transform = NanoCssTransform::new(debug_options(), "src/app.tsx".to_string());
        module.visit_mut_with(&mut transform);

        let var_decl = get_var_decl(&module, 0);
        let init = var_decl.decls[0].init.as_ref().expect("expected init");
        let Expr::Object(styles) = &**init else {
            panic!("expected styles object")
        };
        let Some(PropOrSpread::Prop(root)) = styles.props.first() else {
            panic!("expected root style")
        };
        let Prop::KeyValue(root) = &**root else {
            panic!("expected root key value")
        };
        let Expr::Arrow(root_fn) = &*root.value else {
            panic!("expected dynamic style function")
        };
        let swc_core::ecma::ast::BlockStmtOrExpr::Expr(body) = &*root_fn.body else {
            panic!("expected expression body")
        };
        let style = match &**body {
            Expr::Object(style) => style,
            Expr::Paren(paren) => {
                let Expr::Object(style) = &*paren.expr else {
                    panic!("expected style object")
                };
                style
            }
            _ => panic!("expected style object"),
        };

        assert_eq!(style.props.len(), 4);
        let Some(PropOrSpread::Prop(dynamic_width)) = style.props.first() else {
            panic!("expected dynamic width property")
        };
        let Prop::KeyValue(dynamic_width) = &**dynamic_width else {
            panic!("expected dynamic width key value")
        };
        assert!(
            matches!(&dynamic_width.key, PropName::Str(name) if name.value.starts_with("--_nanocss_dynamic_"))
        );
        assert!(matches!(&*dynamic_width.value, Expr::Cond(_)));

        let Some(PropOrSpread::Prop(margin_left)) = style.props.get(1) else {
            panic!("expected marginLeft property")
        };
        let Prop::KeyValue(margin_left) = &**margin_left else {
            panic!("expected marginLeft key value")
        };
        let Expr::Lit(Lit::Str(margin_left_value)) = &*margin_left.value else {
            panic!("expected marginLeft string")
        };
        let margin_left_value = margin_left_value
            .value
            .as_str()
            .expect("expected string value");
        assert!(margin_left_value.contains("var(--_nanocss_dynamic_"));
        assert!(margin_left_value.contains("0px"));
    }

    #[test]
    fn compiles_hook_values_in_create_theme_calls() {
        let mut module = parse_module(
            r#"
              import { css } from 'nanocss-compiler';
              const colors = css.defineVars({
                primary: 'green'
              });
              const theme = css.createTheme(colors, {
                primary: {
                  default: 'red',
                  ':hover': 'blue'
                }
              });
            "#,
        );
        let mut transform = NanoCssTransform::new(debug_options(), "src/tokens.css.ts".to_string());
        module.visit_mut_with(&mut transform);

        let var_decl = get_var_decl(&module, 1);
        let init = var_decl.decls[0].init.as_ref().expect("expected init");
        let Expr::Object(theme) = &**init else {
            panic!("expected theme object")
        };
        assert!(matches!(theme.props.first(), Some(PropOrSpread::Spread(_))));
        let Some(PropOrSpread::Prop(primary)) = theme.props.get(1) else {
            panic!("expected primary property")
        };
        let Prop::KeyValue(primary) = &**primary else {
            panic!("expected primary key value")
        };
        let Expr::Lit(Lit::Str(value)) = &*primary.value else {
            panic!("expected compiled hook string")
        };
        assert_eq!(
            value.value.as_str(),
            Some("var(--_hover-mbscpo-1, blue) var(--_hover-mbscpo-0, red)")
        );
        assert!(transform.style_sheet().contains("*:hover"));
    }

    #[test]
    fn replaces_imported_create_theme_calls_with_computed_keys() {
        let mut module = parse_module(
            r#"
              import { css } from 'nanocss-compiler';
              import { colors } from './tokens.css';
              const theme = css.createTheme(colors, {
                primary: 'purple',
                '--brand': 'red'
              });
            "#,
        );
        let mut transform = NanoCssTransform::new(debug_options(), "src/app.tsx".to_string());
        module.visit_mut_with(&mut transform);

        let var_decl = get_var_decl(&module, 0);
        let init = var_decl.decls[0].init.as_ref().expect("expected init");
        let Expr::Object(object) = &**init else {
            panic!("expected object literal replacement")
        };
        assert_eq!(object.props.len(), 3);
    }

    #[test]
    fn compiles_hook_objects_for_computed_imported_variable_token_keys() {
        let mut module = parse_module(
            r#"
              import { css } from 'nanocss-compiler';
              import { colors } from './tokens.css';
              const styles = css.create({
                root: {
                  [colors.primary]: {
                    default: 'red',
                    ':hover': 'blue'
                  }
                }
              });
              const rootProps = css.props(styles.root);
            "#,
        );
        let mut transform = NanoCssTransform::new(debug_options(), "src/app.tsx".to_string());
        module.visit_mut_with(&mut transform);

        let output = emit_module(&module);
        assert!(output.contains(
            "[colors.primary]: \"var(--_hover-mbscpo-1, blue) var(--_hover-mbscpo-0, red)\""
        ));
        assert!(!output.contains("default: 'red'"));
        assert!(transform.style_sheet().contains("*:hover"));
    }

    #[test]
    fn replaces_null_create_theme_overrides_with_unshadowable_undefined() {
        let mut module = parse_module(
            r#"
              import { css } from 'nanocss-compiler';
              import { colors } from './tokens.css';
              const undefined = 'shadowed';
              const theme = css.createTheme(colors, {
                danger: null
              });
            "#,
        );
        let mut transform = NanoCssTransform::new(debug_options(), "src/app.tsx".to_string());
        module.visit_mut_with(&mut transform);

        let output = emit_module(&module);
        assert!(output.contains("[colors.danger]: void 0"));
        assert!(!output.contains("[colors.danger]: undefined"));
    }

    #[test]
    #[should_panic(
        expected = "[nanocss] css.createTheme(...) must be assigned to a variable declaration."
    )]
    fn rejects_unassigned_create_theme_calls() {
        let mut module = parse_module(
            r#"
              import { css } from 'nanocss-compiler';
              import { colors } from './tokens.css';
              css.createTheme(colors, {
                primary: 'purple'
              });
            "#,
        );
        let mut transform = NanoCssTransform::new(debug_options(), "src/app.tsx".to_string());
        module.visit_mut_with(&mut transform);
    }

    #[test]
    fn replaces_static_create_calls() {
        let mut module = parse_module(
            r#"
              import { css } from 'nanocss-compiler';
              const styles = css.create({
                root: {
                  opacity: 1,
                  color: 'red'
                }
              });
              const keep = styles;
            "#,
        );
        let mut transform = NanoCssTransform::new(debug_options(), "src/app.tsx".to_string());
        module.visit_mut_with(&mut transform);

        let var_decl = get_var_decl(&module, 0);
        let init = var_decl.decls[0].init.as_ref().expect("expected init");
        let Expr::Object(object) = &**init else {
            panic!("expected object literal replacement")
        };
        assert_eq!(object.props.len(), 1);
    }

    #[test]
    #[should_panic(
        expected = "[nanocss] css.create(...) must be assigned to a variable declaration."
    )]
    fn rejects_unassigned_create_calls() {
        let mut module = parse_module(
            r#"
              import { css } from 'nanocss-compiler';
              css.create({
                root: { opacity: 1 }
              });
            "#,
        );
        let mut transform = NanoCssTransform::new(debug_options(), "src/app.tsx".to_string());
        module.visit_mut_with(&mut transform);
    }

    #[test]
    #[should_panic(
        expected = "[nanocss] css.createTheme(...) must be called with exactly two arguments."
    )]
    fn rejects_create_theme_calls_without_arguments_with_nanocss_error() {
        let mut module = parse_module(
            r#"
              import { css } from 'nanocss-compiler';
              const theme = css.createTheme();
            "#,
        );
        let mut transform = NanoCssTransform::new(debug_options(), "src/app.tsx".to_string());
        module.visit_mut_with(&mut transform);
    }

    #[test]
    #[should_panic(
        expected = "[nanocss] css.create(...) declarations must not share a variable declaration with other declarators."
    )]
    fn rejects_create_calls_in_multi_declarator_declarations() {
        let mut module = parse_module(
            r#"
              import { css } from 'nanocss-compiler';
              const styles = css.create({
                root: { opacity: 1 }
              }), other = 1;
            "#,
        );
        let mut transform = NanoCssTransform::new(debug_options(), "src/app.tsx".to_string());
        module.visit_mut_with(&mut transform);
    }

    #[test]
    #[should_panic(expected = "[nanocss] css.create(...) must define at least one style.")]
    fn rejects_empty_create_calls() {
        let mut module = parse_module(
            r#"
              import { css } from 'nanocss-compiler';
              const styles = css.create({});
            "#,
        );
        let mut transform = NanoCssTransform::new(debug_options(), "src/app.tsx".to_string());
        module.visit_mut_with(&mut transform);
    }

    #[test]
    #[should_panic(expected = "CSS shorthand property \"margin\" is not supported")]
    fn rejects_shorthand_style_properties() {
        let mut module = parse_module(
            r#"
              import { css } from 'nanocss-compiler';
              const styles = css.create({
                root: { margin: 0 }
              });
            "#,
        );
        let mut transform = NanoCssTransform::new(debug_options(), "src/app.tsx".to_string());
        module.visit_mut_with(&mut transform);
    }

    #[test]
    fn replaces_props_calls_with_style_props_object() {
        let mut module = parse_module(
            r#"
              import { css } from 'nanocss-compiler';
              const styles = css.create({
                root: { opacity: 1 },
                active: { color: 'red' }
              });
              const rootProps = css.props(styles.root, styles.active);
            "#,
        );
        let mut transform = NanoCssTransform::new(debug_options(), "src/app.tsx".to_string());
        module.visit_mut_with(&mut transform);

        let var_decl = get_var_decl(&module, 1);
        let init = var_decl.decls[0].init.as_ref().expect("expected init");
        let Expr::Object(object) = &**init else {
            panic!("expected props object replacement")
        };
        assert_eq!(object.props.len(), 1);
    }

    #[test]
    fn zero_arg_props_calls_return_empty_style_props_object() {
        let mut module = parse_module(
            r#"
              import { css } from 'nanocss-compiler';
              const rootProps = css.props();
            "#,
        );
        let mut transform = NanoCssTransform::new(debug_options(), "src/app.tsx".to_string());
        module.visit_mut_with(&mut transform);

        let var_decl = get_var_decl(&module, 0);
        let init = var_decl.decls[0].init.as_ref().expect("expected init");
        let Expr::Object(object) = &**init else {
            panic!("expected props object replacement")
        };
        let Some(Expr::Object(style)) = get_style_prop_value(object) else {
            panic!("expected empty style object")
        };
        assert!(style.props.is_empty());
    }

    #[test]
    fn inlines_static_style_members_and_removes_local_create_declarations() {
        let mut module = parse_module(
            r#"
              import { css } from 'nanocss-compiler';
              const styles = css.create({
                root: { opacity: 1 }
              });
              const rootProps = css.props(styles.root);
            "#,
        );
        let mut transform = NanoCssTransform::new(debug_options(), "src/app.tsx".to_string());
        module.visit_mut_with(&mut transform);

        assert_eq!(module.body.len(), 2);
        let var_decl = get_var_decl(&module, 1);
        let Some(binding) = var_decl.decls[0].name.as_ident() else {
            panic!("expected identifier binding")
        };
        assert_eq!(binding.id.sym.as_str(), "rootProps");
        let init = var_decl.decls[0].init.as_ref().expect("expected init");
        let Expr::Object(object) = &**init else {
            panic!("expected props object replacement")
        };
        assert!(matches!(get_style_prop_value(object), Some(Expr::Ident(_))));
    }

    #[test]
    fn merged_static_styles_preserve_last_duplicate_property_order() {
        let mut module = parse_module(
            r#"
              import { css } from 'nanocss-compiler';
              const styles = css.create({
                base: { opacity: 1, color: 'red' },
                override: { opacity: 0.5 }
              });
              const rootProps = css.props(styles.base, styles.override);
            "#,
        );
        let mut transform = NanoCssTransform::new(debug_options(), "src/app.tsx".to_string());
        module.visit_mut_with(&mut transform);

        let output = emit_module(&module);
        let color_index = output.find("color").expect("expected color property");
        let opacity_index = output.find("opacity").expect("expected opacity property");
        assert!(
            color_index < opacity_index,
            "last duplicate property should move to the end"
        );
        assert!(!output.contains("opacity: 1"));
    }

    #[test]
    fn merged_static_styles_preserve_overwritten_property_side_effects() {
        let mut module = parse_module(
            r#"
              import { css } from 'nanocss-compiler';
              const styles = css.create({
                base: { color: before(), opacity: 1 },
                override: { color: 'blue', opacity: 0.5 }
              });
              const rootProps = css.props(styles.base, styles.override);
            "#,
        );
        let mut transform = NanoCssTransform::new(debug_options(), "src/app.tsx".to_string());
        module.visit_mut_with(&mut transform);

        let output = emit_module(&module);
        assert!(
            output.contains("color: before()"),
            "overwritten effectful value should still evaluate"
        );
        assert!(output.contains("color: 'blue'"));
        assert!(!output.contains("opacity: 1"));
    }

    #[test]
    fn reuses_single_props_style_expression_without_spread_allocation() {
        let mut module = parse_module(
            r#"
              import { css } from 'nanocss-compiler';
              const rootProps = css.props(theme);
            "#,
        );
        let mut transform = NanoCssTransform::new(debug_options(), "src/app.tsx".to_string());
        module.visit_mut_with(&mut transform);

        let var_decl = get_var_decl(&module, 0);
        let init = var_decl.decls[0].init.as_ref().expect("expected init");
        let Expr::Object(object) = &**init else {
            panic!("expected props object replacement")
        };
        let Some(PropOrSpread::Prop(prop)) = object.props.first() else {
            panic!("expected style prop")
        };
        let Prop::KeyValue(prop) = &**prop else {
            panic!("expected style key value")
        };
        assert!(matches!(&*prop.value, Expr::Ident(name) if name.sym.as_str() == "theme"));
    }

    #[test]
    fn replaces_jsx_props_spreads_with_style_attributes() {
        let mut module = parse_module(
            r#"
              import { css } from 'nanocss-compiler';
              const styles = css.create({
                root: { opacity: 1 },
                active: { color: 'red' }
              });
              const element = <div id="x" {...css.props(styles.root, styles.active)} />;
            "#,
        );
        let mut transform = NanoCssTransform::new(debug_options(), "src/app.tsx".to_string());
        module.visit_mut_with(&mut transform);

        let var_decl = get_var_decl(&module, 1);
        let init = var_decl.decls[0].init.as_ref().expect("expected init");
        let Expr::JSXElement(element) = &**init else {
            panic!("expected jsx element")
        };
        assert!(matches!(
            element.opening.attrs[1],
            JSXAttrOrSpread::JSXAttr(JSXAttr {
                name: JSXAttrName::Ident(..),
                ..
            })
        ));
    }

    #[test]
    #[should_panic(
        expected = "[nanocss] Compiled style objects cannot be referenced directly. Pass styles only to css.props(...)."
    )]
    fn rejects_direct_local_style_references_in_jsx_style_attributes() {
        let mut module = parse_module(
            r#"
              import { css } from 'nanocss-compiler';
              const styles = css.create({
                root: { opacity: 1 }
              });
              const element = <div style={styles.root} />;
            "#,
        );
        let mut transform = NanoCssTransform::new(debug_options(), "src/app.tsx".to_string());
        module.visit_mut_with(&mut transform);
    }

    #[test]
    #[should_panic(
        expected = "[nanocss] Compiled style objects cannot be referenced directly. Pass styles only to css.props(...)."
    )]
    fn rejects_direct_local_style_references_nested_in_jsx_style_callbacks() {
        let mut module = parse_module(
            r#"
              import { css } from 'nanocss-compiler';
              const styles = css.create({
                root: { opacity: 1 }
              });
              const element = <div style={{
                get value() {
                  if (enabled) {
                    return styles.root;
                  }
                  return null;
                }
              }} />;
            "#,
        );
        let mut transform = NanoCssTransform::new(debug_options(), "src/app.tsx".to_string());
        module.visit_mut_with(&mut transform);
    }

    #[test]
    #[should_panic(
        expected = "[nanocss] Compiled style objects cannot be referenced directly. Pass styles only to css.props(...)."
    )]
    fn rejects_direct_local_style_references_nested_in_jsx_style_functions() {
        let mut module = parse_module(
            r#"
              import { css } from 'nanocss-compiler';
              const styles = css.create({
                root: { opacity: 1 }
              });
              const element = <div style={{
                value: () => {
                  for (const item of items) {
                    if (item.active) {
                      return styles.root;
                    }
                  }
                }
              }} />;
            "#,
        );
        let mut transform = NanoCssTransform::new(debug_options(), "src/app.tsx".to_string());
        module.visit_mut_with(&mut transform);
    }

    #[test]
    fn allows_shadowed_direct_style_references_nested_in_jsx_style_functions() {
        let mut module = parse_module(
            r#"
              import { css } from 'nanocss-compiler';
              const styles = css.create({
                root: { opacity: 1 }
              });
              const element = <div style={{
                value: () => {
                  const styles = runtimeStyles;
                  return styles.root;
                }
              }} />;
            "#,
        );
        let mut transform = NanoCssTransform::new(debug_options(), "src/app.tsx".to_string());
        module.visit_mut_with(&mut transform);

        let var_decl = get_var_decl(&module, 1);
        let init = var_decl.decls[0].init.as_ref().expect("expected init");
        let Expr::JSXElement(element) = &**init else {
            panic!("expected jsx element")
        };
        assert!(is_jsx_style_attribute(&element.opening.attrs[0]));
    }

    #[test]
    fn allows_shadowed_direct_style_references_in_jsx_style_attributes() {
        let mut module = parse_module(
            r#"
              import { css } from 'nanocss-compiler';
              const styles = css.create({
                root: { opacity: 1 }
              });
              function Comp({ styles }) {
                return <div style={styles.root} />;
              }
            "#,
        );
        let mut transform = NanoCssTransform::new(debug_options(), "src/app.tsx".to_string());
        module.visit_mut_with(&mut transform);

        let function = get_fn_decl(&module, 0);
        let return_statement = &function
            .function
            .body
            .as_ref()
            .expect("expected function body")
            .stmts[0];
        let Stmt::Return(return_statement) = return_statement else {
            panic!("expected return statement")
        };
        let Expr::JSXElement(element) = &**return_statement
            .arg
            .as_ref()
            .expect("expected return argument")
        else {
            panic!("expected jsx return")
        };
        assert!(is_jsx_style_attribute(&element.opening.attrs[0]));
    }

    #[test]
    fn does_not_transform_catch_parameter_shadowed_css() {
        let mut module = parse_module(
            r#"
              import { css } from 'nanocss-compiler';
              const styles = css.create({
                root: { opacity: 1 }
              });
              function Comp() {
                try {
                  run();
                } catch (css) {
                  return css.props(styles.root);
                }
              }
            "#,
        );
        let mut transform = NanoCssTransform::new(debug_options(), "src/app.tsx".to_string());
        module.visit_mut_with(&mut transform);

        let output = emit_module(&module);
        assert!(output.contains("return css.props(styles.root)"));
    }

    #[test]
    fn does_not_transform_for_binding_shadowed_css() {
        let mut module = parse_module(
            r#"
              import { css } from 'nanocss-compiler';
              const styles = css.create({
                root: { opacity: 1 }
              });
              function Comp(items) {
                for (const css of items) {
                  css.props(styles.root);
                }
              }
            "#,
        );
        let mut transform = NanoCssTransform::new(debug_options(), "src/app.tsx".to_string());
        module.visit_mut_with(&mut transform);

        let output = emit_module(&module);
        assert!(output.contains("css.props(styles.root)"));
    }

    #[test]
    fn does_not_resolve_for_binding_shadowed_style_groups() {
        let mut module = parse_module(
            r#"
              import { css } from 'nanocss-compiler';
              const styles = css.create({
                root: { opacity: 1 }
              });
              function Comp(items) {
                for (const styles of items) {
                  const props = css.props(styles.root);
                }
              }
            "#,
        );
        let mut transform = NanoCssTransform::new(debug_options(), "src/app.tsx".to_string());
        module.visit_mut_with(&mut transform);

        let output = emit_module(&module);
        assert!(output.contains("style: styles.root"));
        assert!(!output.contains("style: _styles"));
    }

    #[test]
    fn does_not_transform_nested_var_hoisted_shadowed_css() {
        let mut module = parse_module(
            r#"
              import { css } from 'nanocss-compiler';
              const styles = css.create({
                root: { opacity: 1 }
              });
              function Comp() {
                if (enabled) {
                  var css = other;
                }
                return css.props(styles.root);
              }
            "#,
        );
        let mut transform = NanoCssTransform::new(debug_options(), "src/app.tsx".to_string());
        module.visit_mut_with(&mut transform);

        let output = emit_module(&module);
        assert!(output.contains("return css.props(styles.root)"));
    }

    #[test]
    fn allows_local_style_references_in_html_style_attributes() {
        let mut module = parse_module(
            r#"
              import { css, html } from 'nanocss-compiler';
              const styles = css.create({
                root: { opacity: 1 }
              });
              const element = <html.div style={styles.root} />;
            "#,
        );
        let mut transform = NanoCssTransform::new(debug_options(), "src/app.tsx".to_string());
        module.visit_mut_with(&mut transform);

        let var_decl = get_var_decl(&module, 1);
        let init = var_decl.decls[0].init.as_ref().expect("expected init");
        let Expr::JSXElement(element) = &**init else {
            panic!("expected jsx element")
        };
        let Some(JSXAttrOrSpread::JSXAttr(style_attr)) = element.opening.attrs.last() else {
            panic!("expected style attribute")
        };
        let Some(JSXAttrValue::JSXExprContainer(container)) = &style_attr.value else {
            panic!("expected style expression")
        };
        let JSXExpr::Expr(expression) = &container.expr else {
            panic!("expected style expression")
        };
        let Expr::Ident(style) = &**expression else {
            panic!("expected direct style identifier")
        };
        assert_eq!(style.sym.as_ref(), "_stylesRoot");
    }

    #[test]
    fn collapses_duplicate_jsx_style_attributes_after_props_spreads() {
        let mut module = parse_module(
            r#"
              import { css } from 'nanocss-compiler';
              const styles = css.create({
                a: { opacity: 1 },
                b: { opacity: 0.5 }
              });
              const element = <div {...css.props(styles.a)} {...css.props(styles.b)} />;
            "#,
        );
        let mut transform = NanoCssTransform::new(debug_options(), "src/app.tsx".to_string());
        module.visit_mut_with(&mut transform);

        let var_decl = get_var_decl(&module, 2);
        let init = var_decl.decls[0].init.as_ref().expect("expected init");
        let Expr::JSXElement(element) = &**init else {
            panic!("expected jsx element")
        };
        assert_eq!(
            element
                .opening
                .attrs
                .iter()
                .filter(|attribute| is_jsx_style_attribute(attribute))
                .count(),
            1
        );
    }

    #[test]
    fn preserves_jsx_spread_order_around_props_styles() {
        let mut module = parse_module(
            r#"
              import { css } from 'nanocss-compiler';
              const styles = css.create({
                root: { opacity: 1 }
              });
              const element = <div {...css.props(styles.root)} {...extraProps} />;
            "#,
        );
        let mut transform = NanoCssTransform::new(debug_options(), "src/app.tsx".to_string());
        module.visit_mut_with(&mut transform);

        let var_decl = get_var_decl(&module, 1);
        let init = var_decl.decls[0].init.as_ref().expect("expected init");
        let Expr::JSXElement(element) = &**init else {
            panic!("expected jsx element")
        };
        assert!(is_jsx_style_attribute(&element.opening.attrs[0]));
        assert!(matches!(
            element.opening.attrs[1],
            JSXAttrOrSpread::SpreadElement(_)
        ));
    }

    #[test]
    fn flattens_props_array_composition() {
        let mut module = parse_module(
            r#"
              import { css } from 'nanocss-compiler';
              const styles = css.create({
                root: { opacity: 1 },
                active: { color: 'red' }
              });
              const rootProps = css.props([styles.root, [false, styles.active], undefined]);
            "#,
        );
        let mut transform = NanoCssTransform::new(debug_options(), "src/app.tsx".to_string());
        module.visit_mut_with(&mut transform);

        let var_decl = get_var_decl(&module, 1);
        let init = var_decl.decls[0].init.as_ref().expect("expected init");
        let Expr::Object(object) = &**init else {
            panic!("expected props object replacement")
        };
        assert!(matches!(get_style_prop_value(object), Some(Expr::Ident(_))));
    }

    #[test]
    fn inlines_conditional_static_style_members() {
        let mut module = parse_module(
            r#"
              import { css } from 'nanocss-compiler';
              const styles = css.create({
                active: { opacity: 1 },
                inactive: { opacity: 0.5 }
              });
              const rootProps = css.props(isActive ? styles.active : styles.inactive);
            "#,
        );
        let mut transform = NanoCssTransform::new(debug_options(), "src/app.tsx".to_string());
        module.visit_mut_with(&mut transform);

        assert_eq!(module.body.len(), 3);
        let var_decl = get_var_decl(&module, 2);
        let init = var_decl.decls[0].init.as_ref().expect("expected init");
        let Expr::Object(object) = &**init else {
            panic!("expected props object replacement")
        };
        let Some(Expr::Cond(conditional)) = get_style_prop_value(object) else {
            panic!("expected conditional style expression")
        };
        assert!(matches!(&*conditional.cons, Expr::Ident(_)));
        assert!(matches!(&*conditional.alt, Expr::Ident(_)));
    }

    #[test]
    fn inlines_logical_static_style_members() {
        let mut module = parse_module(
            r#"
              import { css } from 'nanocss-compiler';
              const styles = css.create({
                root: { display: 'flex' },
                disabled: { pointerEvents: 'none' }
              });
              const rootProps = css.props(styles.root, isDisabled && styles.disabled);
            "#,
        );
        let mut transform = NanoCssTransform::new(debug_options(), "src/app.tsx".to_string());
        module.visit_mut_with(&mut transform);

        assert_eq!(module.body.len(), 3);
        let var_decl = get_var_decl(&module, 2);
        let init = var_decl.decls[0].init.as_ref().expect("expected init");
        let Expr::Object(object) = &**init else {
            panic!("expected props object replacement")
        };
        let JSXOrProp::Style(style) = get_style_prop(object) else {
            panic!("expected style prop")
        };
        let Some(PropOrSpread::Spread(spread)) = style.props.get(1) else {
            panic!("expected logical style spread")
        };
        let Expr::Bin(binary) = &*spread.expr else {
            panic!("expected logical style expression")
        };
        assert!(matches!(binary.op, BinaryOp::LogicalAnd));
        assert!(matches!(&*binary.right, Expr::Ident(_)));
    }

    #[test]
    fn emits_dynamic_style_helpers_and_removes_local_create_declarations() {
        let mut module = parse_module(
            r#"
              import { css } from 'nanocss-compiler';
              const styles = css.create({
                root: { opacity: 1 },
                width: value => ({ width: value })
              });
              const rootProps = css.props(styles.root, styles.width(10));
            "#,
        );
        let mut transform = NanoCssTransform::new(debug_options(), "src/app.tsx".to_string());
        module.visit_mut_with(&mut transform);

        assert_eq!(module.body.len(), 3);
        let helper_decl = get_var_decl(&module, 1);
        assert_eq!(
            helper_decl.decls[0].name.as_ident().unwrap().id.sym,
            "_stylesWidth"
        );
        assert!(matches!(
            helper_decl.decls[0].init.as_deref(),
            Some(Expr::Arrow(_))
        ));

        let var_decl = get_var_decl(&module, 2);
        let init = var_decl.decls[0].init.as_ref().expect("expected init");
        let Expr::Object(object) = &**init else {
            panic!("expected props object replacement")
        };
        let JSXOrProp::Style(style) = get_style_prop(object) else {
            panic!("expected style prop")
        };
        assert_eq!(style.props.len(), 2);
        let Some(PropOrSpread::Spread(dynamic_style)) = style.props.get(1) else {
            panic!("expected dynamic style spread")
        };
        let Expr::Call(dynamic_call) = &*dynamic_style.expr else {
            panic!("expected dynamic style call")
        };
        let Callee::Expr(callee) = &dynamic_call.callee else {
            panic!("expected expression callee")
        };
        assert!(matches!(&**callee, Expr::Ident(id) if id.sym == "_stylesWidth"));
    }

    #[test]
    #[should_panic(
        expected = "[nanocss] css.create(...) dynamic style function bodies must be object literals."
    )]
    fn rejects_dynamic_style_block_bodies() {
        let mut module = parse_module(
            r#"
              import { css } from 'nanocss-compiler';
              const styles = css.create({
                root: value => {
                  return { width: value };
                }
              });
            "#,
        );
        let mut transform = NanoCssTransform::new(debug_options(), "src/app.tsx".to_string());
        module.visit_mut_with(&mut transform);
    }

    #[test]
    #[should_panic(
        expected = "[nanocss] css.create(...) dynamic style function parameters must be simple identifiers."
    )]
    fn rejects_dynamic_style_destructured_parameters() {
        let mut module = parse_module(
            r#"
              import { css } from 'nanocss-compiler';
              const styles = css.create({
                root: ({ value }) => ({ width: value })
              });
            "#,
        );
        let mut transform = NanoCssTransform::new(debug_options(), "src/app.tsx".to_string());
        module.visit_mut_with(&mut transform);
    }

    #[test]
    #[should_panic(
        expected = "[nanocss] css.create(...) dynamic style function parameters must be simple identifiers."
    )]
    fn rejects_dynamic_style_default_parameters() {
        let mut module = parse_module(
            r#"
              import { css } from 'nanocss-compiler';
              const styles = css.create({
                root: (value = 1) => ({ width: value })
              });
            "#,
        );
        let mut transform = NanoCssTransform::new(debug_options(), "src/app.tsx".to_string());
        module.visit_mut_with(&mut transform);
    }

    #[test]
    #[should_panic(
        expected = "[nanocss] css.create(...) dynamic style function parameters must be simple identifiers."
    )]
    fn rejects_dynamic_style_rest_parameters() {
        let mut module = parse_module(
            r#"
              import { css } from 'nanocss-compiler';
              const styles = css.create({
                root: (...values) => ({ width: values[0] })
              });
            "#,
        );
        let mut transform = NanoCssTransform::new(debug_options(), "src/app.tsx".to_string());
        module.visit_mut_with(&mut transform);
    }

    #[test]
    #[should_panic(
        expected = "[nanocss] css.create(...) values must be style object expressions or arrow functions returning style object expressions."
    )]
    fn rejects_dynamic_style_function_expressions() {
        let mut module = parse_module(
            r#"
              import { css } from 'nanocss-compiler';
              const styles = css.create({
                root: function(value) {
                  return { width: value };
                }
              });
            "#,
        );
        let mut transform = NanoCssTransform::new(debug_options(), "src/app.tsx".to_string());
        module.visit_mut_with(&mut transform);
    }

    #[test]
    #[should_panic(expected = "[nanocss] css.create(...) declarations must be at the top level.")]
    fn rejects_nested_create_declarations() {
        let mut module = parse_module(
            r#"
              import { css } from 'nanocss-compiler';
              function makeStyles() {
                const styles = css.create({
                  root: { color: 'red' }
                });
                return styles;
              }
            "#,
        );
        let mut transform = NanoCssTransform::new(debug_options(), "src/app.tsx".to_string());
        module.visit_mut_with(&mut transform);
    }

    #[test]
    #[should_panic(
        expected = "[nanocss] css.defineVars(...) declarations must be at the top level."
    )]
    fn rejects_nested_define_vars_declarations() {
        let mut module = parse_module(
            r#"
              import { css } from 'nanocss-compiler';
              {
                const colors = css.defineVars({
                  primary: 'red'
                });
              }
            "#,
        );
        let mut transform = NanoCssTransform::new(debug_options(), "src/app.tsx".to_string());
        module.visit_mut_with(&mut transform);
    }

    #[test]
    #[should_panic(
        expected = "[nanocss] css.keyframes(...) declarations must be at the top level."
    )]
    fn rejects_nested_keyframes_declarations() {
        let mut module = parse_module(
            r#"
              import { css } from 'nanocss-compiler';
              function makeAnimation() {
                return css.keyframes({
                  from: { opacity: 0 },
                  to: { opacity: 1 }
                });
              }
            "#,
        );
        let mut transform = NanoCssTransform::new(debug_options(), "src/app.tsx".to_string());
        module.visit_mut_with(&mut transform);
    }

    #[test]
    fn exposes_collected_style_sheet() {
        let mut module = parse_module(
            r#"
              import { css } from 'nanocss-compiler';
              const colors = css.defineVars({
                primary: 'green'
              });
              const fadeIn = css.keyframes({
                from: { opacity: 0 },
                to: { opacity: 1 }
              });
            "#,
        );
        let mut transform = NanoCssTransform::new(debug_options(), "src/tokens.css.ts".to_string());
        module.visit_mut_with(&mut transform);

        assert_eq!(
            transform.style_sheet(),
            "@keyframes __nanocss_keyframes-firn26 {\n  from {\n    opacity: 0;\n  }\n  to {\n    opacity: 1;\n  }\n}\n* {\n  --_nanocss_var_colors_primary_vec0x7--n-default: green;\n}"
        );
    }
}

#[cfg(test)]
enum JSXOrProp<'a> {
    Style(&'a swc_core::ecma::ast::ObjectLit),
    Other,
}

#[cfg(test)]
fn get_style_prop(object: &swc_core::ecma::ast::ObjectLit) -> JSXOrProp<'_> {
    let Some(Expr::Object(style)) = get_style_prop_value(object) else {
        return JSXOrProp::Other;
    };
    JSXOrProp::Style(style)
}

#[cfg(test)]
fn get_style_prop_value(object: &swc_core::ecma::ast::ObjectLit) -> Option<&Expr> {
    let Some(swc_core::ecma::ast::PropOrSpread::Prop(prop)) = object.props.first() else {
        return None;
    };
    let swc_core::ecma::ast::Prop::KeyValue(prop) = &**prop else {
        return None;
    };
    Some(&prop.value)
}
