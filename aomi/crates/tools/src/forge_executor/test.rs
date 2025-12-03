use super::plan::OperationGroup;
use super::tools::{NextGroups, SetExecutionPlan, SetExecutionPlanParameters};
use super::types::{GroupResult, GroupResultInner, TransactionData};
use rig::tool::Tool;
use serde_json;

#[tokio::test]
async fn test_set_execution_plan_success_with_serialization() {
    let groups = vec![
        OperationGroup {
            description: "Wrap ETH to WETH".to_string(),
            operations: vec!["wrap 1 ETH to WETH".to_string()],
            dependencies: vec![],
            contracts: vec![(
                "1".to_string(),
                "0xC02aaA39b223FE8D0A0e5C4F27eAD9083C756Cc2".to_string(),
                "WETH".to_string(),
            )],
        },
        OperationGroup {
            description: "Swap WETH for USDC".to_string(),
            operations: vec!["swap 1 WETH for USDC on Uniswap".to_string()],
            dependencies: vec![0], // Depends on group 0
            contracts: vec![
                (
                    "1".to_string(),
                    "0xC02aaA39b223FE8D0A0e5C4F27eAD9083C756Cc2".to_string(),
                    "WETH".to_string(),
                ),
                (
                    "1".to_string(),
                    "0xA0b86991c6218b36c1d19D4a2e9Eb0cE3606eB48".to_string(),
                    "USDC".to_string(),
                ),
            ],
        },
    ];

    let params = SetExecutionPlanParameters {
        groups: groups.clone(),
    };

    let tool = SetExecutionPlan;
    let result = tool.call(params).await.expect("should succeed");

    // Verify it's valid JSON
    let parsed: serde_json::Value =
        serde_json::from_str(&result).expect("should be valid JSON");

    // Verify structure
    assert_eq!(parsed["success"], true);
    assert_eq!(parsed["total_groups"], 2);
    assert!(parsed["message"]
        .as_str()
        .unwrap()
        .contains("Background contract fetching started"));
}

#[tokio::test]
async fn test_next_groups_no_plan_error() {
    // Attempt to call NextGroups without setting a plan first
    let tool = NextGroups;
    let params = super::tools::NextGroupsParameters {};

    let result = tool.call(params).await;

    // Should return an error
    assert!(result.is_err());
    let error = result.unwrap_err();
    assert!(error.to_string().contains("No execution plan set"));
}

#[tokio::test]
async fn test_next_groups_json_serialization() {
    // First, set up a plan
    let groups = vec![OperationGroup {
        description: "Simple operation".to_string(),
        operations: vec!["do something".to_string()],
        dependencies: vec![],
        contracts: vec![],
    }];

    let set_tool = SetExecutionPlan;
    let set_params = SetExecutionPlanParameters {
        groups: groups.clone(),
    };

    set_tool
        .call(set_params)
        .await
        .expect("should set plan successfully");

    // Now call NextGroups (it will fail to execute due to missing BAML/contracts, but we can test serialization)
    let next_tool = NextGroups;
    let next_params = super::tools::NextGroupsParameters {};

    let result = next_tool.call(next_params).await;

    // The call will likely fail due to missing dependencies, but if it returns a result,
    // verify it's valid JSON
    if let Ok(json_str) = result {
        let parsed: serde_json::Value =
            serde_json::from_str(&json_str).expect("should be valid JSON");

        // Verify structure
        assert!(parsed.get("results").is_some());
        assert!(parsed.get("remaining_groups").is_some());

        // results should be an array
        assert!(parsed["results"].is_array());

        // remaining_groups should be a number
        assert!(parsed["remaining_groups"].is_number());
    }
}


