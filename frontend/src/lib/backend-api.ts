export interface SessionMessage {
  sender?: string;
  content?: string;
  timestamp?: string;
  is_streaming?: boolean;
  tool_stream?: [string, string] | { topic?: unknown; content?: unknown } | null;
}

export interface SessionResponsePayload {
  messages?: SessionMessage[] | null;
  is_processing?: boolean;
  pending_wallet_tx?: string | null;
}

export interface SystemResponsePayload {
  res?: SessionMessage | null;
}

function toQueryString(payload: Record<string, unknown>): string {
  const params = new URLSearchParams();
  for (const [key, value] of Object.entries(payload)) {
    if (value === undefined || value === null) continue;
    params.set(key, String(value));
  }
  const qs = params.toString();
  return qs ? `?${qs}` : "";
}

async function postState<T>(
  backendUrl: string,
  path: string,
  payload: Record<string, unknown>
): Promise<T> {
  const query = toQueryString(payload);
  const response = await fetch(`${backendUrl}${path}${query}`, {
    method: "POST",
  });

  if (!response.ok) {
    throw new Error(`HTTP ${response.status}: ${response.statusText}`);
  }

  return (await response.json()) as T;
}

export class BackendApi {
  constructor(private readonly backendUrl: string) {}

  async fetchState(sessionId: string): Promise<SessionResponsePayload> {
    const response = await fetch(`${this.backendUrl}/api/state?session_id=${encodeURIComponent(sessionId)}`);

    if (!response.ok) {
      throw new Error(`HTTP ${response.status}: ${response.statusText}`);
    }

    return (await response.json()) as SessionResponsePayload;
  }

  async postChatMessage(sessionId: string, message: string): Promise<SessionResponsePayload> {
    return postState<SessionResponsePayload>(this.backendUrl, "/api/chat", { message, session_id: sessionId });
  }

  async postSystemMessage(sessionId: string, message: string): Promise<SystemResponsePayload> {
    return postState<SystemResponsePayload>(this.backendUrl, "/api/system", { message, session_id: sessionId });
  }

  async postInterrupt(sessionId: string): Promise<SessionResponsePayload> {
    return postState<SessionResponsePayload>(this.backendUrl, "/api/interrupt", { session_id: sessionId });
  }
}
