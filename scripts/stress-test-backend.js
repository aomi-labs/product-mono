#!/usr/bin/env node

const { randomUUID } = require('crypto');
const { EventEmitter } = require('node:events');
const process = require('process');

if (typeof fetch !== 'function') {
  throw new Error('Global fetch API is required. Use Node.js 18+ or enable experimental fetch support.');
}

if (typeof TextDecoder !== 'function') {
  throw new Error('Global TextDecoder API is required to parse SSE responses.');
}

const defaultConfig = {
  sessions: 20,
  rounds: 12,
  readinessTimeoutMs: 120000,
  roundTimeoutMs: 180000,
  postRequestTimeoutMs: 15000,
  messageDelayMs: 250,
  streamReconnectDelayMs: 1000,
};

function parseArgs(argv) {
  const options = {};

  for (let i = 0; i < argv.length; i += 1) {
    const arg = argv[i];

    if (arg === '-h' || arg === '--help') {
      options.help = true;
      continue;
    }

    if (!arg.startsWith('--')) {
      continue;
    }

    const eqIndex = arg.indexOf('=');
    if (eqIndex !== -1) {
      const key = arg.slice(2, eqIndex);
      const value = arg.slice(eqIndex + 1);
      options[key] = value;
      continue;
    }

    const key = arg.slice(2);
    const next = argv[i + 1];
    if (next && !next.startsWith('--')) {
      options[key] = next;
      i += 1;
    } else {
      options[key] = true;
    }
  }

  return options;
}

function toPositiveInt(value, fallback) {
  const number = Number.parseInt(value, 10);
  return Number.isFinite(number) && number > 0 ? number : fallback;
}

function toDurationMs(value, fallback) {
  const number = Number.parseInt(value, 10);
  return Number.isFinite(number) && number >= 0 ? number : fallback;
}

function normaliseUrl(url) {
  if (!url) {
    return url;
  }
  return url.endsWith('/') ? url.slice(0, -1) : url;
}

function buildConfig() {
  const args = parseArgs(process.argv.slice(2));

  if (args.help) {
    printHelp();
    process.exit(0);
  }

  const backendUrl = normaliseUrl(
    args.url || args.backendUrl || process.env.BACKEND_URL || 'https://api.aomi.dev',
  );

  return {
    backendUrl,
    sessions: toPositiveInt(args.sessions, defaultConfig.sessions),
    rounds: toPositiveInt(args.rounds, defaultConfig.rounds),
    readinessTimeoutMs: toDurationMs(args.readinessTimeoutMs, defaultConfig.readinessTimeoutMs),
    roundTimeoutMs: toDurationMs(args.roundTimeoutMs, defaultConfig.roundTimeoutMs),
    postRequestTimeoutMs: toDurationMs(args.postRequestTimeoutMs, defaultConfig.postRequestTimeoutMs),
    messageDelayMs: toDurationMs(args.messageDelayMs, defaultConfig.messageDelayMs),
    streamReconnectDelayMs: toDurationMs(
      args.streamReconnectDelayMs,
      defaultConfig.streamReconnectDelayMs,
    ),
    prompts: [
      'Give me a quick overview of current DeFi opportunities on Polygon.',
      'Suggest a strategy to test swaps from ETH to USDC using a forked testnet.',
      'List the steps to verify wallet balances before making a trade.',
      'Explain how to evaluate staking yields across popular protocols.',
      'Outline a safe approach for exploring Base network DeFi apps.',
      'Summarise the risks of automated liquidity provision strategies.',
    ],
  };
}

function printHelp() {
  const usage = `
Usage: node scripts/stress-test-backend.js [options]

Environment variables:
  BACKEND_URL                Backend base URL (default: https://api.aomi.dev)

Options:
  --url <string>                  Backend base URL (overrides BACKEND_URL)
  --sessions <number>             Number of concurrent chat sessions (default: ${defaultConfig.sessions})
  --rounds <number>               Chat rounds per session (default: ${defaultConfig.rounds})
  --readinessTimeoutMs <number>   Timeout for readiness phase per session (default: ${defaultConfig.readinessTimeoutMs})
  --roundTimeoutMs <number>       Timeout for each chat round to finish processing (default: ${defaultConfig.roundTimeoutMs})
  --postRequestTimeoutMs <number> Timeout for POST /api/chat (default: ${defaultConfig.postRequestTimeoutMs})
  --messageDelayMs <number>       Delay between rounds within a session (default: ${defaultConfig.messageDelayMs})
  --streamReconnectDelayMs <number> Delay before reconnecting to SSE stream (default: ${defaultConfig.streamReconnectDelayMs})
  --help                          Show this help message
`;
  console.log(usage.trim());
}

