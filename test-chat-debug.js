const { chromium } = require('/Users/ceciliazhang/.npm/_npx/e41f203b7505f1fb/node_modules/playwright');

async function debugChatConnection() {
  const browser = await chromium.launch({ headless: true });
  const page = await browser.newPage();

  // Collect console logs
  const logs = [];
  page.on('console', msg => {
    const text = msg.text();
    // Only log relevant messages
    if (text.includes('ChatManager') || text.includes('sendMessage') || text.includes('connection') ||
        text.includes('🚀') || text.includes('🌐') || text.includes('❌') || text.includes('✅') ||
        text.includes('handleSendMessage') || text.includes('SSE') || text.includes('Backend')) {
      logs.push(`${msg.type()}: ${text}`);
    }
  });

  // Capture network requests
  const requests = [];
  page.on('request', request => {
    if (request.url().includes('localhost:8080')) {
      requests.push({
        method: request.method(),
        url: request.url(),
        body: request.postData()
      });
    }
  });

  try {
    console.log('🌐 Loading page...');
    await page.goto('http://localhost:3000/', {
      waitUntil: 'domcontentloaded',
      timeout: 10000
    });

    console.log('⏳ Waiting for React hydration and connection setup...');
    await page.waitForTimeout(5000);

    console.log('\n📋 Console logs during page load:');
    logs.forEach(log => console.log('  ', log));

    console.log('\n🌐 Network requests during page load:', requests.length);
    requests.forEach(req => console.log('  ', req.method, req.url));

    // Clear logs and try sending a message
    logs.length = 0;
    requests.length = 0;

    console.log('\n💬 Attempting to send a message...');
    const input = page.locator('#terminal-message-input');
    await input.waitFor({ state: 'visible' });
    await input.click();
    await input.fill('Hello from debug test!');
    await input.press('Enter');

    console.log('⏳ Waiting for response...');
    await page.waitForTimeout(5000);

    console.log('\n📋 Console logs during message send:');
    logs.forEach(log => console.log('  ', log));

    // Check if we see the connection status logs
    if (logs.length === 0 || !logs.some(log => log.includes('Connection status'))) {
      console.log('⚠️  No connection status logs found - message may have been sent successfully');
    }

    console.log('\n🌐 Network requests during message send:', requests.length);
    requests.forEach(req => {
      console.log('  ', req.method, req.url);
      if (req.body) console.log('    Body:', req.body);
    });

    // Check final connection status
    const statusElement = await page.locator('.connection-status').textContent();
    console.log('\n📊 Final connection status:', statusElement);

  } catch (error) {
    console.error('❌ Error:', error.message);
  } finally {
    await browser.close();
  }
}

console.log('🔍 Starting debug test...\n');
debugChatConnection();