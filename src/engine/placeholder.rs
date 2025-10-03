use anyhow::Result;

use super::{ExecutionOutcome, ExecutionPayload, LanguageEngine};

pub struct PlaceholderEngine {
    id: &'static str,
}

impl PlaceholderEngine {
    pub fn new(id: &'static str) -> Self {
        Self { id }
    }
}

impl LanguageEngine for PlaceholderEngine {
    fn id(&self) -> &'static str {
        self.id
    }

    fn display_name(&self) -> &'static str {
        self.id
    }

    fn aliases(&self) -> &[&'static str] {
        &[]
    }

    fn execute(&self, _payload: &ExecutionPayload) -> Result<ExecutionOutcome> {
        Ok(ExecutionOutcome {
            language: self.id.to_string(),
            exit_code: Some(1),
            stdout: String::new(),
            stderr: format!(
                "Language '{}' is not implemented yet. Contributions welcome!",
                self.id
            ),
            duration: Default::default(),
        })
    }
}
