use serde::{Deserialize, Serialize};

/// Result of executing an operation group
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct GroupResult {
    pub group_index: usize,
    pub description: String,
    pub operations: Vec<String>,
    pub inner: GroupResultInner,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum GroupResultInner {
    Done {
        transactions: Vec<TransactionData>,
        generated_code: String,
    },
    Failed {
        error: String,
    },
}

/// Transaction data ready to be sent to wallet
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct TransactionData {
    pub from: Option<String>,
    pub to: Option<String>,
    pub value: String,
    pub data: String,
    pub rpc_url: String,
}
