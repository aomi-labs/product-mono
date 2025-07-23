use alloy_primitives::Address;
use alloy_provider::{RootProvider, network::AnyNetwork};
use chisel::prelude::{SessionSource, SessionSourceConfig};
use foundry_cli::{opts::RpcOpts, utils::LoadConfig};
use foundry_compilers::solc::Solc;
use foundry_config::Config;
use foundry_evm_core::opts::EvmOpts;
use rmcp::{Error as McpError, RoleServer, ServerHandler, model::*, service::RequestContext};

pub struct ChiselMCP {
    config: Config,
    provider: RootProvider<AnyNetwork>,
    session: SessionSource,
    //
}

impl ChiselMCP {
    pub fn new(config: Config) -> Self {
        let mut config = RpcOpts::default().load_config().unwrap();
        let evm_opts = EvmOpts::default();

        // TODO: hacking cuz mcp desn't read env properly
        config.eth_rpc_url = Some(
            "https://eth-mainnet.g.alchemy.com/v2/4UjEl1ULr2lQYsGR5n7gGKd3pzgAzxKs".to_string(),
        );
        config.etherscan_api_key = Some("BYY29WWH6IHAB2KS8DXFG2S7YP9C5GQXT5".to_string());
        let provider = foundry_cli::utils::get_provider(&config).unwrap();

        let session_config = SessionSourceConfig {
            // Enable traces if any level of verbosity was passed
            traces: config.verbosity > 0,
            foundry_config: config.clone(),
            evm_opts,
            no_vm: false, // TODO: check if this is correct
            backend: None,
            calldata: None,
        };
        let solc =
            Solc::find_or_install(&semver::Version::new(0, 8, 19)).expect("could not install solc");

        let session = SessionSource::new(solc, session_config);
        Self {
            config,
            provider,
            session,
        }
    }
}

impl ServerHandler for ChiselMCP {
    fn get_info(&self) -> ServerInfo {
        ServerInfo {
            protocol_version: ProtocolVersion::V_2024_11_05,
            capabilities: ServerCapabilities::builder().enable_resources().build(),
            server_info: Implementation::from_build_env(),
            instructions: Some(
                "Chisel MCP exposes the current session as a Forge script resource.".to_string(),
            ),
        }
    }

    async fn list_resources(
        &self,
        _request: Option<PaginatedRequestParam>,
        _: RequestContext<RoleServer>,
    ) -> Result<ListResourcesResult, McpError> {
        let resource = RawResource {
            uri: "chisel://session/source".to_string(),
            name: "Chisel Session Source".to_string(),
            description: Some("The current Chisel session as a Forge script".to_string()),
            mime_type: Some("text/plain".to_string()),
            size: None,
        };
        Ok(ListResourcesResult {
            resources: vec![resource.no_annotation()],
            next_cursor: None,
        })
    }

    async fn read_resource(
        &self,
        ReadResourceRequestParam { uri }: ReadResourceRequestParam,
        _: RequestContext<RoleServer>,
    ) -> Result<ReadResourceResult, McpError> {
        if uri == "chisel://session/source" {
            let code = self.session.to_script_source();
            Ok(ReadResourceResult {
                contents: vec![ResourceContents::text(code, uri)],
            })
        } else {
            Err(McpError::resource_not_found(
                "resource_not_found",
                Some(serde_json::json!({"uri": uri})),
            ))
        }
    }
}