function delay(ms) {
  return new Promise((resolve) => {
    setTimeout(resolve, ms);
  });
}

function getMessages(state) {
  return Array.isArray(state?.messages) ? state.messages : [];
}

function createDeferred() {
  let settled = false;
  let resolveFn;
  let rejectFn;

  const promise = new Promise((resolve, reject) => {
    resolveFn = (value) => {
      if (!settled) {
        settled = true;
        resolve(value);
      }
    };
    rejectFn = (reason) => {
      if (!settled) {
        settled = true;
        reject(reason);
      }
    };
  });

  return {
    promise,
    resolve: resolveFn,
    reject: rejectFn,
    get settled() {
      return settled;
    },
  };
}

async function fetchJson(url, { timeoutMs, ...options }) {
  const controller = new AbortController();
  const timeout = setTimeout(() => controller.abort(), timeoutMs);

  try {
    const response = await fetch(url, {
      ...options,
      signal: controller.signal,
    });

    const text = await response.text();
    let data = null;

    if (text.length > 0) {
      try {
        data = JSON.parse(text);
      } catch (error) {
        const parsingError = new Error(`Failed to parse JSON response from ${url}: ${error.message}`);
        parsingError.cause = error;
        parsingError.responseBody = text.slice(0, 512);
        throw parsingError;
      }
    }

    if (!response.ok) {
      const statusError = new Error(`Request to ${url} failed with HTTP ${response.status} ${response.statusText}`);
      statusError.responseBody = data;
      statusError.status = response.status;
      throw statusError;
    }

    return data;
  } catch (error) {
    if (error.name === 'AbortError') {
      throw new Error(`Request to ${url} timed out after ${timeoutMs}ms`);
    }
    throw error;
  } finally {
    clearTimeout(timeout);
  }
}

async function postChat(config, sessionId, message) {
  const url = `${config.backendUrl}/api/chat`;
  return fetchJson(url, {
    timeoutMs: config.postRequestTimeoutMs,
    method: 'POST',
    headers: {
      'Content-Type': 'application/json',
    },
    body: JSON.stringify({
      message,
      session_id: sessionId,
    }),
  });
}

class SessionRunner {
  constructor(config, sessionIndex) {
    this.config = config;
    this.sessionIndex = sessionIndex;
    this.sessionId = randomUUID();
    this.sessionLabel = `session-${sessionIndex + 1}`;
    this.latestState = null;
    this.lastReadinessPhase = undefined;
    this.stopped = false;
    this.streamAbortController = null;
    this.streamPromise = null;
    this.streamReady = createDeferred();
    this.eventEmitter = new EventEmitter();
    this.eventEmitter.setMaxListeners(0);
  }

  startStream() {
    if (!this.streamPromise) {
      this.streamPromise = this.establishStreamLoop();
    }
    return this.streamPromise;
  }

  async establishStreamLoop() {
    const url = `${this.config.backendUrl}/api/chat/stream?session_id=${encodeURIComponent(this.sessionId)}`;

    while (!this.stopped) {
      const controller = new AbortController();
      this.streamAbortController = controller;

      try {
        const response = await fetch(url, {
          signal: controller.signal,
          headers: {
            Accept: 'text/event-stream',
          },
        });

        if (!response.ok) {
          throw new Error(`SSE request failed with HTTP ${response.status} ${response.statusText}`);
        }

        if (!response.body || typeof response.body.getReader !== 'function') {
          throw new Error('SSE response body is not a readable stream');
        }

        await this.consumeSse(response.body);

        if (!this.stopped) {
          throw new Error('SSE stream closed unexpectedly');
        }
      } catch (error) {
        if (this.stopped && error.name === 'AbortError') {
          break;
        }

        if (error.name !== 'AbortError') {
          console.error(`[${this.sessionLabel}] stream error: ${error.message}`);
        }

        this.eventEmitter.emit('stream-error', error);

        if (this.stopped) {
          break;
        }

        await delay(this.config.streamReconnectDelayMs || 1000);
      } finally {
        if (this.streamAbortController === controller) {
          this.streamAbortController = null;
        }
      }
    }
  }