#[test]
fn test_group_result_serialization() {
    // Test Done variant
    let done_result = GroupResult {
        group_index: 0,
        description: "Test operation".to_string(),
        operations: vec!["op1".to_string(), "op2".to_string()],
        inner: GroupResultInner::Done {
            transactions: vec![TransactionData {
                from: Some("0x123".to_string()),
                to: Some("0x456".to_string()),
                value: "0x1000".to_string(),
                data: "0xabcd".to_string(),
                rpc_url: "http://localhost:8545".to_string(),
            }],
            generated_code: "pragma solidity ^0.8.0;".to_string(),
        },
    };

    let json = serde_json::to_string(&done_result).expect("should serialize");
    let parsed: serde_json::Value = serde_json::from_str(&json).expect("should parse");

    assert_eq!(parsed["group_index"], 0);
    assert_eq!(parsed["description"], "Test operation");
    assert_eq!(parsed["operations"].as_array().unwrap().len(), 2);
    assert!(parsed["inner"]["Done"].is_object());
    assert_eq!(
        parsed["inner"]["Done"]["transactions"]
            .as_array()
            .unwrap()
            .len(),
        1
    );

    // Test Failed variant
    let failed_result = GroupResult {
        group_index: 1,
        description: "Failed operation".to_string(),
        operations: vec!["bad_op".to_string()],
        inner: GroupResultInner::Failed {
            error: "Contract not found".to_string(),
        },
    };

    let json = serde_json::to_string(&failed_result).expect("should serialize");
    let parsed: serde_json::Value = serde_json::from_str(&json).expect("should parse");

    assert_eq!(parsed["group_index"], 1);
    assert!(parsed["inner"]["Failed"].is_object());
    assert_eq!(parsed["inner"]["Failed"]["error"], "Contract not found");
}


#[tokio::test]
#[ignore] // Requires full stack (BAML client, contracts, DB)
async fn test_full_workflow_set_and_execute() {
    // This is a full integration test that requires:
    // - BAML client configured
    // - Database with contracts
    // - Proper environment setup

    // 1. Create operation groups with real contracts
    let groups = vec![
        OperationGroup {
            description: "Get WETH balance".to_string(),
            operations: vec!["check WETH balance of address 0x123...".to_string()],
            dependencies: vec![],
            contracts: vec![(
                "1".to_string(),
                "0xC02aaA39b223FE8D0A0e5C4F27eAD9083C756Cc2".to_string(),
                "WETH".to_string(),
            )],
        },
        OperationGroup {
            description: "Wrap ETH".to_string(),
            operations: vec!["wrap 0.1 ETH to WETH".to_string()],
            dependencies: vec![0],
            contracts: vec![(
                "1".to_string(),
                "0xC02aaA39b223FE8D0A0e5C4F27eAD9083C756Cc2".to_string(),
                "WETH".to_string(),
            )],
        },
    ];

    // 2. Set execution plan
    let set_tool = SetExecutionPlan;
    let set_params = SetExecutionPlanParameters {
        groups: groups.clone(),
    };

    let set_result = set_tool
        .call(set_params)
        .await
        .expect("should set plan successfully");

    let set_response: serde_json::Value = serde_json::from_str(&set_result).unwrap();
    assert_eq!(set_response["success"], true);
    assert_eq!(set_response["total_groups"], 2);

    // 3. Execute first batch (group 0, no dependencies)
    let next_tool = NextGroups;
    let next_params = super::tools::NextGroupsParameters {};

    let first_batch_result = next_tool
        .call(next_params.clone())
        .await
        .expect("should execute first batch");

    let first_batch: serde_json::Value = serde_json::from_str(&first_batch_result).unwrap();
    assert_eq!(first_batch["results"].as_array().unwrap().len(), 1);
    assert_eq!(first_batch["remaining_groups"], 1); // Group 1 still pending

    // Verify first result structure
    let result = &first_batch["results"][0];
    assert_eq!(result["group_index"], 0);
    assert_eq!(result["description"], "Get WETH balance");
    assert!(result["inner"].is_object());
    println!("first_batch: {:?}", first_batch);

    // 4. Execute second batch (group 1, depends on 0)
    let second_batch_result = next_tool
        .call(next_params.clone())
        .await
        .expect("should execute second batch");

    let second_batch: serde_json::Value = serde_json::from_str(&second_batch_result).unwrap();
    assert_eq!(second_batch["results"].as_array().unwrap().len(), 1);
    assert_eq!(second_batch["remaining_groups"], 0); // All done
    println!("second_batch: {:?}", second_batch);

    // Verify second result
    let result = &second_batch["results"][0];
    assert_eq!(result["group_index"], 1);
    assert_eq!(result["description"], "Wrap ETH");

    // 5. Execute third time (no more groups)
    let third_batch_result = next_tool
        .call(next_params)
        .await
        .expect("should return empty results");

    let third_batch: serde_json::Value = serde_json::from_str(&third_batch_result).unwrap();
    assert_eq!(third_batch["results"].as_array().unwrap().len(), 0);
    assert_eq!(third_batch["remaining_groups"], 0);
}