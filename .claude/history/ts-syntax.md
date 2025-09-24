# TypeScript Syntax Guide

## Table of Contents
1. [React Hooks](#react-hooks)
2. [Interfaces](#interfaces)
3. [Classes](#classes)
4. [Function Types](#function-types)
5. [Generic Types](#generic-types)
6. [Type Utilities](#type-utilities)
7. [Destructuring Patterns](#destructuring-patterns)

## React Hooks

### useState Hook
```typescript
// Basic useState
const [count, setCount] = useState<number>(0);
const [name, setName] = useState<string>('');

// Complex state object
const [walletState, setWalletState] = useState<WalletState>({
  isConnected: false,
  address: undefined,
  chainId: undefined,
  networkName: 'testnet'
});

// Updating state
setWalletState(prev => ({ ...prev, isConnected: true }));
```

### useEffect Hook
```typescript
// Basic syntax
useEffect(effect: EffectCallback, deps?: DependencyList): void

// Example: Watch for transaction errors
useEffect(() => {
  if (isSendError && sendError) {
    handleTransactionError(sendError);
  }
}, [isSendError, sendError]);

// Different dependency scenarios
useEffect(() => {
  console.log('Runs every render');
}); // No dependency array

useEffect(() => {
  console.log('Runs once on mount');
}, []); // Empty dependency array

useEffect(() => {
  console.log('Runs when count changes');
}, [count]); // Specific dependencies
```

### Custom Hooks (like useSendTransaction)
```typescript
// Hook definition pattern
export function useSendTransaction<
  config extends Config = ResolvedRegister['config'],
  context = unknown,
>(
  parameters: UseSendTransactionParameters<config, context> = {},
): UseSendTransactionReturnType<config, context> {
  // Implementation...
}

// Usage with destructuring
const { 
  data: hash, 
  sendTransaction, 
  error: sendError, 
  isError: isSendError 
} = useSendTransaction();
```

## Interfaces

### Basic Interface
```typescript
// Define structure/blueprint
export interface WalletManagerConfig {
  backendUrl: string;
}

export interface WalletManagerEventHandlers {
  onConnectionChange: (isConnected: boolean, address?: string) => void;
  onChainChange: (chainId: number, networkName: string) => void;
  onError: (error: Error) => void;
}

export interface WalletState {
  isConnected: boolean;
  address?: string;
  chainId?: number;
  networkName: string;
  hasPromptedNetworkSwitch: boolean;
}
```

### Using Interfaces
```typescript
// Create object that matches interface shape
const config: WalletManagerConfig = {
  backendUrl: 'http://localhost:8080'
};

// Interface vs Object distinction
// Interface = Blueprint (compile-time only)
interface Car {
  brand: string;
  start(): void;
}

// Object = Actual implementation
const myCar = {
  brand: "Tesla",
  start(): void {
    console.log("Tesla started!");
  }
};
```

## Classes

### Basic Class with Interface Implementation
```typescript
export class WalletManager {
  // Private properties
  private config: WalletManagerConfig;
  private onConnectionChange: (isConnected: boolean, address?: string) => void;
  private onChainChange: (chainId: number, networkName: string) => void;
  private onError: (error: Error) => void;
  private state: WalletState;

  // Constructor with dependency injection
  constructor(
    config: WalletManagerConfig, 
    eventHandlers: Partial<WalletManagerEventHandlers> = {}
  ) {
    this.config = config;
    
    // Event handlers with fallbacks
    this.onConnectionChange = eventHandlers.onConnectionChange || (() => {});
    this.onChainChange = eventHandlers.onChainChange || (() => {});
    this.onError = eventHandlers.onError || (() => {});
    
    // Initialize state (object that matches interface)
    this.state = {
      isConnected: false,
      networkName: 'testnet',
      hasPromptedNetworkSwitch: false,
    };
  }

  // Methods
  async handleConnect(address: string, chainId: number): Promise<void> {
    this.state = {
      ...this.state,
      isConnected: true,
      address,
      chainId,
      networkName: this.getChainIdToNetworkName(chainId),
    };
    
    this.onConnectionChange(true, address);
  }
}
```

### Using Classes
```typescript
// Instantiate with config and event handlers
const walletMgr = new WalletManager(
  { backendUrl: 'http://localhost:8080' }, // config
  { // eventHandlers
    onConnectionChange: (isConnected, address) => {
      setWalletState(prev => ({ ...prev, isConnected, address }));
    },
    onChainChange: (chainId, networkName) => {
      setWalletState(prev => ({ ...prev, chainId, networkName }));
    },
    onError: (error) => {
      console.error('Wallet error:', error);
    },
  }
);
```

## Function Types

### Generic Function Type Definition
```typescript
// Pattern: export type FunctionName<...> = <...>(...) => ReturnType
export type SendTransactionMutate<config extends Config, context = unknown> = <
  chainId extends config['chains'][number]['id'],
>(
  variables: SendTransactionVariables<config, chainId>,
  options?: MutateOptions<SendTransactionData, SendTransactionErrorType> | undefined,
) => void

// Breaking it down:
// 1. Outer generics: <config, context> - for the type definition
// 2. Inner generics: <chainId> - for the function itself  
// 3. Parameters: (variables, options?)
// 4. Return type: => void
```

### Function Type Usage
```typescript
// This function matches the SendTransactionMutate type
const sendTransaction: SendTransactionMutate = (variables, options) => {
  // Implementation that sends blockchain transaction
  console.log('Sending transaction:', variables);
};

// Usage
sendTransaction({
  to: "0x123...",
  value: BigInt("1000000000000000000"),
  data: "0x..."
}, {
  onSuccess: (hash) => console.log("Success!", hash),
  onError: (error) => console.log("Error!", error)
});
```

## Generic Types

### Complex Generic Return Type
```typescript
// Pattern: Compute<UseMutationReturnType<...> & {}>
export type UseSendTransactionReturnType<
  config extends Config = Config,
  context = unknown,
> = Compute<
  UseMutationReturnType<
    SendTransactionData,
    SendTransactionErrorType,
    SendTransactionVariables<config, config['chains'][number]['id']>,
    context
  > & {
    sendTransaction: SendTransactionMutate<config, context>
    sendTransactionAsync: SendTransactionMutateAsync<config, context>
  }
>

// This combines:
// 1. React Query's UseMutationReturnType (gives data, error, isError, etc.)
// 2. Custom blockchain functions (sendTransaction, sendTransactionAsync)
// 3. Compute utility (simplifies the complex type)
```

### Generic Constraints
```typescript
// Generic with constraints
export type MyType<T extends string | number> = {
  value: T;
  process(): T;
};

// Usage
const stringType: MyType<string> = {
  value: "hello",
  process: () => "processed"
};

const numberType: MyType<number> = {
  value: 42,
  process: () => 84
};
```

## Type Utilities

### Compute Utility
```typescript
// Compute<Type> - Simplifies complex types
type ComplexType = Compute<
  { a: string } & 
  { b: number } & 
  { c: boolean }
>;

// Results in:
// {
//   a: string;
//   b: number; 
//   c: boolean;
// }
```

### Intersection Types (&)
```typescript
// Combine multiple types
type Combined = TypeA & TypeB & {
  additional: string;
};

// Example: React Query + Custom blockchain functions
type WalletHookReturn = UseMutationReturnType<...> & {
  sendTransaction: Function;
  sendTransactionAsync: Function;
};
```

### Partial Utility
```typescript
// Makes all properties optional
type PartialEventHandlers = Partial<WalletManagerEventHandlers>;

// Equivalent to:
type PartialEventHandlers = {
  onConnectionChange?: (isConnected: boolean, address?: string) => void;
  onChainChange?: (chainId: number, networkName: string) => void;
  onError?: (error: Error) => void;
};
```

## Destructuring Patterns

### Basic Destructuring
```typescript
// Extract values from object
const { name, age } = person;

// With renaming
const { firstName: name, userAge: age } = person;
```

### Hook Destructuring with Renaming
```typescript
// Rename fields from hook return
const { 
  data: hash,           // data -> hash
  error: sendError,     // error -> sendError  
  isError: isSendError, // isError -> isSendError
  sendTransaction       // keep same name
} = useSendTransaction();

// Now use the renamed variables
console.log(hash);        // Instead of data
console.log(sendError);   // Instead of error
console.log(isSendError); // Instead of isError
```

### State Destructuring
```typescript
// Multiple state values
const [count, setCount] = useState(0);
const [user, setUser] = useState(null);
const [loading, setLoading] = useState(false);

// Object state
const [walletState, setWalletState] = useState({
  isConnected: false,
  address: undefined,
  chainId: undefined
});

// Update object state
setWalletState(prev => ({ 
  ...prev, 
  isConnected: true 
}));
```

## Key Patterns Summary

### 1. Hook Pattern
```typescript
const { data, error, isLoading, mutate } = useCustomHook();
```

### 2. Interface + Class Pattern
```typescript
interface Config { /* ... */ }
class Manager {
  constructor(config: Config, handlers: Partial<Handlers>) { /* ... */ }
}
```

### 3. Generic Function Type Pattern
```typescript
export type MyFunction<A, B> = <C>(param: C) => A & B;
```

### 4. Complex Return Type Pattern
```typescript
export type MyReturnType<T> = Compute<BaseType<T> & { custom: Function }>;
```

### 5. Destructuring with Renaming Pattern
```typescript
const { originalName: newName, keepSame } = someObject;
```

## Common TypeScript Concepts

- **Interfaces**: Blueprints for object shapes (compile-time only)
- **Classes**: Templates for creating objects with behavior
- **Generics**: Type parameters that make code reusable
- **Union Types**: `string | number` (either string OR number)
- **Intersection Types**: `TypeA & TypeB` (combine types)
- **Optional Properties**: `property?: string` (property may not exist)
- **Type Guards**: Runtime checks to narrow types
- **Utility Types**: `Partial<T>`, `Pick<T, K>`, `Omit<T, K>`, etc.
