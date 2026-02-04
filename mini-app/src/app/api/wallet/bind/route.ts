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
    
    // Sort params alphabetically
    const sortedParams = Array.from(params.entries())
      .sort(([a], [b]) => a.localeCompare(b))
      .map(([k, v]) => `${k}=${v}`)
      .join('\n');
    
    // Create secret key from bot token
    const secretKey = crypto
      .createHmac('sha256', 'WebAppData')
      .update(botToken)
      .digest();
    
    // Calculate hash
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
    const { wallet_address, platform, platform_user_id, init_data } = body;

    console.log('Wallet bind request:', { wallet_address, platform, platform_user_id });

    // Validate required fields
    if (!wallet_address || !platform || !platform_user_id) {
      return NextResponse.json(
        { error: 'Missing required fields' },
        { status: 400 }
      );
    }

    // Validate wallet address format
    if (!/^0x[a-fA-F0-9]{40}$/.test(wallet_address)) {
      return NextResponse.json(
        { error: 'Invalid wallet address format' },
        { status: 400 }
      );
    }

    // Verify Telegram auth if init_data provided (optional for now)
    if (platform === 'telegram' && init_data) {
      const botToken = process.env.TELEGRAM_BOT_TOKEN;
      if (botToken) {
        const isValid = verifyTelegramAuth(init_data, botToken);
        console.log('Telegram auth verification:', isValid);
        // For now, just log - don't block if verification fails
      }
    }

    // Build session key
    const session_key = `${platform}:dm:${platform_user_id}`;

    // Ensure user exists, then bind wallet to session
    await pool.query(
      `
      INSERT INTO users (public_key, username, created_at)
      VALUES ($1, NULL, EXTRACT(EPOCH FROM NOW())::BIGINT)
      ON CONFLICT (public_key) DO NOTHING
      `,
      [wallet_address]
    );

    const query = `
      INSERT INTO sessions (id, public_key, started_at, last_active_at, title, pending_transaction)
      VALUES ($1, $2, EXTRACT(EPOCH FROM NOW())::BIGINT, EXTRACT(EPOCH FROM NOW())::BIGINT, NULL, NULL)
      ON CONFLICT (id)
      DO UPDATE SET public_key = $2
      RETURNING id, public_key
    `;

    const result = await pool.query(query, [session_key, wallet_address]);
    console.log('Wallet bound successfully:', result.rows[0]);

    if (platform === 'telegram') {
      const botToken = process.env.TELEGRAM_BOT_TOKEN;
      if (botToken) {
        const confirmation = `✅ Wallet connected: ${wallet_address}`;
        try {
          await fetch(`https://api.telegram.org/bot${botToken}/sendMessage`, {
            method: 'POST',
            headers: { 'Content-Type': 'application/json' },
            body: JSON.stringify({
              chat_id: platform_user_id,
              text: confirmation,
            }),
          });
        } catch (e) {
          console.error('Failed to send Telegram confirmation:', e);
        }
      } else {
        console.warn('TELEGRAM_BOT_TOKEN not set; skipping confirmation message');
      }
    }

    return NextResponse.json({
      success: true,
      wallet_address,
      session_key,
    });

  } catch (error) {
    console.error('Error binding wallet:', error);
    return NextResponse.json(
      { error: 'Internal server error: ' + (error instanceof Error ? error.message : 'Unknown') },
      { status: 500 }
    );
  }
}

// Health check endpoint
export async function GET() {
  try {
    await pool.query('SELECT 1');
    return NextResponse.json({ status: 'ok', db: 'connected' });
  } catch (error) {
    return NextResponse.json({ status: 'error', db: 'disconnected' }, { status: 500 });
  }
}
