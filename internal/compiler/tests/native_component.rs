// Copyright © SixtyFPS GmbH <info@slint.dev>
// SPDX-License-Identifier: GPL-3.0-only OR LicenseRef-Slint-Royalty-free-2.0 OR LicenseRef-Slint-Software-3.0

use i_slint_compiler::diagnostics::BuildDiagnostics;
use i_slint_compiler::generator::OutputFormat;
use i_slint_compiler::parser::parse;
use i_slint_compiler::{CompilerConfiguration, compile_syntax_node};

const SOURCE: &str = r#"
native component Finder {
    in property <int> limit;
    out property <int> found;
    callback finished(count: int);
    pure function start(from: int) -> bool;
}
export component App inherits Window {
    Rectangle {
        f := Finder { limit: 3; }
        Text { text: f.found + (f.start(1) ? "a" : "b"); }
    }
}
"#;

fn compile(
    config: CompilerConfiguration,
) -> (BuildDiagnostics, i_slint_compiler::object_tree::Document) {
    let mut diagnostics = BuildDiagnostics::default();
    let syntax_node = parse(SOURCE.into(), None, &mut diagnostics);
    let (doc, diagnostics, _) =
        spin_on::spin_on(compile_syntax_node(syntax_node, diagnostics, config));
    (diagnostics, doc)
}

fn config(format: OutputFormat) -> CompilerConfiguration {
    let mut config = CompilerConfiguration::new(format);
    config.enable_experimental = true;
    config
}

const ERROR: &str = "'native component' is only supported when generating C++";

#[cfg(feature = "rust")]
#[test]
fn rust_output_is_rejected() {
    let (diagnostics, _) = compile(config(OutputFormat::Rust));
    assert_eq!(diagnostics.to_string_vec().iter().filter(|m| m.contains(ERROR)).count(), 1);
}

#[test]
fn unsupported_output_is_rejected() {
    let mut config = config(OutputFormat::Interpreter);
    config.native_components_supported = false;
    let (diagnostics, _) = compile(config);
    assert_eq!(diagnostics.to_string_vec().iter().filter(|m| m.contains(ERROR)).count(), 1);
}

#[test]
fn instance_is_lowered_to_members() {
    let compiler_config = config(OutputFormat::Interpreter);
    let (diagnostics, doc) = compile(compiler_config.clone());
    assert!(!diagnostics.has_errors(), "{:?}", diagnostics.to_string_vec());
    let unit =
        i_slint_compiler::llr::lower_to_item_tree::lower_to_item_tree(&doc, &compiler_config);

    let instances: Vec<_> =
        unit.sub_components.iter().flat_map(|sc| sc.native_components.iter()).collect();
    let [instance] = instances.as_slice() else { panic!("expected one instance") };
    assert_eq!(instance.class_name, "Finder");
    let names =
        |v: &[(smol_str::SmolStr, _)]| v.iter().map(|(n, _)| n.to_string()).collect::<Vec<_>>();
    assert_eq!(
        instance.properties.iter().map(|p| p.name.to_string()).collect::<Vec<_>>(),
        ["found", "limit"]
    );
    // Only `limit` is set by the instantiation, and only it is an `in` property
    assert_eq!(
        instance.properties.iter().map(|p| (p.name.to_string(), p.is_set)).collect::<Vec<_>>(),
        [("found".to_string(), false), ("limit".to_string(), true)]
    );
    assert_eq!(names(&instance.changed), ["limit"]);
    assert_eq!(names(&instance.callbacks), ["finished"]);
    assert_eq!(names(&instance.functions), ["start"]);

    let [class] = unit.native_component_classes.as_slice() else { panic!("expected one class") };
    assert_eq!(class.name, "Finder");
    assert_eq!(class.members.len(), 4);
}
