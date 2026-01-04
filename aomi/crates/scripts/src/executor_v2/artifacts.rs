use alloy_primitives::Address;
use foundry_compilers::ProjectCompileOutput;
use std::collections::HashMap;

#[derive(Debug, Default)]
pub struct GroupArtifacts {
    pub compilations: HashMap<String, ProjectCompileOutput>,
    pub deployments: HashMap<String, Address>,
}

impl GroupArtifacts {
    pub fn new() -> Self {
        Self::default()
    }
}
