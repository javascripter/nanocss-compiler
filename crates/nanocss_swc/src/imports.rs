use std::collections::HashSet;

use swc_core::ecma::{
    ast::{Ident, ImportSpecifier, Module, ModuleDecl, ModuleExportName, ModuleItem},
    visit::{Visit, VisitWith},
};

pub(crate) fn get_named_import(specifier: &ImportSpecifier) -> Option<(String, String)> {
    let ImportSpecifier::Named(named) = specifier else {
        return None;
    };
    let imported = named
        .imported
        .as_ref()
        .map(|name| match name {
            ModuleExportName::Ident(ident) => ident.sym.to_string(),
            ModuleExportName::Str(str_) => str_.value.as_str().unwrap_or("").to_string(),
            #[cfg(swc_ast_unknown)]
            _ => panic!("[nanocss] Unknown SWC module export name."),
        })
        .unwrap_or_else(|| named.local.sym.to_string());
    Some((imported, named.local.sym.to_string()))
}

pub(crate) fn remove_unused_nanocss_imports(module: &mut Module, import_sources: &[String]) {
    let mut removable_names = HashSet::new();
    for item in &module.body {
        let ModuleItem::ModuleDecl(ModuleDecl::Import(import_decl)) = item else {
            continue;
        };
        let Some(source) = import_decl.src.value.as_str() else {
            continue;
        };
        if !import_sources
            .iter()
            .any(|import_source| import_source == source)
        {
            continue;
        }

        for specifier in &import_decl.specifiers {
            let Some((imported, local)) = get_named_import(specifier) else {
                continue;
            };
            if imported == "css" || imported == "html" {
                removable_names.insert(local);
            }
        }
    }

    if removable_names.is_empty() {
        return;
    }

    let mut collector = IdentifierCollector {
        targets: &removable_names,
        used: HashSet::new(),
    };
    for item in &module.body {
        if matches!(item, ModuleItem::ModuleDecl(ModuleDecl::Import(_))) {
            continue;
        }
        item.visit_with(&mut collector);
    }

    module.body.retain_mut(|item| {
        let ModuleItem::ModuleDecl(ModuleDecl::Import(import_decl)) = item else {
            return true;
        };
        let Some(source) = import_decl.src.value.as_str() else {
            return true;
        };
        if !import_sources
            .iter()
            .any(|import_source| import_source == source)
        {
            return true;
        }

        import_decl.specifiers.retain(|specifier| {
            let Some((imported, local)) = get_named_import(specifier) else {
                return true;
            };
            (imported != "css" && imported != "html") || collector.used.contains(&local)
        });

        !import_decl.specifiers.is_empty()
    });
}

struct IdentifierCollector<'a> {
    targets: &'a HashSet<String>,
    used: HashSet<String>,
}

// This collector is intentionally scope-blind. It may keep a nanocss import
// alive when a same-named local binding exists, which is conservative but
// avoids removing imports that could still be referenced.
impl Visit for IdentifierCollector<'_> {
    fn visit_ident(&mut self, ident: &Ident) {
        let name = ident.sym.to_string();
        if self.targets.contains(&name) {
            self.used.insert(name);
        }
    }
}
