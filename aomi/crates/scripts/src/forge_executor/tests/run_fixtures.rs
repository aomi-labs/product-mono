use crate::forge_executor::plan::OperationGroup;
use aomi_forge::tools::{
    NextGroups, NextGroupsParameters, SetExecutionPlan, SetExecutionPlanParameters,
};
use anyhow::{Context, Result, anyhow};
use rig::tool::Tool;
use serde::Deserialize;
use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};
use tokio::time::{Duration, timeout};
use tracing_subscriber::{EnvFilter, fmt};

const FIXTURE_DIR: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/src/forge_executor/tests/fixtures"
);

#[derive(Debug, Deserialize, Clone)]
struct FixtureContract {
    chain_id: String,
    address: String,
    name: String,
}

#[derive(Debug, Deserialize, Clone)]
struct FixtureGroup {
    description: String,
    operations: Vec<String>,
    #[serde(default)]
    dependencies: Vec<usize>,
    #[serde(default)]
    contracts: Vec<FixtureContract>,
}

#[derive(Debug, Deserialize, Clone)]
struct FixtureFile {
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    description: Option<String>,
    groups: Vec<FixtureGroup>,
}

#[derive(Debug, Clone)]
struct LoadedFixture {
    name: String,
    #[allow(dead_code)]
    description: Option<String>,
    groups: Vec<FixtureGroup>,
}

impl LoadedFixture {
    fn to_operation_groups(&self) -> Vec<OperationGroup> {
        self.groups
            .iter()
            .map(|group| OperationGroup {
                description: group.description.clone(),
                operations: group.operations.clone(),
                dependencies: group.dependencies.clone(),
                contracts: group
                    .contracts
                    .iter()
                    .map(|c| (c.chain_id.clone(), c.address.clone(), c.name.clone()))
                    .collect(),
            })
            .collect()
    }
}

fn fixture_paths(dir: &Path) -> Result<Vec<PathBuf>> {
    let entries = fs::read_dir(dir).with_context(|| format!("reading {}", dir.display()))?;
    let mut paths = Vec::new();
    for entry in entries {
        let entry = entry?;
        let path = entry.path();
        if path
            .extension()
            .and_then(|ext| ext.to_str())
            .map(|ext| ext.eq_ignore_ascii_case("json"))
            .unwrap_or(false)
        {
            paths.push(path);
        }
    }
    paths.sort();
    Ok(paths)
}

fn load_fixtures() -> Result<Vec<LoadedFixture>> {
    let paths = fixture_paths(Path::new(FIXTURE_DIR))?;
    let mut fixtures = Vec::new();
    let filter = std::env::var("FORGE_TEST_FIXTURE_FILTER").ok().map(|val| {
        val.split(',')
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect::<HashSet<_>>()
    });

    for path in paths {
        let contents = fs::read_to_string(&path)
            .with_context(|| format!("reading fixture {}", path.display()))?;
        let parsed: FixtureFile = serde_json::from_str(&contents)
            .with_context(|| format!("parsing fixture {}", path.display()))?;
        let name = parsed.name.clone().unwrap_or_else(|| {
            path.file_stem()
                .unwrap_or_default()
                .to_string_lossy()
                .to_string()
        });
        let loaded = LoadedFixture {
            name,
            description: parsed.description.clone(),
            groups: parsed.groups.clone(),
        };

        // Apply filter if provided
        if let Some(f) = &filter
            && !f.is_empty()
            && !f
                .iter()
                .any(|needle| loaded.name.contains(needle) || loaded.name == *needle)
        {
            continue;
        }
        fixtures.push(loaded);
    }

    Ok(fixtures)
}

fn require_env(var: &str) -> Result<String> {
    std::env::var(var).map_err(|_| anyhow!("Environment variable {} must be set", var))
}

fn display_generated_code(code: &str) {
    println!("   ┌─ Generated Forge Script ─────────────────");
    for line in code.lines() {
        println!("   │ {}", line);
    }
    println!("   └───────────────────────────────────────────");
}

fn display_transactions(txs: &[serde_json::Value]) {
    println!("   📤 Transactions ({}):", txs.len());
    for (i, tx) in txs.iter().enumerate() {
        let to = tx.get("to").and_then(|t| t.as_str()).unwrap_or("N/A");
        let value = tx.get("value").and_then(|v| v.as_str()).unwrap_or("0");
        let data = tx.get("data").and_then(|d| d.as_str()).unwrap_or("");

        // Extract function selector for description
        let func_desc = if data.len() >= 10 {
            let selector = &data[0..10];
            format!("function call ({})", selector)
        } else if !data.is_empty() {
            "raw data".to_string()
        } else {
            "ETH transfer".to_string()
        };

        let value_desc = if value != "0" && value != "0x0" {
            format!(" with {} wei", value)
        } else {
            String::new()
        };

        println!("      {}. {} to {}{}", i + 1, func_desc, to, value_desc);
    }
}

