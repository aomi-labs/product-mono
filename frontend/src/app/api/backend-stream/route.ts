import { NextRequest } from 'next/server';

export async function GET(request: NextRequest) {
  const BACKEND_URL = 'http://68.183.172.179:8081';
  const { searchParams } = new URL(request.url);
  const sessionId = searchParams.get('session_id');

  if (!sessionId) {
    return new Response(JSON.stringify({ error: 'session_id parameter is required' }), {
      status: 400,
      headers: { 'Content-Type': 'application/json' },
    });
  }

  try {
    const response = await fetch(`${BACKEND_URL}/api/chat/stream?session_id=${sessionId}`);

    if (!response.ok) {
      throw new Error(`HTTP ${response.status}: ${response.statusText}`);
    }

    return new Response(response.body, {
      status: 200,
      headers: {
        'Content-Type': 'text/event-stream',
        'Cache-Control': 'no-cache',
        'Connection': 'keep-alive',
        'Access-Control-Allow-Origin': '*',
        'Access-Control-Allow-Headers': 'Cache-Control'
      },
    });

  } catch (error) {
    console.error('SSE proxy error:', error);
    return new Response(JSON.stringify({ error: 'SSE proxy failed' }), {
      status: 500,
      headers: { 'Content-Type': 'application/json' },
    });
  }
}