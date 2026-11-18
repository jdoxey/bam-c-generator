use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};

use crate::source::{canonicalize_input, load_source, SourceError, SourceFile, SourceLocation};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ImportKind {
    Standard {
        module: String,
        symbols: Vec<String>,
        c_header: String,
    },
    Object {
        source_path: PathBuf,
        header_include: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedImport {
    pub location: SourceLocation,
    pub kind: ImportKind,
}

#[derive(Debug, Clone)]
pub struct LoadedModule {
    pub source: SourceFile,
    pub c_text: String,
    pub imports: Vec<ResolvedImport>,
}

#[derive(Debug)]
pub struct ProjectSources {
    pub root: PathBuf,
    /// Dependencies precede their importers; the root module is last.
    pub modules: Vec<LoadedModule>,
}

#[derive(Debug, Clone)]
enum ImportSpec {
    Standard {
        module: String,
        symbols: Vec<String>,
    },
    Object {
        path: PathBuf,
    },
}

#[derive(Debug, Clone)]
struct ImportDirective {
    location: SourceLocation,
    spec: ImportSpec,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum LexState {
    Normal,
    BlockComment,
    LineComment,
    String,
    Character,
}

pub fn load_project(path: &Path) -> Result<ProjectSources, SourceError> {
    let root = canonicalize_input(path)?;
    let mut resolver = Resolver {
        active: Vec::new(),
        loaded: HashMap::new(),
    };
    resolver.visit(root.clone(), None)?;
    Ok(resolver.finish(root))
}

struct Resolver {
    active: Vec<PathBuf>,
    loaded: HashMap<PathBuf, LoadedModule>,
}

impl Resolver {
    fn visit(
        &mut self,
        path: PathBuf,
        import_location: Option<(PathBuf, SourceLocation)>,
    ) -> Result<(), SourceError> {
        if self.active.contains(&path) {
            let (source_path, location) =
                import_location.unwrap_or((path.clone(), SourceLocation { line: 1, column: 1 }));
            return Err(SourceError::new(
                source_path,
                Some(location),
                format!("object import cycle reaches '{}'", path.display()),
            ));
        }
        if self.loaded.contains_key(&path) {
            return Ok(());
        }

        self.active.push(path.clone());
        let source = load_source(&path)?;
        let (c_text, directives) = mask_and_parse_imports(&source)?;
        let mut imports = Vec::with_capacity(directives.len());
        let mut dependencies = Vec::new();

        for directive in directives {
            match directive.spec {
                ImportSpec::Standard { module, symbols } => {
                    let c_header = format!("{module}.h");
                    imports.push(ResolvedImport {
                        location: directive.location,
                        kind: ImportKind::Standard {
                            module,
                            symbols,
                            c_header,
                        },
                    });
                }
                ImportSpec::Object {
                    path: imported_path,
                } => {
                    let imported_source =
                        resolve_object_import(&source, &imported_path, directive.location)?;
                    let header_path = imported_path.with_extension("h");
                    let header_include = path_to_include(&header_path);
                    imports.push(ResolvedImport {
                        location: directive.location,
                        kind: ImportKind::Object {
                            source_path: imported_source.clone(),
                            header_include,
                        },
                    });
                    dependencies.push((imported_source, source.path.clone(), directive.location));
                }
            }
        }

        self.loaded.insert(
            path.clone(),
            LoadedModule {
                source,
                c_text,
                imports,
            },
        );

        for (dependency, importer, location) in dependencies {
            self.visit(dependency, Some((importer, location)))?;
        }

        self.active.pop();
        Ok(())
    }

    fn finish(self, root: PathBuf) -> ProjectSources {
        let modules: Vec<_> = self.loaded.into_values().collect();
        let mut visited = HashSet::new();
        let mut ordered = Vec::with_capacity(modules.len());
        visit_order(&root, &modules, &mut visited, &mut ordered);
        let by_path: HashMap<_, _> = modules
            .into_iter()
            .map(|module| (module.source.path.clone(), module))
            .collect();
        ProjectSources {
            root,
            modules: ordered
                .into_iter()
                .filter_map(|path| by_path.get(&path).cloned())
                .collect(),
        }
    }
}

fn visit_order(
    path: &Path,
    modules: &[LoadedModule],
    visited: &mut HashSet<PathBuf>,
    ordered: &mut Vec<PathBuf>,
) {
    let path = path.to_path_buf();
    if !visited.insert(path.clone()) {
        return;
    }
    if let Some(module) = modules.iter().find(|module| module.source.path == path) {
        for import in &module.imports {
            if let ImportKind::Object { source_path, .. } = &import.kind {
                visit_order(source_path, modules, visited, ordered);
            }
        }
        ordered.push(path);
    }
}

fn resolve_object_import(
    importer: &SourceFile,
    import_path: &Path,
    location: SourceLocation,
) -> Result<PathBuf, SourceError> {
    if import_path.is_absolute() {
        return Err(SourceError::new(
            &importer.path,
            Some(location),
            "object imports must use paths relative to the importing .bam file".to_string(),
        ));
    }

    let mut source_path = import_path.to_path_buf();
    match source_path
        .extension()
        .and_then(|extension| extension.to_str())
    {
        None => {
            source_path.set_extension("bam");
        }
        Some("bam") => {}
        Some(_) => {
            return Err(SourceError::new(
                &importer.path,
                Some(location),
                "object import path must have no extension or the .bam extension".to_string(),
            ));
        }
    }

    let candidate = importer
        .path
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .join(source_path);
    let canonical_path = fs::canonicalize(&candidate).map_err(|error| {
        SourceError::new(
            &importer.path,
            Some(location),
            format!(
                "cannot resolve object import '{}': {error}",
                candidate.display()
            ),
        )
    })?;

    if !canonical_path.is_file() {
        return Err(SourceError::new(
            &importer.path,
            Some(location),
            format!("object import '{}' is not a file", canonical_path.display()),
        ));
    }

    Ok(canonical_path)
}

fn path_to_include(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

fn mask_and_parse_imports(
    source: &SourceFile,
) -> Result<(String, Vec<ImportDirective>), SourceError> {
    let mut masked = source.text.as_bytes().to_vec();
    let mut directives = Vec::new();
    let mut line_start = 0;
    let mut line_number = 1;
    let mut brace_depth = 0isize;
    let mut state = LexState::Normal;
    let mut escaped = false;

    while line_start < masked.len() {
        let line_end = source.text[line_start..]
            .find('\n')
            .map_or(masked.len(), |offset| line_start + offset + 1);
        let line = &source.text[line_start..line_end];
        let content_end = line.trim_end_matches(['\r', '\n']).len();
        let content = &line[..content_end];

        if state == LexState::Normal && brace_depth == 0 {
            if let Some((column, import_text)) = import_candidate(content) {
                let location = SourceLocation {
                    line: line_number,
                    column,
                };
                let spec = parse_import(import_text)
                    .map_err(|message| SourceError::new(&source.path, Some(location), message))?;
                directives.push(ImportDirective { location, spec });
                for byte in &mut masked[line_start..line_start + content_end] {
                    *byte = b' ';
                }
                line_start = line_end;
                line_number += 1;
                continue;
            }
        }

        scan_c_line(line.as_bytes(), &mut state, &mut brace_depth, &mut escaped);
        line_start = line_end;
        line_number += 1;
    }

    let c_text = String::from_utf8(masked).expect("masking preserves valid UTF-8");
    Ok((c_text, directives))
}

fn import_candidate(line: &str) -> Option<(usize, &str)> {
    let trimmed = line.trim_start_matches([' ', '\t']);
    if trimmed
        .strip_prefix("from")
        .is_some_and(|rest| rest.starts_with([' ', '\t']))
    {
        let leading = line.len() - trimmed.len();
        Some((leading + 1, trimmed))
    } else {
        None
    }
}

fn parse_import(text: &str) -> Result<ImportSpec, String> {
    let mut rest = text
        .strip_prefix("from")
        .ok_or_else(|| "expected 'from' at start of import".to_string())?;
    rest = rest
        .strip_prefix([' ', '\t'])
        .ok_or_else(|| "expected whitespace after 'from'".to_string())?
        .trim_start();

    let (source_value, suffix) = match rest.as_bytes().first() {
        Some(b'<') => {
            let end = rest
                .find('>')
                .ok_or_else(|| "unterminated standard-header import".to_string())?;
            (&rest[1..end], &rest[end + 1..])
        }
        Some(b'"') => {
            let end = find_closing_quote(rest)
                .ok_or_else(|| "unterminated object import path".to_string())?;
            let decoded = decode_quoted_path(&rest[1..end])?;
            let suffix = &rest[end + 1..];
            return parse_import_suffix(&decoded, suffix, true);
        }
        _ => {
            return Err(
                "expected a standard header in angle brackets or object path in quotes".to_string(),
            );
        }
    };

    parse_import_suffix(source_value, suffix, false)
}

fn parse_import_suffix(source: &str, suffix: &str, object: bool) -> Result<ImportSpec, String> {
    if !object && !is_identifier(source) {
        return Err("standard header name must be a simple identifier".to_string());
    }

    let suffix = suffix.trim_start();
    let clause = suffix
        .strip_prefix("import")
        .filter(|rest| rest.is_empty() || rest.starts_with([' ', '\t', '{', '*']))
        .ok_or_else(|| "expected 'import' after import source".to_string())?
        .trim();

    if object {
        if clause != "*" {
            return Err("object imports must use '*': from \"path\" import *".to_string());
        }
        if source.is_empty() {
            return Err("object import path cannot be empty".to_string());
        }
        return Ok(ImportSpec::Object {
            path: PathBuf::from(source),
        });
    }

    let symbols_text = clause
        .strip_prefix('{')
        .and_then(|rest| rest.strip_suffix('}'))
        .ok_or_else(|| "standard imports must use a braced symbol list".to_string())?;
    let symbols: Vec<_> = symbols_text
        .split(',')
        .map(str::trim)
        .map(str::to_string)
        .collect();
    if symbols.is_empty() || symbols.iter().any(|symbol| !is_identifier(symbol)) {
        return Err("standard import symbols must be a non-empty list of identifiers".to_string());
    }

    Ok(ImportSpec::Standard {
        module: source.to_string(),
        symbols,
    })
}

fn find_closing_quote(text: &str) -> Option<usize> {
    let bytes = text.as_bytes();
    let mut index = 1;
    while index < bytes.len() {
        match bytes[index] {
            b'\\' => index += 2,
            b'"' => return Some(index),
            b'\n' | b'\r' => return None,
            _ => index += 1,
        }
    }
    None
}

fn decode_quoted_path(text: &str) -> Result<String, String> {
    let mut decoded = String::with_capacity(text.len());
    let mut chars = text.chars();
    while let Some(character) = chars.next() {
        if character != '\\' {
            decoded.push(character);
            continue;
        }
        match chars.next() {
            Some('\\') => decoded.push('\\'),
            Some('"') => decoded.push('"'),
            Some(_) => {
                return Err(
                    "object import paths only support escaped backslashes and quotes".to_string(),
                )
            }
            None => return Err("object import path ends with an incomplete escape".to_string()),
        }
    }
    Ok(decoded)
}

fn is_identifier(value: &str) -> bool {
    let mut chars = value.chars();
    matches!(chars.next(), Some('_' | 'a'..='z' | 'A'..='Z'))
        && chars.all(|character| character == '_' || character.is_ascii_alphanumeric())
}

fn scan_c_line(line: &[u8], state: &mut LexState, brace_depth: &mut isize, escaped: &mut bool) {
    let mut index = 0;
    while index < line.len() {
        let byte = line[index];
        let next = line.get(index + 1).copied();
        match *state {
            LexState::Normal => match (byte, next) {
                (b'/', Some(b'/')) => {
                    *state = LexState::LineComment;
                    index += 1;
                }
                (b'/', Some(b'*')) => {
                    *state = LexState::BlockComment;
                    index += 1;
                }
                (b'"', _) => {
                    *state = LexState::String;
                    *escaped = false;
                }
                (b'\'', _) => {
                    *state = LexState::Character;
                    *escaped = false;
                }
                (b'{', _) => *brace_depth += 1,
                (b'}', _) => *brace_depth -= 1,
                _ => {}
            },
            LexState::BlockComment => {
                if byte == b'*' && next == Some(b'/') {
                    *state = LexState::Normal;
                    index += 1;
                }
            }
            LexState::LineComment => {
                if byte == b'\n' {
                    *state = LexState::Normal;
                }
            }
            LexState::String | LexState::Character => {
                if *escaped {
                    *escaped = false;
                } else if byte == b'\\' {
                    *escaped = true;
                } else if (*state == LexState::String && byte == b'"')
                    || (*state == LexState::Character && byte == b'\'')
                {
                    *state = LexState::Normal;
                } else if byte == b'\n' {
                    *state = LexState::Normal;
                }
            }
        }
        index += 1;
    }

    if *state == LexState::LineComment {
        *state = LexState::Normal;
    }
}
