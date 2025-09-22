// Simple test to check frontend and backend connectivity
const http = require('http');
const https = require('https');

async function testEndpoint(url, options = {}) {
  return new Promise((resolve, reject) => {
    const client = url.startsWith('https:') ? https : http;
    const req = client.request(url, options, (res) => {
      let data = '';
      res.on('data', chunk => data += chunk);
      res.on('end', () => resolve({
        status: res.statusCode,
        headers: res.headers,
        body: data
      }));
    });
    req.on('error', reject);
    req.setTimeout(5000, () => reject(new Error('Timeout')));
    if (options.body) {
      req.write(options.body);
    }
    req.end();
  });
}

async function runTests() {
  console.log('🔍 Testing frontend and backend connectivity...\n');

  // Test 1: Frontend
  try {
    console.log('1. Testing frontend (http://localhost:3000/)...');
    const frontendRes = await testEndpoint('http://localhost:3000/');
    console.log('✅ Frontend status:', frontendRes.status);
    console.log('📄 Contains HTML:', frontendRes.body.includes('<html>') ? 'Yes' : 'No');
    console.log('🔍 Page title:', frontendRes.body.match(/<title>(.*?)<\/title>/)?.[1] || 'Not found');
  } catch (error) {
    console.log('❌ Frontend error:', error.message);
  }

  // Test 2: Backend health
  try {
    console.log('\n2. Testing backend health (http://localhost:8080/health)...');
    const healthRes = await testEndpoint('http://localhost:8080/health');
    console.log('✅ Backend health status:', healthRes.status);
    console.log('📝 Response:', healthRes.body.trim());
  } catch (error) {
    console.log('❌ Backend health error:', error.message);
  }

  // Test 3: Backend state endpoint
  try {
    console.log('\n3. Testing backend state endpoint...');
    const stateRes = await testEndpoint('http://localhost:8080/api/state');
    console.log('✅ State endpoint status:', stateRes.status);
    if (stateRes.status === 200) {
      const state = JSON.parse(stateRes.body);
      console.log('📊 State data:');
      console.log('  - Messages:', state.messages?.length || 0);
      console.log('  - Processing:', state.is_processing || false);
      const readiness = state.readiness || {};
      console.log('  - Readiness phase:', readiness.phase || 'unknown');
      if (readiness.detail) {
        console.log('  - Readiness detail:', readiness.detail);
      }
    }
  } catch (error) {
    console.log('❌ State endpoint error:', error.message);
  }

  // Test 4: SSE endpoint (just check if it responds)
  try {
    console.log('\n4. Testing SSE endpoint...');
    const sseRes = await testEndpoint('http://localhost:8080/api/chat/stream?session_id=test-123');
    console.log('✅ SSE endpoint status:', sseRes.status);
    console.log('📡 Content-Type:', sseRes.headers['content-type'] || 'Not set');
  } catch (error) {
    console.log('❌ SSE endpoint error:', error.message);
  }

  // Test 5: Chat endpoint with session
  try {
    console.log('\n5. Testing chat endpoint with session...');
    const chatRes = await testEndpoint('http://localhost:8080/api/chat', {
      method: 'POST',
      headers: {
        'Content-Type': 'application/json'
      },
      body: JSON.stringify({
        message: 'Hello test',
        session_id: 'test-123'
      })
    });
    console.log('✅ Chat endpoint status:', chatRes.status);
    if (chatRes.status === 200) {
      const chatState = JSON.parse(chatRes.body);
      console.log('💬 Chat response:');
      console.log('  - Messages:', chatState.messages?.length || 0);
      if (chatState.messages && chatState.messages.length > 0) {
        const lastMsg = chatState.messages[chatState.messages.length - 1];
        console.log('  - Last message:', lastMsg.sender, ':', lastMsg.content.substring(0, 50) + '...');
      }
    }
  } catch (error) {
    console.log('❌ Chat endpoint error:', error.message);
  }

  console.log('\n🏁 Test complete!');
}

runTests().catch(console.error);
