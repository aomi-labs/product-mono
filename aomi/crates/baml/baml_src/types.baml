// Event operation for add/remove semantics
class EventOperation {
  event_signature string @description("Full event signature (e.g., 'ValidatorAdded(uint256 indexed chainId,address indexed validator)')")
  where string[]? @description("Optional filter expressed as ['operator', '#field', 'value'] (e.g., ['=', '#chainId', '325'])")
}

// Event handler configuration matching Rust EventHandler
class EventHandlerConfig {
  event_signature string? @description("Primary event signature when a single event drives updates. Null when using explicit add/remove operations only.")
  select_field string @description("Event parameter that represents the value being tracked (e.g., 'validator', 'account')")
  return_type string @description("Solidity type for the selected field (e.g., 'address', 'uint256')")
  add EventOperation? @description("Event operation that inserts/updates a value in the tracked set")
  remove EventOperation? @description("Event operation that removes a value from the tracked set")
}