Absolutely! Here’s how your design fits together and what you should do next:

---

## **How It Works**

- **On the server (Rust, forge-mcp):**
  - Each contract field is handled by a `Handler` implementation.
  - Each handler, when executed, returns a `HandlerResult` with a `field` and a `value` (as `serde_json::Value`).
  - All these results are collected into a map:  
    `HashMap<String, HandlerResult>`, where the key is the field name.

- **On the client (TypeScript or Rust):**
  - You want to build a `ContractCtx` struct:
    ```rust
    struct ContractCtx {
        name: String,
        root: Address,
        retrievals: Map<String, String>, // field name -> value (as string)
        source: String,
    }
    ```
  - The `retrievals` field should be populated with the field names and their corresponding values (converted to strings).

---

## **How to Achieve This**

1. **Server Side:**
   - After running all handlers for a contract, collect their results:
     ```rust
     let mut retrievals = HashMap::new();
     for (field, result) in handler_results {
         if let Some(value) = &result.value {
             retrievals.insert(field.clone(), value.to_string());
         }
     }
     ```
   - Serialize this map (and the other fields) and send to the client.

2. **Client Side:**
   - Receive the contract context (as JSON or via your protocol).
   - Construct the `ContractCtx` struct, filling `retrievals` with the field-value pairs.

---

## **Summary Table**

| HandlerResult (server)         | ContractCtx (client)         |
|------------------------------- |----------------------------- |
| field: String                  | retrievals key (field name)  |
| value: Option<serde_json::Value> | retrievals value (as string) |
| ...                            | ...                          |

---

## **Next Steps**

- **Server:** Ensure you collect all handler results and convert their values to strings for the `retrievals` map.
- **Client:** Parse the received data and fill your `ContractCtx` struct accordingly.

---

**Let me know if you want a code example for the server-side aggregation or the client-side struct construction!**
