mod ast;
mod constants;
mod declarations;
mod define_consts;
mod env;
mod generated_strings;
mod hash;
mod hooks;
mod html;
mod imports;
mod jsx;
mod keyframes;
mod keyframes_ast;
mod metadata;
mod options;
mod position_try;
mod props;
mod scope;
mod styles;
mod transform;
mod variables;
mod view_transition;

use std::path::{self, Component, Path, PathBuf};

use swc_core::{
    common::SourceMapper,
    ecma::{ast::Program, visit::VisitMutWith},
    plugin::{
        metadata::TransformPluginMetadataContextKind, plugin_transform,
        proxies::TransformPluginProgramMetadata,
    },
};

use transform::NanoCssTransform;

pub use options::TransformOptions;

pub struct TransformResult {
    pub program: Program,
    pub style_sheet: String,
}

pub fn transform_program(
    mut program: Program,
    options: TransformOptions,
    filename: String,
) -> TransformResult {
    let mut transform = NanoCssTransform::new(options, get_file_identity(&filename));
    program.visit_mut_with(&mut transform);
    let style_sheet = transform.style_sheet();

    TransformResult {
        program,
        style_sheet,
    }
}

pub fn transform_program_with_source_map(
    mut program: Program,
    options: TransformOptions,
    filename: String,
    source_map: &dyn SourceMapper,
) -> TransformResult {
    let mut transform = NanoCssTransform::new_with_source_map(
        options,
        get_file_identity(&filename),
        Some(source_map),
    );
    program.visit_mut_with(&mut transform);
    let style_sheet = transform.style_sheet();

    TransformResult {
        program,
        style_sheet,
    }
}

#[plugin_transform]
pub fn process_transform(program: Program, metadata: TransformPluginProgramMetadata) -> Program {
    let options = TransformOptions::from_json(metadata.get_transform_plugin_config());
    let filename = metadata
        .get_context(&TransformPluginMetadataContextKind::Filename)
        .unwrap_or_else(|| "unknown".to_string());
    let file_identity = get_file_identity(&filename);
    let mut program = program;
    let mut transform =
        NanoCssTransform::new_with_source_map(options, file_identity, Some(&metadata.source_map));
    program.visit_mut_with(&mut transform);
    // SWC plugin transforms can only return the JavaScript AST. CSS collection
    // is exposed through transform_program/transform_sync in the Node wrapper.
    program
}

fn get_file_identity(filename: &str) -> String {
    let file_path = normalize_path_components(Path::new(filename));
    let relative = if file_path.is_absolute() {
        std::env::current_dir()
            .ok()
            .map(|cwd| normalize_path_components(&cwd))
            .and_then(|cwd| file_path.strip_prefix(cwd).ok().map(Path::to_path_buf))
            .unwrap_or(file_path)
    } else {
        file_path
    };

    relative
        .to_string_lossy()
        .replace(path::MAIN_SEPARATOR, "/")
}

fn normalize_path_components(path: &Path) -> PathBuf {
    let mut normalized = PathBuf::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => match normalized.components().next_back() {
                Some(Component::Normal(_)) => {
                    normalized.pop();
                }
                Some(Component::RootDir | Component::Prefix(_)) => {}
                _ => normalized.push(component.as_os_str()),
            },
            _ => normalized.push(component.as_os_str()),
        }
    }
    normalized
}

#[cfg(test)]
mod tests {
    use super::get_file_identity;

    #[test]
    fn keeps_relative_file_identities() {
        assert_eq!(get_file_identity("src/app.tsx"), "src/app.tsx");
    }

    #[test]
    fn normalizes_relative_file_identity_parent_segments() {
        assert_eq!(get_file_identity("./src/../src/app.tsx"), "src/app.tsx");
    }

    #[test]
    fn preserves_unresolvable_leading_relative_parent_segments() {
        assert_eq!(get_file_identity("../src/../app.tsx"), "../app.tsx");
    }

    #[test]
    fn relativizes_absolute_file_identities() {
        let filename = std::env::current_dir().unwrap().join("src/app.tsx");
        assert_eq!(
            get_file_identity(&filename.to_string_lossy()),
            "src/app.tsx"
        );
    }

    #[test]
    fn normalizes_absolute_file_identity_parent_segments_before_relativizing() {
        let filename = std::env::current_dir()
            .unwrap()
            .join("src")
            .join("..")
            .join("src")
            .join("app.tsx");
        assert_eq!(
            get_file_identity(&filename.to_string_lossy()),
            "src/app.tsx"
        );
    }
}
