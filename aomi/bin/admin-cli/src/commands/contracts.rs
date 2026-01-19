use anyhow::{bail, Result};
use serde_json::Value;
use sqlx::{Postgres, QueryBuilder};

use crate::cli::{ContractDeleteArgs, ContractListArgs, ContractUpdateArgs};
use crate::models::ContractRow;
use crate::util::print_json;

pub async fn list_contracts(args: ContractListArgs, pool: &sqlx::PgPool) -> Result<()> {
    let mut query = QueryBuilder::<Postgres>::new(
        "SELECT address, chain, chain_id, name, symbol, protocol, contract_type, version, is_proxy, implementation_address, updated_at FROM contracts WHERE 1=1",
    );

    if let Some(chain_id) = args.chain_id {
        query.push(" AND chain_id = ").push_bind(chain_id);
    }

    if let Some(address) = args.address {
        query.push(" AND address = ").push_bind(address);
    }

    if let Some(symbol) = args.symbol {
        query.push(" AND symbol = ").push_bind(symbol);
    }

    if let Some(name) = args.name {
        query
            .push(" AND LOWER(name) LIKE LOWER(")
            .push_bind(format!("%{name}%"))
            .push(")");
    }

    if let Some(protocol) = args.protocol {
        query
            .push(" AND LOWER(protocol) LIKE LOWER(")
            .push_bind(format!("%{protocol}%"))
            .push(")");
    }

    if let Some(contract_type) = args.contract_type {
        query.push(" AND contract_type = ").push_bind(contract_type);
    }

    if let Some(version) = args.version {
        query.push(" AND version = ").push_bind(version);
    }

    query.push(" ORDER BY updated_at DESC");

    if let Some(limit) = args.limit {
        query.push(" LIMIT ").push_bind(limit);
    }

    if let Some(offset) = args.offset {
        query.push(" OFFSET ").push_bind(offset);
    }

    let rows: Vec<ContractRow> = query.build_query_as().fetch_all(pool).await?;
    let json_rows = rows
        .iter()
        .map(|row| {
            Ok(serde_json::json!({
                "address": row.address,
                "chain": row.chain,
                "chain_id": row.chain_id,
                "name": row.name,
                "symbol": row.symbol,
                "protocol": row.protocol,
                "contract_type": row.contract_type,
                "version": row.version,
                "is_proxy": row.is_proxy,
                "implementation_address": row.implementation_address,
                "updated_at": row.updated_at,
            }))
        })
        .collect::<Result<Vec<Value>>>()?;

    print_json(&Value::from(json_rows))?;
    Ok(())
}

pub async fn update_contract(args: ContractUpdateArgs, pool: &sqlx::PgPool) -> Result<()> {
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

    let mut updates = 0;
    let mut query = QueryBuilder::<Postgres>::new("UPDATE contracts SET ");
    let mut separated = query.separated(", ");

    if let Some(name) = args.name {
        separated.push("name = ").push_bind_unseparated(name);
        updates += 1;
    }

    if let Some(symbol) = args.symbol {
        separated.push("symbol = ").push_bind_unseparated(symbol);
        updates += 1;
    }

    if args.clear_symbol {
        separated.push("symbol = NULL");
        updates += 1;
    }

    if let Some(protocol) = args.protocol {
        separated.push("protocol = ").push_bind_unseparated(protocol);
        updates += 1;
    }

    if args.clear_protocol {
        separated.push("protocol = NULL");
        updates += 1;
    }

    if let Some(contract_type) = args.contract_type {
        separated.push("contract_type = ").push_bind_unseparated(contract_type);
        updates += 1;
    }

    if args.clear_contract_type {
        separated.push("contract_type = NULL");
        updates += 1;
    }

    if let Some(version) = args.version {
        separated.push("version = ").push_bind_unseparated(version);
        updates += 1;
    }

    if args.clear_version {
        separated.push("version = NULL");
        updates += 1;
    }

    if args.is_proxy || args.not_proxy {
        separated.push("is_proxy = ").push_bind_unseparated(args.is_proxy);
        updates += 1;
    }

    if let Some(implementation_address) = args.implementation_address {
        separated
            .push("implementation_address = ")
            .push_bind_unseparated(implementation_address);
        updates += 1;
    }

    if args.clear_implementation_address {
        separated.push("implementation_address = NULL");
        updates += 1;
    }

    if let Some(description) = args.description {
        separated.push("description = ").push_bind_unseparated(description);
        updates += 1;
    }

    if args.clear_description {
        separated.push("description = NULL");
        updates += 1;
    }

    if updates == 0 {
        bail!("no fields provided to update");
    }

    let now = chrono::Utc::now().timestamp();
    separated
        .push("updated_at = ")
        .push_bind_unseparated(now);

    query.push(" WHERE chain_id = ").push_bind(args.chain_id);
    query.push(" AND address = ").push_bind(args.address);
    query.push(
        " RETURNING address, chain, chain_id, name, symbol, protocol, contract_type, version, is_proxy, implementation_address, updated_at",
    );

    let row: ContractRow = query.build_query_as().fetch_one(pool).await?;
    print_json(&serde_json::json!({
        "address": row.address,
        "chain": row.chain,
        "chain_id": row.chain_id,
        "name": row.name,
        "symbol": row.symbol,
        "protocol": row.protocol,
        "contract_type": row.contract_type,
        "version": row.version,
        "is_proxy": row.is_proxy,
        "implementation_address": row.implementation_address,
        "updated_at": row.updated_at,
    }))?;
    Ok(())
}

pub async fn delete_contract(args: ContractDeleteArgs, pool: &sqlx::PgPool) -> Result<()> {
    let result = sqlx::query("DELETE FROM contracts WHERE chain_id = $1 AND address = $2")
        .bind(args.chain_id)
        .bind(&args.address)
        .execute(pool)
        .await?;

    print_json(&serde_json::json!({
        "chain_id": args.chain_id,
        "address": args.address,
        "deleted": result.rows_affected(),
    }))?;
    Ok(())
}
