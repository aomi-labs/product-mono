/// <reference types="jest" />
// Test for ChatManager session functionality

import { ChatManager } from './chat-manager';
import { ConnectionStatus } from './types';

// Mock EventSource with required static props and instance methods
class MockEventSource {
  static readonly CONNECTING = 0;
  static readonly OPEN = 1;
  static readonly CLOSED = 2;
  onopen: ((this: EventSource, ev: Event) => any) | null = null;
  onmessage: ((this: EventSource, ev: MessageEvent) => any) | null = null;
  onerror: ((this: EventSource, ev: Event) => any) | null = null;
  close = jest.fn();
  addEventListener = jest.fn();
  removeEventListener = jest.fn();
  constructor(_url: string) {}
}
(global as any).EventSource = MockEventSource as unknown as typeof EventSource;

// Mock fetch
global.fetch = jest.fn();

describe('ChatManager Session Management', () => {
  afterEach(() => {
    jest.clearAllMocks();
  });

  test('should generate unique session IDs', () => {
    const manager1 = new ChatManager();
    const manager2 = new ChatManager();

    const sessionId1 = manager1.getSessionId();
    const sessionId2 = manager2.getSessionId();

    expect(sessionId1).toBeDefined();
    expect(sessionId2).toBeDefined();
    expect(sessionId1).not.toBe(sessionId2);
    expect(sessionId1).toMatch(/^[0-9a-f]{8}-[0-9a-f]{4}-4[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/i);
  });

  test('should use provided session ID', () => {
    const customSessionId = 'custom-session-id';
    const manager = new ChatManager({ sessionId: customSessionId });

    expect(manager.getSessionId()).toBe(customSessionId);
  });

  test('should set session ID and reconnect if connected', () => {
    const manager = new ChatManager();
    const connectSpy = jest.spyOn(manager, 'connectSSE');
    const originalSessionId = manager.getSessionId();

    // Mock connected state
    (manager as any).state.connectionStatus = ConnectionStatus.CONNECTED;

    const newSessionId = 'new-session-id';
    manager.setSessionId(newSessionId);

    expect(manager.getSessionId()).toBe(newSessionId);
    expect(manager.getSessionId()).not.toBe(originalSessionId);
    expect(connectSpy).toHaveBeenCalled(); // Should reconnect with new session
  });

  test('should include session ID in SSE connection', () => {
    const customSessionId = 'test-session-123';
    const manager = new ChatManager({ sessionId: customSessionId });

    manager.connectSSE();

    expect(global.EventSource).toHaveBeenCalledWith(
      `http://localhost:8080/api/chat/stream?session_id=${customSessionId}`
    );
  });

  test('should include session ID in chat API call', async () => {
    const customSessionId = 'test-session-456';
    const manager = new ChatManager({ sessionId: customSessionId });

    // Mock connected state and successful response
    (manager as any).state.connectionStatus = ConnectionStatus.CONNECTED;
    (manager as any).state.readiness = { phase: 'ready' };
    (global.fetch as jest.Mock).mockResolvedValueOnce({
      ok: true,
      json: async () => ({ messages: [], is_processing: false, readiness: { phase: 'ready' } })
    });

    await manager.postMessageToBackend('Hello');

    expect(global.fetch).toHaveBeenCalledWith(
      'http://localhost:8080/api/chat',
      {
        method: 'POST',
        headers: {
          'Content-Type': 'application/json',
        },
        body: JSON.stringify({
          message: 'Hello',
          session_id: customSessionId
        }),
      }
    );
  });

  test('should include session ID in interrupt API call', async () => {
    const customSessionId = 'test-session-789';
    const manager = new ChatManager({ sessionId: customSessionId });

    (global.fetch as jest.Mock).mockResolvedValueOnce({
      ok: true,
      json: async () => ({ messages: [] })
    });

    await manager.interrupt();

    expect(global.fetch).toHaveBeenCalledWith(
      'http://localhost:8080/api/interrupt',
      {
        method: 'POST',
        headers: {
          'Content-Type': 'application/json',
        },
        body: JSON.stringify({
          session_id: customSessionId
        }),
      }
    );
  });

  test('should include session ID in network switch request', async () => {
    const customSessionId = 'test-session-network';
    const manager = new ChatManager({ sessionId: customSessionId });

    (global.fetch as jest.Mock).mockResolvedValueOnce({
      ok: true,
      json: async () => ({ messages: [] })
    });

    await manager.sendNetworkSwitchRequest('mainnet');

    expect(global.fetch).toHaveBeenCalledWith(
      'http://localhost:8080/api/system',
      {
        method: 'POST',
        headers: {
          'Content-Type': 'application/json',
        },
        body: JSON.stringify({
          message: "Dectected user's wallet connected to mainnet network",
          session_id: customSessionId
        }),
      }
    );
  });

  test('should include session ID in transaction result', async () => {
    const customSessionId = 'test-session-tx';
    const manager = new ChatManager({ sessionId: customSessionId });

    (global.fetch as jest.Mock).mockResolvedValueOnce({
      ok: true,
      json: async () => ({ messages: [] })
    });

    await manager.sendTransactionResult(true, '0x123');

    expect(global.fetch).toHaveBeenCalledWith(
      'http://localhost:8080/api/system',
      {
        method: 'POST',
        headers: {
          'Content-Type': 'application/json',
        },
        body: JSON.stringify({
          message: 'Transaction sent: 0x123',
          session_id: customSessionId
        }),
      }
    );
  });
});
