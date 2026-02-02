flowchart TD
    subgraph AppLayer["App Layer"]
      AdminApp["Admin App\n(crates/apps/admin)"]
      AdminTools["Admin Tools"]
    end

    subgraph Backend["Backend"]
      Manager["Session Manager"]
      Endpoint["HTTP Endpoint"]
      Trigger["admin-magic trigger"]
    end

    subgraph Clients["Clients"]
      CLI["CLI"]
      TUI["TUI"]
    end

    subgraph Stores["DB Abstractions (tools crate)"]
      ContractStore["ContractStore"]
      SessionStore["SessionStore"]
      ApiKeyStore["ApiKeyStore"]
    end

    subgraph Tables["Postgres Tables"]
      Contracts["contracts"]
      Users["users"]
      Sessions["sessions"]
      ApiKeys["api_keys"]
    end

    subgraph AdminToolList["Admin Tools (admin_* )"]
      TCreateKey["admin_create_api_key"]
      TListKeys["admin_list_api_keys"]
      TUpdateKey["admin_update_api_key"]
      TListUsers["admin_list_users"]
      TUpdateUser["admin_update_user"]
      TDeleteUser["admin_delete_user"]
      TListSessions["admin_list_sessions"]
      TUpdateSession["admin_update_session"]
      TDeleteSession["admin_delete_session"]
      TListContracts["admin_list_contracts"]
      TUpdateContract["admin_update_contract"]
      TDeleteContract["admin_delete_contract"]
    end

    AdminApp --> AdminTools
    AdminTools --> AdminToolList

    TCreateKey --> ApiKeyStore
    TListKeys --> ApiKeyStore
    TUpdateKey --> ApiKeyStore

    TListUsers --> SessionStore
    TUpdateUser --> SessionStore
    TDeleteUser --> SessionStore
    TListSessions --> SessionStore
    TUpdateSession --> SessionStore
    TDeleteSession --> SessionStore

    TListContracts --> ContractStore
    TUpdateContract --> ContractStore
    TDeleteContract --> ContractStore

    ApiKeyStore --> ApiKeys
    SessionStore --> Users
    SessionStore --> Sessions
    ContractStore --> Contracts

    Endpoint --> Trigger
    Trigger --> Manager
    Manager --> AdminApp

    CLI --> Manager
    TUI --> Manager