  async consumeSse(stream) {
    const reader = stream.getReader();
    const decoder = new TextDecoder();
    let buffer = '';

    while (!this.stopped) {
      const { value, done } = await reader.read();

      if (done) {
        buffer += decoder.decode();
        buffer = this.processSseBuffer(buffer);
        break;
      }

      buffer += decoder.decode(value, { stream: true });
      buffer = this.processSseBuffer(buffer);
    }
  }

  processSseBuffer(buffer) {
    let workingBuffer = buffer;
    let eventBoundary = workingBuffer.indexOf('\n\n');

    while (eventBoundary !== -1) {
      const rawEvent = workingBuffer.slice(0, eventBoundary);
      workingBuffer = workingBuffer.slice(eventBoundary + 2);
      this.handleRawSseEvent(rawEvent);
      eventBoundary = workingBuffer.indexOf('\n\n');
    }

    return workingBuffer;
  }

  handleRawSseEvent(rawEvent) {
    const normalised = rawEvent.replace(/\r\n/g, '\n').replace(/\r/g, '\n');
    const lines = normalised.split('\n');
    const dataLines = [];

    for (const line of lines) {
      if (line.startsWith('data:')) {
        dataLines.push(line.slice(5).trimStart());
      }
    }

    if (dataLines.length === 0) {
      return;
    }

    const payload = dataLines.join('\n');

    if (!payload) {
      return;
    }

    try {
      const state = JSON.parse(payload);
      this.updateState(state);
    } catch (error) {
      console.error(`[${this.sessionLabel}] failed to parse SSE payload: ${error.message}`);
    }
  }

  updateState(state) {
    this.latestState = state;

    if (!this.streamReady.settled) {
      this.streamReady.resolve();
    }

    const phase = state?.readiness?.phase;
    if (typeof phase === 'string' && phase !== this.lastReadinessPhase) {
      this.lastReadinessPhase = phase;
      console.log(`[${this.sessionLabel}] readiness -> ${phase}`);
    }

    this.eventEmitter.emit('state', state);
  }

  ingestImmediateState(state) {
    if (state && typeof state === 'object') {
      this.updateState(state);
    }
  }

  getMessageCount() {
    return getMessages(this.latestState).length;
  }

  waitFor(predicate, timeoutMs, description) {
    if (predicate(this.latestState)) {
      return Promise.resolve(this.latestState);
    }

    return new Promise((resolve, reject) => {
      let finished = false;

      const onState = (state) => {
        if (finished) {
          return;
        }

        try {
          if (predicate(state)) {
            cleanup();
            resolve(state);
          }
        } catch (error) {
          cleanup();
          reject(error);
        }
      };

      const timer = setTimeout(() => {
        if (finished) {
          return;
        }
        finished = true;
        this.eventEmitter.off('state', onState);
        reject(new Error(`Timed out waiting for ${description} after ${timeoutMs}ms`));
      }, timeoutMs);

      const cleanup = () => {
        if (finished) {
          return;
        }
        finished = true;
        clearTimeout(timer);
        this.eventEmitter.off('state', onState);
      };

      this.eventEmitter.on('state', onState);
    });
  }

  async waitForReadiness() {
    const start = Date.now();
    await this.streamReady.promise;
    const state = await this.waitFor(
      (latest) => latest?.readiness?.phase === 'ready',
      this.config.readinessTimeoutMs,
      'readiness to reach ready',
    );

    return { state, elapsedMs: Date.now() - start };
  }

  async waitForRoundCompletion(baselineCount, roundIndex) {
    const start = Date.now();

    const state = await this.waitFor(
      (latest) => {
        if (!latest || latest.is_processing) {
          return false;
        }

        const messages = getMessages(latest);
        if (messages.length < baselineCount + 2) {
          return false;
        }

        const lastMessage = messages[messages.length - 1];
        if (!lastMessage || lastMessage.sender === 'user') {
          return false;
        }

        if (typeof lastMessage.content !== 'string' || lastMessage.content.trim().length === 0) {
          return false;
        }

        return true;
      },
      this.config.roundTimeoutMs,
      `round ${roundIndex + 1} to complete`,
    );

    return { state, elapsedMs: Date.now() - start };
  }

