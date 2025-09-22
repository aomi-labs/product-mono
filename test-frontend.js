const { chromium } = require('/Users/ceciliazhang/.npm/_npx/e41f203b7505f1fb/node_modules/playwright');

(async () => {
  const browser = await chromium.launch({ headless: true });
  const page = await browser.newPage();

  try {
    console.log('🔍 Navigating to http://localhost:3000/...');
    await page.goto('http://localhost:3000/', { waitUntil: 'networkidle', timeout: 10000 });

    console.log('📄 Page title:', await page.title());

    // Check if page loads
    const bodyText = await page.textContent('body');
    console.log('📝 Page contains text:', bodyText ? 'Yes' : 'No');

    // Look for chat-related elements
    const chatContainer = await page.locator('[data-testid="chat-container"], .chat-container, #chat').count();
    console.log('💬 Found chat containers:', chatContainer);

    // Look for any input fields
    const inputs = await page.locator('input, textarea').count();
    console.log('📝 Found input fields:', inputs);

    // Check console logs
    const logs = [];
    page.on('console', msg => logs.push(`${msg.type()}: ${msg.text()}`));

    // Wait a bit to collect console logs
    await page.waitForTimeout(2000);

    console.log('🔍 Console logs:');
    logs.forEach(log => console.log('  ', log));

    // Check network requests
    const requests = [];
    page.on('request', request => {
      if (request.url().includes('localhost:8080')) {
        requests.push(`${request.method()} ${request.url()}`);
      }
    });

    await page.waitForTimeout(1000);
    console.log('🌐 Backend requests:', requests.length > 0 ? requests : 'None');

    // Take a screenshot
    await page.screenshot({ path: 'frontend-debug.png', fullPage: true });
    console.log('📸 Screenshot saved as frontend-debug.png');

    // Check for specific elements that should be present
    const elementChecks = [
      { selector: 'h1, h2, h3', name: 'headings' },
      { selector: 'button', name: 'buttons' },
      { selector: '[class*="chat"]', name: 'chat-related elements' },
      { selector: '[class*="terminal"]', name: 'terminal-related elements' },
    ];

    for (const check of elementChecks) {
      const count = await page.locator(check.selector).count();
      console.log(`🔍 Found ${count} ${check.name}`);
    }

  } catch (error) {
    console.error('❌ Error:', error.message);

    // Try to get more info about the error
    const url = page.url();
    console.log('🔗 Current URL:', url);

    // Check if it's a connection error
    if (error.message.includes('net::ERR_CONNECTION_REFUSED')) {
      console.log('💡 Frontend server might not be running on localhost:3000');
    }
  }

  await browser.close();
})();