async fn run_fixture_with_tools(fixture: &LoadedFixture) -> Result<()> {
    let set_tool = SetExecutionPlan;
    let next_tool = NextGroups;

    let set_params = SetExecutionPlanParameters {
        groups: fixture.to_operation_groups(),
    };
    println!(
        "📋 Setting execution plan with {} operation groups",
        set_params.groups.len()
    );

    let set_result = set_tool
        .call(set_params)
        .await
        .with_context(|| format!("setting plan for {}", fixture.name))?;

    let set_response: serde_json::Value = serde_json::from_str(&set_result)?;
    let total_groups = set_response["total_groups"].as_u64().unwrap_or(0) as usize;
    let plan_id = set_response["plan_id"]
        .as_str()
        .ok_or_else(|| anyhow!("set_execution_plan response missing plan_id"))?
        .to_string();
    println!(
        "✓ Plan initialized: {} groups ready to execute",
        total_groups
    );
    let mut remaining = total_groups;

    let next_params = NextGroupsParameters { plan_id };
    let mut iterations = 0usize;
    let mut prev_remaining = remaining;

    while remaining > 0 {
        iterations += 1;
        println!(
            "⚙️  Executing batch {} ({} groups remaining)",
            iterations, remaining
        );
        if iterations > fixture.groups.len() * 2 + 2 {
            return Err(anyhow!(
                "Exceeded iteration budget while executing {}",
                fixture.name
            ));
        }

        let batch_result = timeout(
            Duration::from_secs(180),
            next_tool.call(next_params.clone()),
        )
        .await
        .map_err(|_| anyhow!("Timeout waiting for next_groups for {}", fixture.name))??;

        let batch: serde_json::Value = serde_json::from_str(&batch_result)?;
        if let Some(results) = batch["results"].as_array() {
            for result in results {
                let group_idx = result
                    .get("group_index")
                    .and_then(|v| v.as_u64())
                    .unwrap_or_default();
                let desc = result
                    .get("description")
                    .and_then(|v| v.as_str())
                    .unwrap_or_default();

                if let Some(failed) = result.get("inner").and_then(|i| i.get("Failed")) {
                    let err = failed
                        .get("error")
                        .and_then(|v| v.as_str())
                        .unwrap_or("unknown error");

                    // Display generated code even on failure for debugging
                    println!("\n   ❌ Group {}: {} - FAILED", group_idx, desc);
                    if let Some(code) = failed.get("generated_code").and_then(|c| c.as_str()) {
                        display_generated_code(code);
                    }
                    if let Some(txs) = failed.get("transactions").and_then(|t| t.as_array())
                        && !txs.is_empty()
                    {
                        display_transactions(txs);
                    }

                    return Err(anyhow!(
                        "Fixture {} group {} ({}) failed: {}",
                        fixture.name,
                        group_idx,
                        desc,
                        err
                    ));
                }

                // Display successful execution results
                if let Some(done) = result.get("inner").and_then(|i| i.get("Done")) {
                    println!("\n   📝 Group {}: {}", group_idx, desc);

                    // Display generated Forge script
                    if let Some(code) = done.get("generated_code").and_then(|c| c.as_str()) {
                        display_generated_code(code);
                    }

                    // Display transactions
                    if let Some(txs) = done.get("transactions").and_then(|t| t.as_array()) {
                        display_transactions(txs);
                    }
                }
            }
        }

        remaining = batch["remaining_groups"].as_u64().unwrap_or(0) as usize;

        if remaining >= prev_remaining {
            return Err(anyhow!(
                "No progress while executing {} (remaining: {}, last batch: {})",
                fixture.name,
                remaining,
                batch_result
            ));
        }
        prev_remaining = remaining;
    }

    println!("✓ All {} groups executed successfully", total_groups);
    Ok(())
}

#[test]
fn test_fixture_files_are_well_formed() {
    let fixtures = load_fixtures().expect("fixtures should parse");
    assert!(
        !fixtures.is_empty(),
        "Add at least one fixture under {}",
        FIXTURE_DIR
    );

    for fixture in fixtures {
        assert!(
            !fixture.groups.is_empty(),
            "fixture {} must have operation groups",
            fixture.name
        );
        for group in fixture.groups {
            assert!(
                !group.description.is_empty(),
                "group description is required"
            );
            assert!(
                !group.operations.is_empty(),
                "group {} needs at least one operation",
                group.description
            );
        }
    }
}

#[tokio::test(flavor = "multi_thread")]
#[ignore] // Requires BAML/Etherscan and a local fork
async fn test_fixture_workflows_via_tools() -> Result<()> {
    // Initialize tracing subscriber to read RUST_LOG environment variable
    let _ = fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .try_init();

    let _ = require_env("ETHERSCAN_API_KEY")?;

    let fixtures = load_fixtures()?;
    assert!(
        !fixtures.is_empty(),
        "no fixtures found under {}",
        FIXTURE_DIR
    );

    for fixture in fixtures {
        println!("\n═══════════════════════════════════════════════════");
        println!("🚀 Running fixture: {}", fixture.name);
        println!("═══════════════════════════════════════════════════");
        timeout(Duration::from_secs(240), run_fixture_with_tools(&fixture))
            .await
            .map_err(|_| anyhow!("Timed out executing fixture {}", fixture.name))??;
        println!("✅ Completed fixture: {}\n", fixture.name);
    }

    Ok(())
}