  async stop() {
    this.stopped = true;

    if (this.streamAbortController) {
      this.streamAbortController.abort();
    }

    if (this.streamPromise) {
      try {
        await this.streamPromise;
      } catch (error) {
        if (error && error.name !== 'AbortError') {
          console.error(`[${this.sessionLabel}] stream stop error: ${error.message}`);
        }
      }
    }
  }
}

function buildPrompt(config, sessionIndex, roundIndex) {
  const base = config.prompts[(sessionIndex + roundIndex) % config.prompts.length];
  return `${base} (session ${sessionIndex + 1}, round ${roundIndex + 1})`;
}

async function runSession(config, sessionIndex) {
  const runner = new SessionRunner(config, sessionIndex);
  const errors = [];
  let messagesSent = 0;

  console.log(`[${runner.sessionLabel}] starting with session_id=${runner.sessionId}`);
  runner.startStream();

  try {
    let readinessInfo;

    try {
      readinessInfo = await runner.waitForReadiness();
      console.log(
        `[${runner.sessionLabel}] ready after ${readinessInfo.elapsedMs}ms (messages=${getMessages(readinessInfo.state).length})`,
      );
    } catch (error) {
      errors.push({ stage: 'readiness', error });
      console.error(`[${runner.sessionLabel}] failed during readiness: ${error.message}`);
      return { sessionId: runner.sessionId, errors, messagesSent };
    }

    for (let round = 0; round < config.rounds; round += 1) {
      const prompt = buildPrompt(config, sessionIndex, round);
      console.log(`[${runner.sessionLabel}] round ${round + 1} -> "${prompt}"`);

      try {
        const baselineCount = runner.getMessageCount();
        const postResponse = await postChat(config, runner.sessionId, prompt);
        messagesSent += 1;

        if (postResponse) {
          runner.ingestImmediateState(postResponse);
        }

        const { state, elapsedMs } = await runner.waitForRoundCompletion(baselineCount, round);
        const messages = getMessages(state);
        const lastMessage = messages[messages.length - 1];
        const preview = lastMessage?.content
          ? lastMessage.content.slice(0, 120).replace(/\s+/g, ' ')
          : '';

        console.log(
          `[${runner.sessionLabel}] round ${round + 1} done in ${elapsedMs}ms (messages=${messages.length}) response="${preview}"`,
        );
      } catch (error) {
        console.error(`[${runner.sessionLabel}] round ${round + 1} error: ${error.message}`);
        errors.push({ stage: `round-${round + 1}`, error });
      }

      if (config.messageDelayMs > 0 && round < config.rounds - 1) {
        await delay(config.messageDelayMs);
      }
    }
  } finally {
    await runner.stop();
  }

  return { sessionId: runner.sessionId, errors, messagesSent };
}

async function main() {
  const config = buildConfig();
  console.log(`Starting stress test against ${config.backendUrl}`);
  console.log(
    `Configured for ${config.sessions} sessions, ${config.rounds} rounds each (concurrent SSE streams, reconnect delay ${config.streamReconnectDelayMs}ms)`,
  );

  const results = await Promise.all(
    Array.from({ length: config.sessions }, (_value, index) => runSession(config, index)),
  );

  let totalErrors = 0;
  let totalMessages = 0;

  for (const result of results) {
    totalMessages += result.messagesSent;
    totalErrors += result.errors.length;
    if (result.errors.length > 0) {
      console.error(`Session ${result.sessionId} encountered ${result.errors.length} error(s):`);
      for (const err of result.errors) {
        console.error(`  - ${err.stage}: ${err.error.message}`);
      }
    }
  }

  console.log(`\nStress test complete. Sessions run: ${results.length}. Messages sent: ${totalMessages}.`);
  if (totalErrors > 0) {
    console.error(`Encountered ${totalErrors} error(s) across all sessions.`);
    process.exitCode = 1;
  } else {
    console.log('All sessions completed without reported errors.');
  }
}

main().catch((error) => {
  console.error(`Unexpected failure: ${error.stack || error.message}`);
  process.exit(1);
});
