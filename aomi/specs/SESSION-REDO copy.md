 Core Data Types Only

  // ============================================================================
  // 1. CONTRACT ARTIFACTS (Per-group compilation cache)
  // ============================================================================
  pub struct GroupArtifacts {
      /// Compilation cache: name -> compiled output
      /// Type: HashMap<String, foundry_compilers::ProjectCompileOutput>
      compilations: HashMap<String, ProjectCompileOutput>,

      /// Deployment registry: "compilation:contract" -> address
      /// Type: HashMap<String, alloy_primitives::Address>
      deployments: HashMap<String, Address>,
  }



  // ============================================================================
  // 2. NODE SESSION (Per-group state - compilation only)
  // ============================================================================
  pub struct GroupNode {
      /// Group identifier
      pub group_id: String,

      /// This group's compiled contracts and deployments
      pub artifacts: GroupArtifacts,

      /// Shared config reference
      /// Type: Arc<GroupConfig>
      config: Arc<GroupConfig>,
  }


  // ============================================================================
  // 3. SESSION CONFIG (Immutable configuration)
  // ============================================================================
  pub struct GroupConfig {
      /// Foundry project configuration
      /// Type: Arc<foundry_config::Config>
      pub foundry_config: Arc<Config>,

      /// EVM runtime options (fork URL, gas limits, etc)
      /// Type: foundry_evm::opts::EvmOpts
      pub evm_opts: EvmOpts,

      /// Compiler: disable solc auto-detection
      pub no_auto_detect: bool,

      /// Session identifier
      pub id: Option<String>,
  }


  // ============================================================================
  // 4. BACKEND WRAPPER (Shared execution state with locking)
  // ============================================================================
  pub struct ExecutionBackend {
      /// Shared backend (manages all forks)
      backend: Arc<Mutex<Backend>>,

      /// Per-chain state
      journals: Arc<Mutex<HashMap<ChainId, JournaledState>>>,
      envs: Arc<Mutex<HashMap<ChainId, Env>>>,
      inspectors: Arc<Mutex<HashMap<ChainId, InspectorStack>>>,
  }

  // Manually build EVM on each execution
  impl ExecutionBackend {
      async fn execute_on_chain<F, R>(&self, chain: ChainId, f: F) -> Result<R> {
          // Lock everything
          let mut backend = self.backend.lock().await;
          let mut journals = self.journals.lock().await;
          let inspectors = self.inspectors.lock().await;

          // Switch fork
          backend.select_fork(...)?;

          // Build EVM manually (no Executor)
          let mut evm = Evm::builder()
              .with_db(&mut *backend)
              .with_env(...)
              .with_external_context(inspectors.get(&chain).unwrap())
              .build();

          f(&mut evm)
      }
  }
  ---
  📋 Type Summary

  | Type              | Purpose           | Key Fields                                                                               | Library Types                                                    |
  |-------------------|-------------------|------------------------------------------------------------------------------------------|------------------------------------------------------------------|
  | GroupArtifacts | Compilation cache | compilations: HashMap<String, ProjectCompileOutput>deployments: HashMap<String, Address> | foundry_compilers::ProjectCompileOutputalloy_primitives::Address |
  | GroupNode      | Per-group state   | group_id: Stringartifacts: GroupArtifactsconfig: Arc<GroupConfig>                   | -                                                                |
  | GroupConfig     | Immutable config  | foundry_config: Arc<Config>evm_opts: EvmOpts                                             | foundry_config::Configfoundry_evm::opts::EvmOpts                 |
  | ExecutionBackend    | Shared execution  | executor: Arc<Mutex<Executor>>snapshots: Arc<RwLock<HashMap<String, U256>>>              | foundry_evm::executors::Executorrevm::primitives::U256           |

  ---
  🎯 Usage Example (Your Coordinator Integration)

⏺ // 1. SETUP - Create backend with Ethereum fork
  let eth_fork = backend.create_fork(CreateFork {
      url: "https://eth.llamarpc.com",
      enable_caching: true,
      env: eth_env.clone(),
      evm_opts: evm_opts.clone(),
  });
  let eth_fork_id = backend.insert_fork(eth_fork);

  let op_fork = backend.create_fork(CreateFork {
      url: "https://optimism.llamarpc.com",
      // ...
  });
  let op_fork_id = backend.insert_fork(op_fork);

  let wrapper = BackendWrapper {
      backend: Arc::new(Mutex::new(backend)),
      journals: Arc::new(Mutex::new(HashMap::from([
          (ChainId::Ethereum, JournaledState::new()),
          (ChainId::Optimism, JournaledState::new()),
      ]))),
      envs: Arc::new(Mutex::new(HashMap::from([
          (ChainId::Ethereum, eth_env),
          (ChainId::Optimism, op_env),
      ]))),
      inspectors: Arc::new(Mutex::new(HashMap::from([
          (ChainId::Ethereum, InspectorStack::new()),
          (ChainId::Optimism, InspectorStack::new()),
      ]))),
  };

  // 2. RUN ON ETHEREUM
  wrapper.execute_on_chain(ChainId::Ethereum, eth_fork_id, |evm| {
      evm.transact()?;  // Deploy/call on Ethereum
      Ok(())
  }).await?;

  // 3. SWITCH TO OPTIMISM
  wrapper.execute_on_chain(ChainId::Optimism, op_fork_id, |evm| {
      evm.transact()?;  // Deploy/call on Optimism
      Ok(())
  }).await?;

  // 4. BACK TO ETHEREUM (state preserved)
  wrapper.execute_on_chain(ChainId::Ethereum, eth_fork_id, |evm| {
      // Ethereum state still here
      Ok(())
  }).await?;

  ---
  🔑 Key Points

  1. GroupNode = per-group, concurrent compilation
  2. GroupArtifacts = holds ProjectCompileOutput + deployment addresses
  3. ExecutionBackend = singleton, shared Executor, locked access
  4. Your orchestrator handles dependency waiting before calling request_backend()

  ---
  🧭 Refactor Steps (Executor v2)

  1. Extract data model
     - Add GroupArtifacts, GroupConfig, ExecutionBackend types under executor_v2/.
     - Ensure GroupArtifacts owns ProjectCompileOutput + deployments map.

  2. Define GroupNode API
     - Fields: execution_id, node_id, group: OperationGroup, artifacts, config.
     - Methods: prepare_sources(), generate_script(), compile_script(), execute_on_chain().

  3. Build ForgeOrchestrator
     - State: DashMap<execution_id, ExecutionPlan>.
     - Results: DashMap<execution_id, (sender, Vec<GroupResult>)>.
     - Active: Vec<Arc<Mutex<GroupNode>>>.

  4. Wire async execution loop
     - Spawn GroupNode::run() for ready groups.
     - Poll active_nodes; on completion update plan status.
     - Append GroupResult into results map and notify sender.

  5. Replace execute_single_group call sites
     - Redirect next_groups to orchestrator + GroupNode flow.
     - Remove monolithic ForgeExecutor execution path.

  6. Validate with fixtures
     - Run existing forge_executor tests against executor_v2.
     - Ensure ACK + async updates match current tool expectations.
