# StorageHandler Implementation

This module implements a `StorageHandler` that mimics L2Beat's discovery system for reading smart contract storage slots, using proper Alloy types for enhanced type safety.

## Features

- **Type-Safe Values**: Uses `HandlerValue` enum with proper Alloy types (`Address`, `U256`, `Bytes`, `u8`)
- **Direct Storage Access**: Read specific storage slots by number
- **Dynamic Slot Computation**: Compute storage slots using keccak256 hashing for mappings
- **Reference Resolution**: Use values from other handlers to compute storage slots
- **Automatic Type Conversion**: Convert raw storage values to proper Ethereum types
- **Dependency Tracking**: Automatically track dependencies between handlers

## Usage Examples

### Basic Storage Reading

```rust
use forge_mcp::discovery::handler::{StorageHandler, StorageSlot, SlotValue, StorageReturnType};
use alloy_primitives::U256;

// Read owner address from storage slot 0 - returns HandlerValue::Address(Address)
let owner_handler = StorageHandler::new(
    "owner".to_string(),
    StorageSlot {
        slot: SlotValue::Direct(U256::from(0)),
        offset: None,
        return_type: Some(StorageReturnType::Address),
    },
);

// Read a boolean flag from storage slot 1 - returns HandlerValue::Uint8(u8)
let paused_handler = StorageHandler::new(
    "paused".to_string(),
    StorageSlot {
        slot: SlotValue::Direct(U256::from(1)),
        offset: Some(0), // First byte of the slot
        return_type: Some(StorageReturnType::Uint8),
    },
);
```

### Dynamic Slot Computation

```rust
// Read a mapping value: mapping(address => uint256) balances
// Storage slot = keccak256(address || slot_number) - returns HandlerValue::Number(U256)
let balance_handler = StorageHandler::new(
    "balance".to_string(),
    StorageSlot {
        slot: SlotValue::Array(vec![
            SlotValue::Reference("{{ userAddress }}".to_string()),
            SlotValue::Direct(U256::from(2)), // balances mapping is at slot 2
        ]),
        offset: None,
        return_type: Some(StorageReturnType::Number),
    },
);
```

### Reference Resolution

```rust
// Use the result from another handler to compute storage slot - returns HandlerValue::Bytes(Bytes)
let admin_role_handler = StorageHandler::new(
    "adminRole".to_string(),
    StorageSlot {
        slot: SlotValue::Array(vec![
            SlotValue::Reference("{{ DEFAULT_ADMIN_ROLE }}".to_string()),
            SlotValue::Direct(U256::from(5)), // roleMembers mapping slot
        ]),
        offset: None,
        return_type: Some(StorageReturnType::Bytes),
    },
);
```

## Storage Slot Computation

The handler supports several storage slot computation methods:

1. **Direct**: Use the slot number directly
2. **Array**: Compute `keccak256(value1 || value2 || ...)` for complex mappings
3. **Reference**: Use values from previous handler results

## Return Types

The `HandlerValue` enum provides type-safe return values:

- `HandlerValue::Address(Address)`: Ethereum address from the last 20 bytes
- `HandlerValue::Number(U256)`: Full 256-bit number
- `HandlerValue::Bytes(Bytes)`: Raw bytes from storage
- `HandlerValue::Uint8(u8)`: Single byte value (with optional offset)

### Type Safety Benefits

Unlike L2Beat's string-based approach, our implementation provides:
- **Compile-time type checking**: No runtime errors from incorrect type assumptions
- **Direct Alloy integration**: Native compatibility with Ethereum tooling
- **Memory efficiency**: No JSON parsing overhead for internal operations
- **Rich type methods**: Access to all Alloy primitive methods and traits

## Dependencies

The handler automatically extracts dependencies from reference strings like `{{ fieldName }}` and ensures proper execution order.

## Integration with L2Beat

This implementation follows L2Beat's handler architecture:
- Same slot computation logic using keccak256
- Compatible reference resolution system
- Similar return type conversion
- Equivalent dependency tracking

The main difference is that this is implemented in Rust for better performance and type safety, while L2Beat's implementation is in TypeScript.