use anyhow::{Result, bail};
use serde_json::Value;

use crate::cli::{ContractDeleteArgs, ContractListArgs, ContractUpdateArgs};
use crate::util::print_json;
use aomi_tools::db::{ContractSearchParams, ContractStore, ContractStoreApi, ContractUpdate};

const CONTRACT_PREVIEW_LEN: usize = 200;

pub async fn list_contracts(args: ContractListArgs, pool: &sqlx::AnyPool) -> Result<()> {
    let store = ContractStore::new(pool.clone());
    let params = ContractSearchParams {
        chain_id: args.chain_id.map(|value| value as u32),
        address: args.address,
        name: args.name,
        symbol: args.symbol,
        protocol: args.protocol,
        contract_type: args.contract_type,
        version: args.version,
    };
    let rows = store
        .list_contracts(params, args.limit, args.offset)
        .await?;
    let json_rows = rows
        .iter()
        .map(|row| {
            let source_preview = truncate_preview(&row.source_code, CONTRACT_PREVIEW_LEN);
            let abi_preview = truncate_preview(
                &serde_json::to_string(&row.abi).unwrap_or_default(),
                CONTRACT_PREVIEW_LEN,
            );
            Ok(serde_json::json!({
                "address": row.address,
                "chain": row.chain,
                "chain_id": row.chain_id,
                "name": row.name.clone().unwrap_or_default(),
                "symbol": row.symbol,
                "protocol": row.protocol,
                "contract_type": row.contract_type,
                "version": row.version,
                "is_proxy": row.is_proxy.unwrap_or(false),
                "implementation_address": row.implementation_address,
                "updated_at": row.updated_at.unwrap_or_default(),
                "source_code": source_preview,
                "abi": abi_preview,
            }))
        })
        .collect::<Result<Vec<Value>>>()?;

    print_json(&Value::from(json_rows))?;
    Ok(())
}

pub async fn update_contract(args: ContractUpdateArgs, pool: &sqlx::AnyPool) -> Result<()> {
    if args.clear_symbol && args.symbol.is_some() {
        bail!("cannot set --symbol and --clear-symbol together");
    }

    if args.clear_protocol && args.protocol.is_some() {
        bail!("cannot set --protocol and --clear-protocol together");
    }

    if args.clear_contract_type && args.contract_type.is_some() {
        bail!("cannot set --contract-type and --clear-contract-type together");
    }

    if args.clear_version && args.version.is_some() {
        bail!("cannot set --version and --clear-version together");
    }

    if args.is_proxy && args.not_proxy {
        bail!("cannot set --is-proxy and --not-proxy together");
    }

    if args.clear_implementation_address && args.implementation_address.is_some() {
        bail!("cannot set --implementation-address and --clear-implementation-address together");
    }

    if args.clear_description && args.description.is_some() {
        bail!("cannot set --description and --clear-description together");
    }

    let update = ContractUpdate {
        chain_id: args.chain_id as u32,
        address: args.address,
        name: args.name,
        symbol: args.symbol,
        clear_symbol: args.clear_symbol,
        protocol: args.protocol,
        clear_protocol: args.clear_protocol,
        contract_type: args.contract_type,
        clear_contract_type: args.clear_contract_type,
        version: args.version,
        clear_version: args.clear_version,
        is_proxy: if args.is_proxy || args.not_proxy {
            Some(args.is_proxy)
        } else {
            None
        },
        implementation_address: args.implementation_address,
        clear_implementation_address: args.clear_implementation_address,
        description: args.description,
        clear_description: args.clear_description,
    };

    let store = ContractStore::new(pool.clone());
    let row = store.update_contract(update).await?;
    print_json(&serde_json::json!({
        "address": row.address,
        "chain": row.chain,
        "chain_id": row.chain_id,
        "name": row.name.unwrap_or_default(),
        "symbol": row.symbol,
        "protocol": row.protocol,
        "contract_type": row.contract_type,
        "version": row.version,
        "is_proxy": row.is_proxy.unwrap_or(false),
        "implementation_address": row.implementation_address,
        "updated_at": row.updated_at.unwrap_or_default(),
    }))?;
    Ok(())
}

pub async fn delete_contract(args: ContractDeleteArgs, pool: &sqlx::AnyPool) -> Result<()> {
    let store = ContractStore::new(pool.clone());
    let existed = store
        .get_contract(args.chain_id as u32, args.address.clone())
        .await?
        .is_some();
    store
        .delete_contract(args.chain_id as u32, args.address.clone())
        .await?;

    print_json(&serde_json::json!({
        "chain_id": args.chain_id,
        "address": args.address,
        "deleted": if existed { 1 } else { 0 },
    }))?;
    Ok(())
}

fn truncate_preview(value: &str, max_len: usize) -> String {
    if value.chars().count() <= max_len {
        return value.to_string();
    }
    let mut out: String = value.chars().take(max_len).collect();
    out.push_str("...");
    out
}
