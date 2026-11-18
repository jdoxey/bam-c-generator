use std::collections::HashSet;
use std::ops::Range;
use std::path::PathBuf;

use tree_sitter::{Node, Parser};

use crate::imports::{LoadedModule, ProjectSources};
use crate::source::{SourceError, SourceLocation};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ObjectDefinition {
    pub source_path: PathBuf,
    pub fields: Vec<Field>,
    pub methods: Vec<Method>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Field {
    pub name: String,
    /// Original declaration, including its semicolon.
    pub declaration: String,
    pub location: SourceLocation,
    pub span: Range<usize>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Method {
    pub name: String,
    pub return_type: String,
    pub parameters: Vec<Parameter>,
    /// Byte range of the original compound statement, including its braces.
    pub body_span: Range<usize>,
    pub location: SourceLocation,
    pub span: Range<usize>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Parameter {
    pub name: String,
    /// Original C parameter declaration, excluding separating commas.
    pub declaration: String,
    pub location: SourceLocation,
    pub span: Range<usize>,
}

pub fn parse_root(project: &ProjectSources) -> Result<ObjectDefinition, SourceError> {
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
    parse_module(module)
}

pub fn parse_module(module: &LoadedModule) -> Result<ObjectDefinition, SourceError> {
    let mut parser = Parser::new();
    parser
        .set_language(&tree_sitter_c::LANGUAGE.into())
        .map_err(|error| {
            SourceError::new(
                &module.source.path,
                None,
                format!("cannot initialize the C parser: {error}"),
            )
        })?;

    let tree = parser.parse(&module.c_text, None).ok_or_else(|| {
        SourceError::new(
            &module.source.path,
            None,
            "C parser did not return a syntax tree".to_string(),
        )
    })?;
    let root = tree.root_node();

    if root.has_error() {
        let error_node = first_parse_error(root).unwrap_or(root);
        let location = location_at(&module.c_text, error_node.start_byte());
        let message = if error_node.is_missing() {
            format!("missing C syntax token near '{}'", error_node.kind())
        } else {
            format!(
                "invalid or unsupported C syntax near '{}'",
                error_node.kind()
            )
        };
        return Err(SourceError::new(
            &module.source.path,
            Some(location),
            message,
        ));
    }

    if root.kind() != "translation_unit" {
        return Err(SourceError::new(
            &module.source.path,
            Some(location_at(&module.c_text, root.start_byte())),
            format!("expected a C translation unit, found '{}'", root.kind()),
        ));
    }

    let mut fields = Vec::new();
    let mut methods = Vec::new();
    let mut field_names = HashSet::new();
    let mut method_names = HashSet::new();

    for index in 0..root.named_child_count() {
        let node = root.named_child(index).expect("named child index is valid");
        match node.kind() {
            "comment" => {}
            "declaration" => {
                let field = parse_field(node, &module.c_text, &module.source.path)?;
                if !field_names.insert(field.name.clone()) {
                    return Err(SourceError::new(
                        &module.source.path,
                        Some(field.location),
                        format!("duplicate field '{}'", field.name),
                    ));
                }
                fields.push(field);
            }
            "function_definition" => {
                let method = parse_method(node, &module.c_text, &module.source.path)?;
                if method.name == "new" || method.name == "delete" {
                    return Err(SourceError::new(
                        &module.source.path,
                        Some(method.location),
                        format!(
                            "method '{}' conflicts with a generated lifecycle function",
                            method.name
                        ),
                    ));
                }
                if !method_names.insert(method.name.clone()) {
                    let message = if method.name == "init" || method.name == "deinit" {
                        format!("duplicate lifecycle method '{}'", method.name)
                    } else {
                        format!("duplicate method '{}'", method.name)
                    };
                    return Err(SourceError::new(
                        &module.source.path,
                        Some(method.location),
                        message,
                    ));
                }
                methods.push(method);
            }
            other => {
                return Err(SourceError::new(
                    &module.source.path,
                    Some(location_at(&module.c_text, node.start_byte())),
                    format!("unsupported top-level C construct '{}'", other),
                ));
            }
        }
    }

    Ok(ObjectDefinition {
        source_path: module.source.path.clone(),
        fields,
        methods,
    })
}

fn parse_field(node: Node<'_>, source: &str, path: &std::path::Path) -> Result<Field, SourceError> {
    let type_node = node
        .child_by_field_name("type")
        .ok_or_else(|| error_at_node(path, source, node, "field declaration is missing a type"))?;
    ensure_simple_type(type_node, node, source, path)?;

    let declarators: Vec<_> = named_children(node)
        .into_iter()
        .filter(|child| is_declarator_kind(child.kind()))
        .collect();
    if declarators.len() != 1 {
        return Err(error_at_node(
            path,
            source,
            node,
            "a field declaration must contain exactly one declarator",
        ));
    }

    let declarator = declarators[0];
    if declarator.kind() == "init_declarator" {
        return Err(error_at_node(
            path,
            source,
            declarator,
            "field initializers are not supported; initialize fields in init",
        ));
    }
    let name = declarator_name(declarator, source)
        .map_err(|message| error_at_node(path, source, declarator, &message))?;

    Ok(Field {
        name,
        declaration: node_text(source, node).to_string(),
        location: location_at(source, node.start_byte()),
        span: node.start_byte()..node.end_byte(),
    })
}

fn parse_method(
    node: Node<'_>,
    source: &str,
    path: &std::path::Path,
) -> Result<Method, SourceError> {
    let function_declarator = named_children(node)
        .into_iter()
        .find(|child| child.kind() == "function_declarator")
        .ok_or_else(|| {
            error_at_node(
                path,
                source,
                node,
                "function definition must use a named function declarator",
            )
        })?;
    let name_node = function_declarator
        .child_by_field_name("declarator")
        .ok_or_else(|| {
            error_at_node(
                path,
                source,
                function_declarator,
                "method is missing its name",
            )
        })?;
    let name = declarator_name(name_node, source)
        .map_err(|message| error_at_node(path, source, name_node, &message))?;

    let body = named_children(node)
        .into_iter()
        .find(|child| child.kind() == "compound_statement")
        .ok_or_else(|| error_at_node(path, source, node, "method is missing its body"))?;

    let return_type = text_between(source, node.start_byte(), function_declarator.start_byte())
        .trim()
        .to_string();
    let type_node = node.child_by_field_name("type").ok_or_else(|| {
        error_at_node(
            path,
            source,
            node,
            "method definition is missing a return type",
        )
    })?;
    ensure_simple_type(type_node, node, source, path)?;
    if return_type != node_text(source, type_node) {
        return Err(error_at_node(
            path,
            source,
            type_node,
            "storage classes, qualifiers, and compound return types are not supported",
        ));
    }

    let parameters = parse_parameters(function_declarator, source, path)?;
    let mut parameter_names = HashSet::new();
    for parameter in &parameters {
        if parameter.name == "self" {
            return Err(SourceError::new(
                path,
                Some(parameter.location),
                "method parameters cannot be named 'self'; bam adds that receiver".to_string(),
            ));
        }
        if !parameter_names.insert(parameter.name.clone()) {
            return Err(SourceError::new(
                path,
                Some(parameter.location),
                format!("duplicate parameter '{}'", parameter.name),
            ));
        }
    }

    if (name == "init" || name == "deinit") && return_type != "void" {
        return Err(error_at_node(
            path,
            source,
            type_node,
            format!("lifecycle method '{}' must return void", name),
        ));
    }

    Ok(Method {
        name,
        return_type,
        parameters,
        body_span: body.start_byte()..body.end_byte(),
        location: location_at(source, node.start_byte()),
        span: node.start_byte()..node.end_byte(),
    })
}

fn parse_parameters(
    function_declarator: Node<'_>,
    source: &str,
    path: &std::path::Path,
) -> Result<Vec<Parameter>, SourceError> {
    let parameter_list = function_declarator
        .child_by_field_name("parameters")
        .ok_or_else(|| {
            error_at_node(
                path,
                source,
                function_declarator,
                "method is missing a parameter list",
            )
        })?;
    let declarations: Vec<_> = named_children(parameter_list)
        .into_iter()
        .filter(|child| child.kind() == "parameter_declaration")
        .collect();

    if declarations.len() == 1
        && declarations[0].child_by_field_name("declarator").is_none()
        && declarations[0]
            .child_by_field_name("type")
            .is_some_and(|type_node| node_text(source, type_node) == "void")
    {
        return Ok(Vec::new());
    }

    let mut parameters = Vec::with_capacity(declarations.len());
    for declaration in declarations {
        let type_node = declaration.child_by_field_name("type").ok_or_else(|| {
            error_at_node(
                path,
                source,
                declaration,
                "parameter declaration is missing a type",
            )
        })?;
        ensure_simple_type(type_node, declaration, source, path)?;

        let declarator = declaration
            .child_by_field_name("declarator")
            .ok_or_else(|| {
                error_at_node(path, source, declaration, "parameters must have names")
            })?;
        let name = declarator_name(declarator, source)
            .map_err(|message| error_at_node(path, source, declarator, &message))?;
        let type_prefix =
            text_between(source, declaration.start_byte(), declarator.start_byte()).trim();
        if type_prefix != node_text(source, type_node) {
            return Err(error_at_node(
                path,
                source,
                declaration,
                "storage classes, qualifiers, and compound parameter types are not supported",
            ));
        }

        parameters.push(Parameter {
            name,
            declaration: node_text(source, declaration).to_string(),
            location: location_at(source, declaration.start_byte()),
            span: declaration.start_byte()..declaration.end_byte(),
        });
    }

    // A named child that is neither a parameter nor an empty-list marker is
    // typically a variadic or old-style parameter form, which this subset rejects.
    for child in named_children(parameter_list) {
        if child.kind() != "parameter_declaration" {
            return Err(error_at_node(
                path,
                source,
                child,
                "variadic and old-style parameter lists are not supported",
            ));
        }
    }

    Ok(parameters)
}

fn ensure_simple_type(
    type_node: Node<'_>,
    context: Node<'_>,
    source: &str,
    path: &std::path::Path,
) -> Result<(), SourceError> {
    if !matches!(type_node.kind(), "primitive_type" | "type_identifier") {
        return Err(error_at_node(
            path,
            source,
            context,
            format!("unsupported type syntax '{}'", type_node.kind()),
        ));
    }
    Ok(())
}

fn declarator_name(node: Node<'_>, source: &str) -> Result<String, String> {
    match node.kind() {
        "identifier" => Ok(node_text(source, node).to_string()),
        "pointer_declarator" => {
            let inner = node
                .child_by_field_name("declarator")
                .ok_or_else(|| "unsupported pointer declarator".to_string())?;
            if named_children(node).len() != 1 {
                return Err("qualified pointer declarators are not supported".to_string());
            }
            declarator_name(inner, source)
        }
        _ => Err(format!(
            "unsupported declarator '{}'; only identifiers and pointers are supported",
            node.kind()
        )),
    }
}

fn is_declarator_kind(kind: &str) -> bool {
    matches!(
        kind,
        "identifier"
            | "pointer_declarator"
            | "array_declarator"
            | "function_declarator"
            | "init_declarator"
            | "parenthesized_declarator"
            | "attributed_declarator"
    )
}

fn named_children(node: Node<'_>) -> Vec<Node<'_>> {
    (0..node.named_child_count())
        .filter_map(|index| node.named_child(index))
        .collect()
}

fn first_parse_error(node: Node<'_>) -> Option<Node<'_>> {
    if node.is_error() || node.is_missing() {
        return Some(node);
    }
    for index in 0..node.child_count() {
        if let Some(child) = node.child(index) {
            if let Some(error) = first_parse_error(child) {
                return Some(error);
            }
        }
    }
    None
}

fn error_at_node(
    path: &std::path::Path,
    source: &str,
    node: Node<'_>,
    message: impl Into<String>,
) -> SourceError {
    SourceError::new(
        path,
        Some(location_at(source, node.start_byte())),
        message.into(),
    )
}

fn location_at(source: &str, byte_offset: usize) -> SourceLocation {
    let byte_offset = byte_offset.min(source.len());
    let prefix = &source[..byte_offset];
    let line = prefix.bytes().filter(|&byte| byte == b'\n').count() + 1;
    let column_start = prefix.rfind('\n').map_or(0, |index| index + 1);
    let column = source[column_start..byte_offset].chars().count() + 1;
    SourceLocation { line, column }
}

fn node_text<'a>(source: &'a str, node: Node<'_>) -> &'a str {
    text_between(source, node.start_byte(), node.end_byte())
}

fn text_between(source: &str, start: usize, end: usize) -> &str {
    source.get(start..end).unwrap_or("")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::imports::load_project;
    use crate::source::SourceFile;

    fn parse_text(text: &str) -> Result<ObjectDefinition, SourceError> {
        let path = PathBuf::from("fixture.bam");
        parse_module(&LoadedModule {
            source: SourceFile {
                path: path.clone(),
                text: text.to_string(),
            },
            c_text: text.to_string(),
            imports: Vec::new(),
        })
    }

    #[test]
    fn parses_hexagon_fixture_and_preserves_method_body() {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let fixture = root.join("Documentation/examples/Hexagon/Hexagon.bam");
        let project = load_project(&fixture).expect("Hexagon fixture imports load");
        let object = parse_root(&project).expect("Hexagon fixture parses");

        assert_eq!(
            object
                .fields
                .iter()
                .map(|field| field.name.as_str())
                .collect::<Vec<_>>(),
            ["sideLength"]
        );
        assert_eq!(
            object
                .methods
                .iter()
                .map(|method| method.name.as_str())
                .collect::<Vec<_>>(),
            ["init", "area"]
        );
        let area = object
            .methods
            .iter()
            .find(|method| method.name == "area")
            .unwrap();
        let body = &project.modules.last().unwrap().c_text[area.body_span.clone()];
        assert!(body.contains("threeTimesSqrtOfThreeAllOverTwo"));
        assert!(body.starts_with('{') && body.ends_with('}'));
    }

    #[test]
    fn parses_farm_fixture_fields_and_methods() {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let fixture = root.join("Documentation/examples/HexagonFarm/HexagonFarm.bam");
        let project = load_project(&fixture).expect("Farm imports load");
        let object = parse_root(&project).expect("Farm fixture parses");

        assert_eq!(
            object
                .fields
                .iter()
                .map(|field| field.name.as_str())
                .collect::<Vec<_>>(),
            ["hexagonCount", "hexagons"]
        );
        assert_eq!(
            object
                .methods
                .iter()
                .map(|method| method.name.as_str())
                .collect::<Vec<_>>(),
            ["init", "deinit", "totalArea"]
        );
        assert_eq!(object.fields[1].declaration, "Hexagon **hexagons;");
    }

    #[test]
    fn rejects_malformed_c_with_location() {
        let error = parse_text("void run( {\n}").unwrap_err();
        assert!(error.location.is_some());
        assert!(error.message.contains("syntax"));
    }

    #[test]
    fn rejects_unsupported_declarators_and_duplicate_names() {
        let error = parse_text("int values[3];").unwrap_err();
        assert!(error.message.contains("unsupported declarator"));

        let error = parse_text("int value;\n double value;").unwrap_err();
        assert!(error.message.contains("duplicate field"));
    }

    #[test]
    fn rejects_duplicate_lifecycle_and_conflicting_generated_methods() {
        let error = parse_text("void init() {}\nvoid init(int n) {}").unwrap_err();
        assert!(error.message.contains("duplicate lifecycle method"));

        let error = parse_text("void delete() {}").unwrap_err();
        assert!(error
            .message
            .contains("conflicts with a generated lifecycle function"));
    }

    #[test]
    fn rejects_unrecognized_top_level_forms_and_duplicate_parameters() {
        let error = parse_text("typedef int Count;").unwrap_err();
        assert!(error.message.contains("unsupported top-level"));

        let error = parse_text("void run(int value, int value) {}").unwrap_err();
        assert!(error.message.contains("duplicate parameter"));
    }
}
