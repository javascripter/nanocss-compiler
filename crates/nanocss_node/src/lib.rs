use std::{
    panic::{self, AssertUnwindSafe},
    path::Path,
    sync::{Mutex, OnceLock},
};

use nanocss_swc::{TransformOptions, transform_program_with_source_map};
use napi::{Error, Result, Status};
use napi_derive::napi;
use swc_core::{
    common::{FileName, SourceMap, sync::Lrc},
    ecma::{
        ast::Program,
        codegen::{Emitter, text_writer::JsWriter},
        parser::{EsSyntax, Parser, StringInput, Syntax, TsSyntax},
    },
};

#[napi(object)]
pub struct NativeTransformOptions {
    pub filename: String,
    pub code: Option<bool>,
    pub debug: Option<bool>,
    pub import_sources: Option<Vec<String>>,
    pub input_source_map: Option<String>,
    pub html_defaults: Option<String>,
    pub env: Option<String>,
}

#[napi(object)]
pub struct NativeTransformResult {
    pub code: Option<String>,
    pub metadata: NativeTransformMetadata,
}

#[napi(object)]
pub struct NativeTransformMetadata {
    pub nanocss: NativeNanoCssMetadata,
}

#[napi(object)]
pub struct NativeNanoCssMetadata {
    pub style_sheet: String,
}

#[napi]
pub fn transform_sync(
    source: String,
    options: NativeTransformOptions,
) -> Result<NativeTransformResult> {
    catch_compiler_panic(|| {
        let source_map: Lrc<SourceMap> = Default::default();
        let program = parse_program(&source_map, &source, &options.filename)?;
        let mut transform_options = TransformOptions {
            debug: options.debug.unwrap_or(false),
            input_source_map: options
                .input_source_map
                .as_deref()
                .and_then(|source_map| serde_json::from_str(source_map).ok()),
            html_defaults: options
                .html_defaults
                .as_deref()
                .map(serde_json::from_str)
                .transpose()
                .map_err(|error| {
                    Error::new(
                        Status::GenericFailure,
                        format!("[nanocss] Invalid htmlDefaults option: {error}."),
                    )
                })?
                .unwrap_or_default(),
            env: options
                .env
                .as_deref()
                .map(serde_json::from_str)
                .transpose()
                .map_err(|error| {
                    Error::new(
                        Status::GenericFailure,
                        format!("[nanocss] Invalid env option: {error}."),
                    )
                })?
                .unwrap_or_else(|| serde_json::Value::Object(Default::default())),
            ..Default::default()
        };

        if let Some(import_sources) = options.import_sources {
            transform_options.import_sources = import_sources;
        }

        let result = transform_program_with_source_map(
            program,
            transform_options,
            options.filename,
            &*source_map,
        );

        let code = if options.code.unwrap_or(true) {
            Some(print_program(&source_map, &result.program)?)
        } else {
            None
        };

        Ok(NativeTransformResult {
            code,
            metadata: NativeTransformMetadata {
                nanocss: NativeNanoCssMetadata {
                    style_sheet: result.style_sheet,
                },
            },
        })
    })
    .unwrap_or_else(|payload| Err(Error::new(Status::GenericFailure, panic_message(payload))))
}

fn catch_compiler_panic<F, T>(callback: F) -> std::thread::Result<T>
where
    F: FnOnce() -> T,
{
    static PANIC_HOOK_LOCK: OnceLock<Mutex<()>> = OnceLock::new();

    let lock = PANIC_HOOK_LOCK.get_or_init(|| Mutex::new(()));
    let _guard = lock.lock().unwrap_or_else(|error| error.into_inner());
    let previous_hook = panic::take_hook();
    panic::set_hook(Box::new(|_| {}));
    let result = panic::catch_unwind(AssertUnwindSafe(callback));
    panic::set_hook(previous_hook);
    result
}

