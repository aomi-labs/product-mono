// Type declarations for @aomi-labs packages when not installed via npm.
// These are minimal declarations - full types come from the actual packages when installed.

declare module "@aomi-labs/react" {
  export interface UserState {
    address?: string;
    chainId?: number;
    isConnected: boolean;
    ensName?: string;
  }

  export interface UserConfig {
    user: UserState;
    setUser: (data: Partial<UserState>) => void;
  }

  // Re-export other types as needed
  export * from "@aomi-labs/widget-lib";
}
