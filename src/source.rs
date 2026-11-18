use std::error::Error;
use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SourceLocation {
    pub line: usize,
    pub column: usize,
}

#[derive(Debug)]
pub struct SourceError {
    pub path: PathBuf,
    pub location: Option<SourceLocation>,
    pub message: String,
}

impl SourceError {
    pub fn new(
        path: impl Into<PathBuf>,
        location: Option<SourceLocation>,
        message: String,
    ) -> Self {
        Self {
            path: path.into(),
            location,
            message,
        }
    }
}

impl fmt::Display for SourceError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}", self.path.display())?;
        if let Some(location) = self.location {
            write!(formatter, ":{}:{}", location.line, location.column)?;
        }
        write!(formatter, ": {}", self.message)
    }
}

impl Error for SourceError {}

#[derive(Debug, Clone)]
pub struct SourceFile {
    pub path: PathBuf,
    pub text: String,
}

pub fn canonicalize_input(path: &Path) -> Result<PathBuf, SourceError> {
    fs::canonicalize(path).map_err(|error| {
        SourceError::new(path, None, format!("cannot resolve input path: {error}"))
    })
}

pub fn load_source(path: &Path) -> Result<SourceFile, SourceError> {
    let bytes = fs::read(path)
        .map_err(|error| SourceError::new(path, None, format!("cannot read source: {error}")))?;
    let text = String::from_utf8(bytes).map_err(|error| {
        let valid = error.utf8_error().valid_up_to();
        let valid_prefix = &error.as_bytes()[..valid];
        let line = valid_prefix.iter().filter(|&&byte| byte == b'\n').count() + 1;
        let last_newline = valid_prefix
            .iter()
            .rposition(|&byte| byte == b'\n')
            .map_or(0, |index| index + 1);
        let column = std::str::from_utf8(&valid_prefix[last_newline..])
            .map_or(valid_prefix.len() - last_newline + 1, |prefix| {
                prefix.chars().count() + 1
            });

        SourceError::new(
            path,
            Some(SourceLocation { line, column }),
            "source is not valid UTF-8".to_string(),
        )
    })?;

    Ok(SourceFile {
        path: path.to_path_buf(),
        text,
    })
}
