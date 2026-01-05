use alloy_primitives::Address;
use foundry_compilers::ProjectCompileOutput;
use std::collections::HashMap;

/// Per-group compilation cache and deployment registry
#[derive(Default, Debug)]
pub struct GroupArtifacts {
    /// Compilation cache: compilation_name -> compiled output
    compilations: HashMap<String, ProjectCompileOutput>,

    /// Deployment registry: "compilation:contract" -> address
    deployments: HashMap<String, Address>,
}

impl GroupArtifacts {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn add_compilation(&mut self, name: String, output: ProjectCompileOutput) {
        self.compilations.insert(name, output);
    }

    pub fn get_compilation(&self, name: &str) -> Option<&ProjectCompileOutput> {
        self.compilations.get(name)
    }

    pub fn add_deployment(&mut self, key: String, address: Address) {
        self.deployments.insert(key, address);
    }

    pub fn get_deployment(&self, key: &str) -> Option<Address> {
        self.deployments.get(key).copied()
    }

    pub fn compilations(&self) -> &HashMap<String, ProjectCompileOutput> {
        &self.compilations
    }

    pub fn deployments(&self) -> &HashMap<String, Address> {
        &self.deployments
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloy_primitives::Address;

    #[test]
    fn new_creates_empty_artifacts() {
        let artifacts = GroupArtifacts::new();
        assert_eq!(artifacts.compilations().len(), 0);
        assert_eq!(artifacts.deployments().len(), 0);
    }

    #[test]
    fn insert_and_get_deployment_works() {
        let mut artifacts = GroupArtifacts::new();
        let address = Address::from([0x11u8; 20]);
        artifacts.add_deployment("test:Contract".to_string(), address);

        assert_eq!(artifacts.get_deployment("test:Contract"), Some(address));
        assert_eq!(artifacts.get_deployment("missing"), None);
    }

    #[test]
    fn all_deployments_returns_all() {
        let mut artifacts = GroupArtifacts::new();
        let addr1 = Address::from([0x11u8; 20]);
        let addr2 = Address::from([0x22u8; 20]);

        artifacts.add_deployment("test1:Contract1".to_string(), addr1);
        artifacts.add_deployment("test2:Contract2".to_string(), addr2);

        let all = artifacts.deployments();
        assert_eq!(all.len(), 2);
        assert_eq!(all.get("test1:Contract1"), Some(&addr1));
        assert_eq!(all.get("test2:Contract2"), Some(&addr2));
    }
}
