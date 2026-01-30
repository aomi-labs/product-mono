import { NextRequest, NextResponse } from 'next/server';
import { Pool } from 'pg';

const pool = new Pool({
  connectionString: process.env.DATABASE_URL || 'postgresql://aomi@localhost:5432/chatbot',
});

// Store pending transactions (in production, use Redis or DB)
const pendingTxs = new Map<string, {
  sessionKey: string;
  tx: {
    to: string;
    value: string;
    data?: string;
    chainId: number;
  };
  createdAt: number;
  status: 'pending' | 'signed' | 'rejected';
}>();

// GET: Fetch pending transaction for a session
export async function GET(request: NextRequest) {
  const { searchParams } = new URL(request.url);
  const sessionKey = searchParams.get('session_key');
  const txId = searchParams.get('tx_id');

  if (!sessionKey && !txId) {
    return NextResponse.json({ error: 'Missing session_key or tx_id' }, { status: 400 });
  }

  // Find by txId or sessionKey
  let pendingTx = null;
  if (txId) {
    pendingTx = pendingTxs.get(txId);
  } else if (sessionKey) {
    // Find most recent pending tx for this session
    for (const [id, tx] of pendingTxs.entries()) {
      if (tx.sessionKey === sessionKey && tx.status === 'pending') {
        pendingTx = { ...tx, txId: id };
        break;
      }
    }
  }

  if (!pendingTx) {
    return NextResponse.json({ pending: false });
  }

  return NextResponse.json({
    pending: true,
    txId: txId || pendingTx.txId,
    tx: pendingTx.tx,
    createdAt: pendingTx.createdAt,
  });
}

// POST: Create a new pending transaction (called by backend)
export async function POST(request: NextRequest) {
  try {
    const body = await request.json();
    const { session_key, tx, tx_id } = body;

    if (!session_key || !tx) {
      return NextResponse.json({ error: 'Missing session_key or tx' }, { status: 400 });
    }

    const txId = tx_id || `tx_${Date.now()}_${Math.random().toString(36).slice(2, 8)}`;
    
    pendingTxs.set(txId, {
      sessionKey: session_key,
      tx: {
        to: tx.to,
        value: tx.value || '0',
        data: tx.data,
        chainId: tx.chainId || 1,
      },
      createdAt: Date.now(),
      status: 'pending',
    });

    console.log('Created pending tx:', txId, 'for session:', session_key);

    return NextResponse.json({
      success: true,
      txId,
    });
  } catch (error) {
    console.error('Error creating pending tx:', error);
    return NextResponse.json({ error: 'Internal server error' }, { status: 500 });
  }
}

// PUT: Update transaction status (signed/rejected)
export async function PUT(request: NextRequest) {
  try {
    const body = await request.json();
    const { tx_id, status, tx_hash } = body;

    if (!tx_id || !status) {
      return NextResponse.json({ error: 'Missing tx_id or status' }, { status: 400 });
    }

    const pendingTx = pendingTxs.get(tx_id);
    if (!pendingTx) {
      return NextResponse.json({ error: 'Transaction not found' }, { status: 404 });
    }

    pendingTx.status = status;
    
    console.log('Updated tx:', tx_id, 'status:', status, 'hash:', tx_hash);

    // If signed, we could notify the backend/session here
    // For now, the bot will poll or check on next message

    return NextResponse.json({
      success: true,
      txId: tx_id,
      status,
      txHash: tx_hash,
    });
  } catch (error) {
    console.error('Error updating tx:', error);
    return NextResponse.json({ error: 'Internal server error' }, { status: 500 });
  }
}
