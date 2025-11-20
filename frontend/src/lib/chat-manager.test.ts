/// <reference types="jest" />
// Test for ChatManager session functionality

import { ChatManager } from './chat-manager';
import { ConnectionStatus, ChatManagerState } from './types';

type EventHandler<TEvent extends Event> = (this: EventSource, ev: TEvent) => void;

class MockEventSourceImpl {
  static readonly CONNECTING = 0;
  static readonly OPEN = 1;
  static readonly CLOSED = 2;
  public onopen: EventHandler<Event> | null = null;
  public onmessage: EventHandler<MessageEvent> | null = null;
  public onerror: EventHandler<Event> | null = null;
  public close = jest.fn<void, []>();
  public addEventListener = jest.fn<void, [string, EventListenerOrEventListenerObject | undefined]>();
  public removeEventListener = jest.fn<void, [string, EventListenerOrEventListenerObject | undefined]>();

  constructor(private readonly url: string) {
    void this.url;
  }
}

type EventSourceConstructorMock = jest.Mock<MockEventSourceImpl, [string]> & {
  CONNECTING: number;
  OPEN: number;
  CLOSED: number;
};

const EventSourceMock = Object.assign(
  jest.fn((url: string) => new MockEventSourceImpl(url)),
  {
    CONNECTING: MockEventSourceImpl.CONNECTING,
    OPEN: MockEventSourceImpl.OPEN,
    CLOSED: MockEventSourceImpl.CLOSED,
  }
) as EventSourceConstructorMock;

globalThis.EventSource = EventSourceMock as unknown as typeof EventSource;

const fetchMock = jest.fn() as jest.MockedFunction<typeof fetch>;
globalThis.fetch = fetchMock;

const makeFetchResponse = <T>(body: T) =>
  ({
    ok: true,
    status: 200,
    statusText: 'OK',
    headers: new Headers(),
    redirected: false,
    type: 'basic',
    url: '',
    clone: function () {
      return this;
    },
    json: async () => body,
    text: async () => JSON.stringify(body),
    arrayBuffer: async () => new ArrayBuffer(0),
    blob: async () => new Blob(),
    formData: async () => new FormData(),
    body: null,
    bodyUsed: false
  } as Response);

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
    const managerInternals = manager as unknown as { state: ChatManagerState };
    managerInternals.state.connectionStatus = ConnectionStatus.CONNECTED;

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

    expect(EventSourceMock).toHaveBeenCalledWith(
      `http://localhost:8080/api/chat/stream?session_id=${customSessionId}`
    );
  });

  test('should include session ID in chat API call', async () => {
    const customSessionId = 'test-session-456';
    const manager = new ChatManager({ sessionId: customSessionId });

    // Mock connected state and successful response
    const managerInternals = manager as unknown as { state: ChatManagerState };
    managerInternals.state.connectionStatus = ConnectionStatus.CONNECTED;
    fetchMock.mockResolvedValueOnce(makeFetchResponse({ messages: [], is_processing: false }));

    await manager.postMessageToBackend('Hello');

    expect(fetchMock).toHaveBeenCalledWith(
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

    fetchMock.mockResolvedValueOnce(makeFetchResponse({ messages: [] }));

    await manager.interrupt();

    expect(fetchMock).toHaveBeenCalledWith(
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

  test('should include session ID in transaction result', async () => {
    const customSessionId = 'test-session-tx';
    const manager = new ChatManager({ sessionId: customSessionId });

    fetchMock.mockResolvedValueOnce(makeFetchResponse({
      res: {
        sender: 'system',
        content: 'Transaction sent: 0x123',
        is_streaming: false,
        timestamp: '2024-08-07T12:00:00Z'
      }
    }));

    await manager.sendTransactionResult(true, '0x123');

    expect(fetchMock).toHaveBeenCalledWith(
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

  test('should append system response and clear pending tx', async () => {
    const manager = new ChatManager({ sessionId: 'pending-session' });
    const managerInternals = manager as unknown as { state: ChatManagerState };

    managerInternals.state.pendingWalletTx = {
      to: '0xabc',
      value: '0',
      data: '0x',
      description: 'test tx',
      timestamp: 'now',
    };

    fetchMock.mockResolvedValueOnce(makeFetchResponse({
      res: {
        sender: 'system',
        content: 'Transaction rejected by user: insufficient funds',
        is_streaming: false,
        timestamp: '2024-08-07T12:00:00Z'
      }
    }));

    await manager.sendTransactionResult(false, undefined, 'insufficient funds');

    const lastMessage = managerInternals.state.messages[managerInternals.state.messages.length - 1];
    expect(lastMessage.content).toBe('Transaction rejected by user: insufficient funds');
    expect(managerInternals.state.pendingWalletTx).toBeUndefined();
  });
});
