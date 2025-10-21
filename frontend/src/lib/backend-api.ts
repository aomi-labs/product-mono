import { BackendReadiness } from "./types";

export interface BackendMessagePayload {
  sender?: string;
  content?: string;
  timestamp?: string;
  is_streaming?: boolean;
}

export interface BackendReadinessPayload {
  phase?: unknown;
  detail?: unknown;
  message?: unknown;
}

export interface BackendStatePayload {
  messages?: BackendMessagePayload[] | null;
  isTyping?: boolean;
  is_typing?: boolean;
  isProcessing?: boolean;
  is_processing?: boolean;
  pending_wallet_tx?: string | null;
  readiness?: BackendReadinessPayload | null;
  missingApiKey?: boolean | string;
  missing_api_key?: boolean | string;
  isLoading?: boolean | string;
  is_loading?: boolean | string;
  isConnectingMcp?: boolean | string;
  is_connecting_mcp?: boolean | string;
}

export type BackendSystemResponse = BackendStatePayload;

async function postState(
  backendUrl: string,
  path: string,
  payload: Record<string, unknown>
): Promise<BackendStatePayload> {
  const response = await fetch(`${backendUrl}${path}`, {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify(payload),
  });

  if (!response.ok) {
    throw new Error(`HTTP ${response.status}: ${response.statusText}`);
  }

  return (await response.json()) as BackendStatePayload;
}

export class BackendApi {
  constructor(private readonly backendUrl: string) {}

  async fetchState(sessionId: string): Promise<BackendStatePayload> {
    const response = await fetch(`${this.backendUrl}/api/state?session_id=${encodeURIComponent(sessionId)}`);

    if (!response.ok) {
      throw new Error(`HTTP ${response.status}: ${response.statusText}`);
    }

    return (await response.json()) as BackendStatePayload;
  }

  async postChatMessage(sessionId: string, message: string): Promise<BackendStatePayload> {
    return postState(this.backendUrl, "/api/chat", { message, session_id: sessionId });
  }

  async postSystemMessage(sessionId: string, message: string): Promise<BackendStatePayload> {
    return postState(this.backendUrl, "/api/system", { message, session_id: sessionId });
  }

  async postInterrupt(sessionId: string): Promise<BackendStatePayload> {
    return postState(this.backendUrl, "/api/interrupt", { session_id: sessionId });
  }

  async postMcpCommand(
    sessionId: string,
    command: string,
    args: Record<string, unknown>
  ): Promise<BackendStatePayload> {
    return postState(this.backendUrl, "/api/mcp-command", {
      command,
      args,
      session_id: sessionId,
    });
  }
}

export function normaliseReadiness(payload?: BackendReadinessPayload | null): BackendReadiness | null {
  if (!payload || typeof payload.phase !== "string") {
    return null;
  }

  const detailRaw = typeof payload.detail === "string" && payload.detail.trim().length > 0
    ? payload.detail
    : typeof payload.message === "string" && payload.message.trim().length > 0
      ? payload.message
      : undefined;

  const phase = payload.phase as BackendReadiness["phase"];
  switch (phase) {
    case "connecting_mcp":
    case "validating_anthropic":
    case "ready":
    case "missing_api_key":
    case "error":
      return { phase, detail: detailRaw };
    default:
      return { phase: "error", detail: detailRaw };
  }
}
