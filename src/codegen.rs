use std::fs;
use std::path::{Path, PathBuf};

use crate::imports::ProjectSources;
use crate::parser::{Method, ObjectDefinition};
use crate::source::SourceError;

#[derive(Debug)]
pub struct GeneratedPair {
    pub header_path: PathBuf,
    pub source_path: PathBuf,
    pub header: String,
    pub source: String,
}

pub fn generate(
    project: &ProjectSources,
    object: &ObjectDefinition,
) -> Result<GeneratedPair, SourceError> {
    let module = project
        .modules
        .iter()
        .find(|module| module.source.path == project.root)
        .ok_or_else(|| {
            SourceError::new(
                &project.root,
                None,
                "root source was not loaded".to_string(),
            )
        })?;
    if !module.imports.is_empty() {
        return Err(SourceError::new(
            &module.source.path,
            None,
            "code generation for imports is not implemented yet".to_string(),
        ));
    }

    let type_name = module
        .source
        .path
        .file_stem()
        .and_then(|stem| stem.to_str())
        .filter(|stem| is_c_identifier(stem))
        .ok_or_else(|| {
            SourceError::new(
                &module.source.path,
                None,
                "the .bam file name must be a valid C identifier".to_string(),
            )
        })?;
    if !object.methods.iter().any(|method| method.name == "init") {
        return Err(SourceError::new(
            &module.source.path,
            None,
            "generation currently requires an init method".to_string(),
        ));
    }

    let header_path = module.source.path.with_extension("h");
    let source_path = module.source.path.with_extension("c");
    let guard = format!("BAM_GENERATED_{}_H", type_name.to_ascii_uppercase());
    let header = render_header(type_name, &guard, object);
    let source = render_source(type_name, &header_path, &module.source.text, object);

    Ok(GeneratedPair {
        header_path,
        source_path,
        header,
        source,
    })
}

pub fn write(pair: &GeneratedPair) -> Result<(), SourceError> {
    write_one(&pair.header_path, &pair.header)?;
    write_one(&pair.source_path, &pair.source)
}

fn write_one(path: &Path, contents: &str) -> Result<(), SourceError> {
    fs::write(path, contents).map_err(|error| {
        SourceError::new(path, None, format!("cannot write generated file: {error}"))
    })
}

fn render_header(type_name: &str, guard: &str, object: &ObjectDefinition) -> String {
    let mut output = String::new();
    output.push_str(&format!("#ifndef {guard}\n#define {guard}\n\n"));
    output.push_str(&format!("typedef struct {type_name}_struct {{\n"));
    for field in &object.fields {
        output.push_str("    ");
        output.push_str(field.declaration.trim());
        output.push('\n');
    }
    output.push_str(&format!("}} {type_name};\n\n"));

    let init = object.methods.iter().find(|method| method.name == "init");
    let init_parameters = init.map_or(&[][..], |method| method.parameters.as_slice());
    output.push_str(&format!(
        "{type_name} *{type_name}_new({});\n",
        parameter_list(init_parameters)
    ));
    if let Some(init) = init {
        output.push_str(&format!("{};\n", method_prototype(type_name, init)));
    }
    output.push_str(&format!("void {type_name}_delete({type_name} *self);\n"));
    if let Some(deinit) = object.methods.iter().find(|method| method.name == "deinit") {
        output.push_str(&format!("{};\n", method_prototype(type_name, deinit)));
    }
    let custom_methods = object
        .methods
        .iter()
        .filter(|method| method.name != "init" && method.name != "deinit")
        .collect::<Vec<_>>();
    if !custom_methods.is_empty() {
        output.push('\n');
    }
    for method in custom_methods {
        output.push_str(&format!("{};\n", method_prototype(type_name, method)));
    }
    output.push_str(&format!("\n#endif  // {guard}\n"));
    output
}

