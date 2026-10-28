//! naga validation of a user shader composed with its role prelude.

use crate::{ShaderRole, compose, prelude_lines};
use naga::valid::{Capabilities, ValidationFlags, Validator};

/// A shader that does not parse or validate.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ShaderError {
    /// 1-based line in the user's file; `None` when the error is in the prelude or has no span.
    pub line: Option<u32>,
    pub message: String,
}

impl std::fmt::Display for ShaderError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self.line {
            Some(line) => write!(f, "line {line}: {}", self.message),
            None => f.write_str(&self.message),
        }
    }
}

impl std::error::Error for ShaderError {}

fn user_line(role: ShaderRole, composed: Option<u32>) -> Option<u32> {
    let prelude = u32::try_from(prelude_lines(role)).ok()?;
    composed?.checked_sub(prelude).filter(|l| *l > 0)
}

/// Parses and validates `source` as a shader of `role`.
pub fn validate(role: ShaderRole, source: &str) -> Result<(), ShaderError> {
    let full = compose(role, source);
    let module = naga::front::wgsl::parse_str(&full).map_err(|e| ShaderError {
        line: user_line(role, e.location(&full).map(|l| l.line_number)),
        message: e.message().to_owned(),
    })?;
    Validator::new(ValidationFlags::all(), Capabilities::empty())
        .validate(&module)
        .map_err(|e| {
            let line = user_line(role, e.location(&full).map(|l| l.line_number));
            let mut message = e.as_inner().to_string();
            let mut source = std::error::Error::source(e.as_inner());
            while let Some(s) = source {
                message.push_str(": ");
                message.push_str(&s.to_string());
                source = s.source();
            }
            ShaderError { line, message }
        })?;
    Ok(())
}
