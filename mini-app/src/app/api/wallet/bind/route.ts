import { NextRequest, NextResponse } from 'next/server';
import crypto from 'crypto';

const walletBindInternalKey =
  process.env.WALLET_BIND_INTERNAL_KEY || process.env.TELEGRAM_BOT_TOKEN || '';
const telegramBotToken = process.env.TELEGRAM_BOT_TOKEN || '';
const telegramInitDataMaxAgeSeconds = Number(
  process.env.TELEGRAM_INIT_DATA_MAX_AGE_SECONDS || '600'
);

function unique<T>(values: T[]): T[] {
  return Array.from(new Set(values));
}

function backendCandidates(): string[] {
  const configuredPort = process.env.BACKEND_PORT?.trim();
  return unique(
    [
      process.env.BACKEND_URL,
      process.env.NEXT_PUBLIC_BACKEND_URL,
      configuredPort ? `http://127.0.0.1:${configuredPort}` : undefined,
      configuredPort ? `http://localhost:${configuredPort}` : undefined,
      'http://127.0.0.1:8080',
      'http://localhost:8080',
      'http://127.0.0.1:8081',
      'http://localhost:8081',
    ]
      .map((v) => v?.trim())
      .filter((v): v is string => Boolean(v && v.length > 0))
      .map((v) => v.replace(/\/+$/, ''))
  );
}

async function postToBackend(path: string, body: unknown) {
  const candidates = backendCandidates();
  let lastError: unknown = null;

  for (const baseUrl of candidates) {
    const endpoint = `${baseUrl}${path}`;
    try {
      const response = await fetch(endpoint, {
        method: 'POST',
        headers: {
          'Content-Type': 'application/json',
          'X-Wallet-Bind-Key': walletBindInternalKey,
        },
        body: JSON.stringify(body),
        cache: 'no-store',
      });
      const data = await response.json().catch(() => ({ error: 'Invalid backend response' }));
      return { response, data, endpoint };
    } catch (error) {
      lastError = error;
    }
  }

  throw new Error(
    `Failed to reach backend (${candidates.join(', ')}). Last error: ${
      lastError instanceof Error ? lastError.message : String(lastError)
    }`
  );
}

function verifyTelegramAuth(initData: string, botToken: string): boolean {
  if (!initData || !botToken) return false;

  try {
    const params = new URLSearchParams(initData);
    const hash = params.get('hash');
    if (!hash) return false;
    params.delete('hash');

    const sortedParams = Array.from(params.entries())
      .sort(([a], [b]) => a.localeCompare(b))
      .map(([k, v]) => `${k}=${v}`)
      .join('\n');

    const secretKey = crypto
      .createHmac('sha256', 'WebAppData')
      .update(botToken)
      .digest();

    const calculatedHash = crypto
      .createHmac('sha256', secretKey)
      .update(sortedParams)
      .digest('hex');

    return calculatedHash === hash;
  } catch {
    return false;
  }
}

function extractTelegramUserId(initData: string): string | null {
  try {
    const params = new URLSearchParams(initData);
    const userRaw = params.get('user');
    if (!userRaw) return null;
    const user = JSON.parse(userRaw) as { id?: number | string };
    if (user?.id === undefined || user?.id === null) return null;
    return String(user.id);
  } catch {
    return null;
  }
}

function isTelegramInitDataFresh(initData: string, maxAgeSeconds: number): boolean {
  if (!Number.isFinite(maxAgeSeconds) || maxAgeSeconds <= 0) return true;
  try {
    const params = new URLSearchParams(initData);
    const authDateRaw = params.get('auth_date');
    if (!authDateRaw) return false;
    const authDate = Number(authDateRaw);
    if (!Number.isFinite(authDate)) return false;
    const now = Math.floor(Date.now() / 1000);
    return now - authDate <= maxAgeSeconds;
  } catch {
    return false;
  }
}

export async function POST(request: NextRequest) {
  try {
    const body = await request.json();
    const walletAddress = body?.wallet_address;
    const platform = body?.platform;
    const platformUserId = body?.platform_user_id;
    const initData = body?.init_data;

    if (!walletAddress || !platform || !platformUserId) {
      return NextResponse.json({ error: 'Missing required fields' }, { status: 400 });
    }
    if (platform !== 'telegram') {
      return NextResponse.json({ error: 'Unsupported platform' }, { status: 400 });
    }
    if (!walletBindInternalKey) {
      return NextResponse.json(
        { error: 'Wallet bind internal key is not configured' },
        { status: 500 }
      );
    }
    if (!telegramBotToken) {
      return NextResponse.json(
        { error: 'TELEGRAM_BOT_TOKEN is not configured' },
        { status: 500 }
      );
    }
    if (!initData || typeof initData !== 'string') {
      return NextResponse.json({ error: 'Missing Telegram init_data' }, { status: 401 });
    }
    if (!verifyTelegramAuth(initData, telegramBotToken)) {
      return NextResponse.json({ error: 'Invalid Telegram authentication payload' }, { status: 401 });
    }
    if (!isTelegramInitDataFresh(initData, telegramInitDataMaxAgeSeconds)) {
      return NextResponse.json({ error: 'Expired Telegram authentication payload' }, { status: 401 });
    }

    const verifiedUserId = extractTelegramUserId(initData);
    if (!verifiedUserId) {
      return NextResponse.json({ error: 'Missing Telegram user info in init_data' }, { status: 401 });
    }
    if (verifiedUserId !== String(platformUserId)) {
      return NextResponse.json({ error: 'Telegram user mismatch' }, { status: 403 });
    }

    const { response, data } = await postToBackend('/api/wallet/bind', body);
    return NextResponse.json(data, { status: response.status });
  } catch (error) {
    console.error('Error binding wallet:', error);
    return NextResponse.json(
      {
        error:
          'Backend unavailable for wallet bind. ' +
          (error instanceof Error ? error.message : 'Unknown error'),
      },
      { status: 502 }
    );
  }
}

// Health check endpoint
export async function GET() {
  const candidates = backendCandidates();
  try {
    for (const baseUrl of candidates) {
      try {
        const response = await fetch(`${baseUrl}/health`, {
          method: 'GET',
          cache: 'no-store',
        });
        if (response.ok) {
          return NextResponse.json({ status: 'ok', backend: baseUrl }, { status: 200 });
        }
      } catch {
        // Try next candidate.
      }
    }

    return NextResponse.json(
      { status: 'error', backends: candidates },
      { status: 502 }
    );
  } catch (error) {
    return NextResponse.json(
      { status: 'error', backends: candidates },
      { status: 500 }
    );
  }
}