fn render_source(
    type_name: &str,
    header_path: &Path,
    source_text: &str,
    object: &ObjectDefinition,
) -> String {
    let mut output = String::new();
    output.push_str("#include <stdlib.h> // malloc, free\n\n");
    output.push_str(&format!(
        "#include \"{}\"\n\n",
        header_path.file_name().unwrap().to_string_lossy()
    ));

    let init = object.methods.iter().find(|method| method.name == "init");
    let init_parameters = init.map_or(&[][..], |method| method.parameters.as_slice());
    let init_arguments = init_parameters
        .iter()
        .map(|parameter| parameter.name.as_str())
        .collect::<Vec<_>>()
        .join(", ");
    output.push_str(&format!(
        "{type_name} *{type_name}_new({}) {{\n",
        parameter_list(init_parameters)
    ));
    output.push_str(&format!(
        "    {type_name} *self = malloc(sizeof({type_name}));\n"
    ));
    output.push_str("    if (self == NULL) {\n        return NULL;\n    }\n");
    if init.is_some() {
        output.push_str(&format!(
            "    {type_name}_init(self{}{});\n",
            if init_arguments.is_empty() { "" } else { ", " },
            init_arguments
        ));
    }
    output.push_str("    return self;\n}\n\n");

    if let Some(init) = init {
        append_method(&mut output, type_name, source_text, init);
    }

    output.push_str(&format!("void {type_name}_delete({type_name} *self) {{\n"));
    if object.methods.iter().any(|method| method.name == "deinit") {
        output.push_str(&format!("    {type_name}_deinit(self);\n"));
    }
    output.push_str("    free(self);\n}\n\n");

    if let Some(deinit) = object.methods.iter().find(|method| method.name == "deinit") {
        append_method(&mut output, type_name, source_text, deinit);
    }
    for method in object
        .methods
        .iter()
        .filter(|method| method.name != "init" && method.name != "deinit")
    {
        append_method(&mut output, type_name, source_text, method);
    }
    if output.ends_with("\n\n") {
        output.pop();
    }
    output
}

fn method_prototype(type_name: &str, method: &Method) -> String {
    format!(
        "{} {}_{}({})",
        method.return_type,
        type_name,
        method.name,
        method_parameter_list(type_name, method)
    )
}

fn method_parameter_list(type_name: &str, method: &Method) -> String {
    let mut parameters = vec![format!("{type_name} *self")];
    parameters.extend(
        method
            .parameters
            .iter()
            .map(|parameter| parameter.declaration.clone()),
    );
    parameters.join(", ")
}

fn parameter_list(parameters: &[crate::parser::Parameter]) -> String {
    if parameters.is_empty() {
        "void".to_string()
    } else {
        parameters
            .iter()
            .map(|parameter| parameter.declaration.as_str())
            .collect::<Vec<_>>()
            .join(", ")
    }
}

fn append_method(output: &mut String, type_name: &str, source_text: &str, method: &Method) {
    output.push_str(&method_prototype(type_name, method));
    output.push(' ');
    output.push_str(&source_text[method.body_span.clone()]);
    output.push_str("\n\n");
}

fn is_c_identifier(value: &str) -> bool {
    let mut chars = value.chars();
    chars
        .next()
        .is_some_and(|first| first == '_' || first.is_ascii_alphabetic())
        && chars.all(|ch| ch == '_' || ch.is_ascii_alphanumeric())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::imports::load_project;
    use crate::parser::parse_root;

    #[test]
    fn hexagon_generation_matches_reference_files() {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let fixture = root.join("Documentation/examples/Hexagon/Hexagon.bam");
        let project = load_project(&fixture).expect("Hexagon imports load");
        let object = parse_root(&project).expect("Hexagon parses");
        let generated = generate(&project, &object).expect("Hexagon generates");

        assert_eq!(
            generated.header,
            include_str!("../Documentation/examples/Hexagon/Hexagon.h")
        );
        assert_eq!(
            generated.source,
            include_str!("../Documentation/examples/Hexagon/Hexagon.c")
        );
    }

    #[test]
    fn rejects_importing_objects_until_dependency_generation_is_implemented() {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let fixture = root.join("Documentation/examples/HexagonFarm/HexagonFarm.bam");
        let project = load_project(&fixture).expect("Farm imports load");
        let object = parse_root(&project).expect("Farm parses");
        let error = generate(&project, &object).unwrap_err();

        assert!(error
            .message
            .contains("code generation for imports is not implemented"));
    }
}
