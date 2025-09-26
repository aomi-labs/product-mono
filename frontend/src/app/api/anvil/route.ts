import { NextRequest, NextResponse } from 'next/server';

export async function POST(request: NextRequest) {
  const ANVIL_URL = 'http://68.183.172.179:8545';

  try {
    const body = await request.text();

    const response = await fetch(ANVIL_URL, {
      method: 'POST',
      headers: {
        'Content-Type': 'application/json',
      },
      body
    });

    const data = await response.text();

    return new NextResponse(data, {
      status: response.status,
      headers: {
        'Content-Type': response.headers.get('content-type') || 'text/plain',
      },
    });

  } catch (error) {
    console.error('Anvil proxy error:', error);
    return NextResponse.json({ error: 'Anvil proxy failed' }, { status: 500 });
  }
}