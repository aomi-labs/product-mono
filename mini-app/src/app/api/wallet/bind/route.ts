import { NextRequest, NextResponse } from 'next/server';
import crypto from 'crypto';
import { Pool } from 'pg';

// Database connection pool
const pool = new Pool({
  connectionString: process.env.DATABASE_URL || 'postgresql://aomi@localhost:5432/chatbot',
});

// Verify Telegram initData
function verifyTelegramAuth(initData: string, botToken: string): boolean {
  if (!initData) return false;
  
  try {
    const params = new URLSearchParams(initData);
    const hash = params.get('hash');
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
  } catch (e) {
    console.error('Error verifying Telegram auth:', e);
    return false;
  }
}

export async function POST(request: NextRequest) {
  try {
    const body = await request.json();
    const { wallet_address, platform, platform_user_id, session_key, init_data } = body;

    console.log('Wallet bind request:', { wallet_address, platform, platform_user_id, session_key });

    // Validate wallet address
    if (!wallet_address) {
      return NextResponse.json({ error: 'Missing wallet address' }, { status: 400 });
    }

    if (!/^0x[a-fA-F0-9]{40}$/.test(wallet_address)) {
      return NextResponse.json({ error: 'Invalid wallet address format' }, { status: 400 });
    }

    // Determine session key - either passed directly or built from platform + user_id
    let finalSessionKey: string;
    
    if (session_key) {
      // Direct session key (Discord flow)
      finalSessionKey = session_key;
    } else if (platform && platform_user_id) {
      // Build session key from components (Telegram flow)
      finalSessionKey = `${platform}:dm:${platform_user_id}`;
    } else {
      return NextResponse.json({ error: 'Missing session_key or platform/platform_user_id' }, { status: 400 });
    }

    // Verify Telegram auth if provided
    if (platform === 'telegram' && init_data) {
      const botToken = process.env.TELEGRAM_BOT_TOKEN;
      if (botToken) {
        const isValid = verifyTelegramAuth(init_data, botToken);
        console.log('Telegram auth verification:', isValid);
      }
    }

    // Insert/update wallet binding
    const query = `
      INSERT INTO user_wallets (session_key, wallet_address, verified_at)
      VALUES ($1, $2, NOW())
      ON CONFLICT (session_key) 
      DO UPDATE SET wallet_address = $2, verified_at = NOW()
      RETURNING session_key, wallet_address
    `;

    const result = await pool.query(query, [finalSessionKey, wallet_address]);
    console.log('Wallet bound successfully:', result.rows[0]);

    return NextResponse.json({
      success: true,
      wallet_address,
      session_key: finalSessionKey,
    });

  } catch (error) {
    console.error('Error binding wallet:', error);
    return NextResponse.json(
      { error: 'Internal server error: ' + (error instanceof Error ? error.message : 'Unknown') },
      { status: 500 }
    );
  }
}

export async function GET() {
  try {
    await pool.query('SELECT 1');
    return NextResponse.json({ status: 'ok', db: 'connected' });
  } catch (error) {
    return NextResponse.json({ status: 'error', db: 'disconnected' }, { status: 500 });
  }
}