fn parse_program(source_map: &Lrc<SourceMap>, source: &str, filename: &str) -> Result<Program> {
    let file =
        source_map.new_source_file(FileName::Real(filename.into()).into(), source.to_string());
    let input = StringInput::from(&*file);
    let mut parser = Parser::new(syntax_for_filename(filename), input, None);

    parser.parse_module().map(Program::Module).map_err(|error| {
        Error::new(
            Status::GenericFailure,
            format!("[nanocss] Failed to parse \"{filename}\": {error:?}"),
        )
    })
}

fn print_program(source_map: &Lrc<SourceMap>, program: &Program) -> Result<String> {
    let mut output = Vec::new();
    {
        let writer = JsWriter::new(source_map.clone(), "\n", &mut output, None);
        let mut emitter = Emitter {
            cfg: Default::default(),
            comments: None,
            cm: source_map.clone(),
            wr: writer,
        };
        emitter.emit_program(program).map_err(|error| {
            Error::new(
                Status::GenericFailure,
                format!("[nanocss] Failed to print transformed code: {error}"),
            )
        })?;
    }

    String::from_utf8(output).map_err(|error| {
        Error::new(
            Status::GenericFailure,
            format!("[nanocss] Printed code was not valid UTF-8: {error}"),
        )
    })
}

fn panic_message(payload: Box<dyn std::any::Any + Send>) -> String {
    if let Some(message) = payload.downcast_ref::<String>() {
        return message.clone();
    }
    if let Some(message) = payload.downcast_ref::<&str>() {
        return (*message).to_string();
    }
    "unknown panic".to_string()
}

fn syntax_for_filename(filename: &str) -> Syntax {
    let extension = Path::new(filename)
        .extension()
        .and_then(|extension| extension.to_str())
        .unwrap_or_default();

    match extension {
        "ts" | "tsx" | "mts" | "cts" => Syntax::Typescript(TsSyntax {
            tsx: extension == "tsx",
            decorators: true,
            ..Default::default()
        }),
        _ => Syntax::Es(EsSyntax {
            jsx: true,
            ..Default::default()
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn transforms_source_and_returns_stylesheet_metadata() {
        let result = transform_sync(
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
              export function App() {
                return <div {...css.props(styles.root)} />;
              }
            "#
            .to_string(),
            NativeTransformOptions {
                filename: "src/app.tsx".to_string(),
                code: None,
                debug: Some(true),
                import_sources: None,
                input_source_map: None,
                html_defaults: None,
                env: None,
            },
        )
        .expect("source should transform");

        assert!(result.code.unwrap().contains("style={_stylesRoot}"));
        assert!(result.metadata.nanocss.style_sheet.contains("*:hover"));
        assert!(
            result
                .metadata
                .nanocss
                .style_sheet
                .contains("--_hover-mbscpo-0")
        );
    }

    #[test]
    fn parses_js_files_with_jsx() {
        let result = transform_sync(
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
              export function App() {
                return <div {...css.props(styles.root)} />;
              }
            "#
            .to_string(),
            NativeTransformOptions {
                filename: "src/app.js".to_string(),
                code: None,
                debug: Some(true),
                import_sources: None,
                input_source_map: None,
                html_defaults: None,
                env: None,
            },
        )
        .expect("source should transform");

        assert!(result.code.unwrap().contains("style={_stylesRoot}"));
        assert!(result.metadata.nanocss.style_sheet.contains("*:hover"));
    }

    #[test]
    fn skips_code_generation_when_code_is_false() {
        let result = transform_sync(
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
              export function App() {
                return <div {...css.props(styles.root)} />;
              }
            "#
            .to_string(),
            NativeTransformOptions {
                filename: "src/app.tsx".to_string(),
                code: Some(false),
                debug: Some(true),
                import_sources: None,
                input_source_map: None,
                html_defaults: None,
                env: None,
            },
        )
        .expect("source should transform");

        assert!(result.code.is_none());
        assert!(result.metadata.nanocss.style_sheet.contains("*:hover"));
    }
}
