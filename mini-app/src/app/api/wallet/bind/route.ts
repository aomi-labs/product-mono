import { NextRequest, NextResponse } from 'next/server';
import crypto from 'crypto';

// Verify Telegram initData
function verifyTelegramAuth(initData: string, botToken: string): boolean {
  if (!initData) return false;
  
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
}

export async function POST(request: NextRequest) {
  try {
    const body = await request.json();
    const { wallet_address, platform, platform_user_id, init_data } = body;

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

    // Verify Telegram auth if init_data provided
    if (platform === 'telegram' && init_data) {
      const botToken = process.env.TELEGRAM_BOT_TOKEN;
      if (botToken && !verifyTelegramAuth(init_data, botToken)) {
        return NextResponse.json(
          { error: 'Invalid Telegram authentication' },
          { status: 401 }
        );
      }
    }

    // Build session key
    const session_key = `${platform}:dm:${platform_user_id}`;

    // Call backend to bind wallet
    const backendUrl = process.env.BACKEND_URL || 'http://localhost:8080';
    const backendResponse = await fetch(`${backendUrl}/api/wallet/bind`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({
        session_key,
        wallet_address,
      }),
    });

    if (!backendResponse.ok) {
      const error = await backendResponse.text();
      console.error('Backend error:', error);
      return NextResponse.json(
        { error: 'Failed to bind wallet' },
        { status: 500 }
      );
    }

    return NextResponse.json({
      success: true,
      wallet_address,
      session_key,
    });

  } catch (error) {
    console.error('Error binding wallet:', error);
    return NextResponse.json(
      { error: 'Internal server error' },
      { status: 500 }
    );
  }
}
