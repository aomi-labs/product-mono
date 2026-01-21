use std::{collections::HashMap, sync::Arc};

use aomi_core::{CoreApp, Selection};
use aomi_forge::ForgeApp;
use aomi_l2beat::L2BeatApp;
use anyhow::Result;
use aomi_polymarket::PolymarketApp;

use crate::{
    manager::Namespace,
    types::AomiBackend,
};

pub type BackendMappings = HashMap<Namespace, Arc<AomiBackend>>;

#[derive(Clone, Copy, Debug)]
pub struct BuildOpts {
    pub no_docs: bool,
    pub skip_mcp: bool,
    pub selection: Selection,
}

pub async fn build_backends(configs: Vec<(Namespace, BuildOpts)>) -> Result<BackendMappings> {
    let mut backends = HashMap::new();

    for (namespace, opts) in configs {
        let backend: Arc<AomiBackend> = match namespace {
            Namespace::Polymarket => {
                let app = Arc::new(
                    PolymarketApp::default()
                    .await
                    .map_err(|e| anyhow::anyhow!(e.to_string()))?,
                );
                app
            }
            Namespace::Default => {
                let app = Arc::new(
                    CoreApp::new_with_models(
                        opts.no_docs,
                        opts.skip_mcp,
                        opts.selection.rig,
                    )
                    .await
                    .map_err(|e| anyhow::anyhow!(e.to_string()))?,
                );
                app
            }
            Namespace::L2b => {
                let app = Arc::new(
                    L2BeatApp::new_with_models(
                        opts.no_docs,
                        opts.skip_mcp,
                        opts.selection,
                    )
                    .await
                    .map_err(|e| anyhow::anyhow!(e.to_string()))?,
                );
                app
            }
            Namespace::Forge => {
                let app = Arc::new(
                    ForgeApp::new_with_models(
                        opts.no_docs,
                        opts.skip_mcp,
                        opts.selection,
                    )
                    .await
                    .map_err(|e| anyhow::anyhow!(e.to_string()))?,
                );
                app
            }
            Namespace::Test => {
                let app = Arc::new(
                    CoreApp::new_with_models(
                        opts.no_docs,
                        opts.skip_mcp,
                        opts.selection.rig,
                    )
                    .await
                    .map_err(|e| anyhow::anyhow!(e.to_string()))?,
                );
                app
            }
        };

        backends.insert(namespace, backend);
    }

    Ok(backends)
}
