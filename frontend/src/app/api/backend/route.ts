import { NextRequest, NextResponse } from 'next/server';

export async function POST(request: NextRequest) {
  const BACKEND_URL = 'http://68.183.172.179:8081';

  try {
    const body = await request.text();
    const { searchParams } = new URL(request.url);
    const path = searchParams.get('path') || '';

    const response = await fetch(`${BACKEND_URL}/api/${path}`, {
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
    console.error('Backend proxy error:', error);
    return NextResponse.json({ error: 'Backend proxy failed' }, { status: 500 });
  }
}