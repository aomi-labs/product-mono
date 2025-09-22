const { chromium } = require('/Users/ceciliazhang/.npm/_npx/e41f203b7505f1fb/node_modules/playwright');

async function testChatFunctionality() {
  const browser = await chromium.launch({
    headless: true,
    args: ['--no-sandbox', '--disable-setuid-sandbox']
  });

  const page = await browser.newPage();

  try {
    console.log('🌐 Navigating to http://localhost:3000/...');
    await page.goto('http://localhost:3000/', {
      waitUntil: 'domcontentloaded',
      timeout: 10000
    });

    console.log('📄 Page loaded, title:', await page.title());

    // Wait for the page to hydrate
    console.log('⏳ Waiting for React hydration...');
    await page.waitForTimeout(3000);

    // Take screenshot before interaction
    await page.screenshot({ path: 'before-chat.png', fullPage: true });
    console.log('📸 Screenshot saved: before-chat.png');

    // Look for input fields
    console.log('🔍 Looking for input fields...');
    const inputs = await page.locator('input, textarea').count();
    console.log(`Found ${inputs} input fields`);

    if (inputs > 0) {
      // Get all inputs and their details
      const inputDetails = await page.$$eval('input, textarea', elements =>
        elements.map((el, i) => ({
          index: i,
          type: el.type || el.tagName.toLowerCase(),
          placeholder: el.placeholder || '',
          className: el.className || '',
          id: el.id || '',
          disabled: el.disabled,
          style: el.style.display
        }))
      );

      console.log('📝 Input field details:', JSON.stringify(inputDetails, null, 2));

      // Try to find a chat input specifically
      let chatInput = null;

      // Try different selectors for chat input
      const selectors = [
        'input[placeholder*="message"]',
        'input[placeholder*="chat"]',
        'input[placeholder*="type"]',
        'textarea[placeholder*="message"]',
        'textarea[placeholder*="chat"]',
        '.terminal-input input',
        '[data-testid="chat-input"]',
        'input:not([type="hidden"])',
        'textarea'
      ];

      for (const selector of selectors) {
        const element = page.locator(selector);
        const count = await element.count();
        if (count > 0) {
          console.log(`✅ Found input with selector: ${selector}`);
          chatInput = element.first();
          break;
        }
      }

      if (chatInput) {
        console.log('💬 Attempting to send a test message...');

        // Wait for the input to be visible and enabled
        await chatInput.waitFor({ state: 'visible', timeout: 5000 });

        const isDisabled = await chatInput.isDisabled();
        console.log('🔒 Input disabled:', isDisabled);

        if (!isDisabled) {
          // Click to focus and type message
          await chatInput.click();
          await chatInput.fill('Hello, this is a test message from Playwright!');

          console.log('✅ Message typed into input field');

          // Look for submit button or try Enter key
          const submitButtons = await page.$$eval('button', buttons =>
            buttons.map(btn => ({
              text: btn.textContent?.trim(),
              className: btn.className,
              type: btn.type,
              disabled: btn.disabled
            })).filter(btn =>
              btn.text?.toLowerCase().includes('send') ||
              btn.text?.toLowerCase().includes('submit') ||
              btn.className?.includes('send') ||
              btn.type === 'submit'
            )
          );

          console.log('🔘 Found submit buttons:', submitButtons);

          // Try pressing Enter first (most common for chat)
          await chatInput.press('Enter');
          console.log('✅ Pressed Enter key to send message');

          // Wait for potential response
          console.log('⏳ Waiting for response...');
          await page.waitForTimeout(5000);

          // Check for new messages or activity
          const messageElements = await page.$$eval('[class*="message"], [class*="chat"], .chat-array *', elements =>
            elements.map(el => el.textContent?.trim()).filter(text => text && text.length > 0)
          );

          console.log('💬 Messages found on page:', messageElements.length);
          if (messageElements.length > 0) {
            console.log('📝 Recent messages:', messageElements.slice(-5));
          }

        } else {
          console.log('⚠️ Input field is disabled - backend might still be loading');
        }
      } else {
        console.log('❌ Could not find a suitable chat input field');

        // Debug: show all interactive elements
        const allInteractive = await page.$$eval('button, input, textarea, [onclick], [role="button"]', elements =>
          elements.map(el => ({
            tag: el.tagName.toLowerCase(),
            type: el.type,
            text: el.textContent?.trim().substring(0, 50),
            className: el.className,
            id: el.id
          }))
        );
        console.log('🔍 All interactive elements found:', JSON.stringify(allInteractive, null, 2));
      }
    } else {
      console.log('❌ No input fields found at all');
    }

    // Check for connection status or loading indicators
    const connectionInfo = await page.evaluate(() => {
      // Look for connection status indicators
      const statusElements = document.querySelectorAll('[class*="status"], [class*="connect"], [class*="loading"]');
      return Array.from(statusElements).map(el => ({
        className: el.className,
        text: el.textContent?.trim()
      }));
    });

    console.log('🔌 Connection status elements:', connectionInfo);

    // Check network requests to backend
    const requests = [];
    page.on('request', request => {
      if (request.url().includes('localhost:8080')) {
        requests.push({
          url: request.url(),
          method: request.method()
        });
      }
    });

    await page.waitForTimeout(2000);
    console.log('🌐 Backend requests made:', requests.length > 0 ? requests : 'None detected');

    // Final screenshot
    await page.screenshot({ path: 'after-chat-attempt.png', fullPage: true });
    console.log('📸 Final screenshot saved: after-chat-attempt.png');

  } catch (error) {
    console.error('❌ Error during chat test:', error.message);

    // Emergency screenshot
    try {
      await page.screenshot({ path: 'error-screenshot.png', fullPage: true });
      console.log('📸 Error screenshot saved: error-screenshot.png');
    } catch (screenshotError) {
      console.log('Failed to take error screenshot');
    }
  } finally {
    await browser.close();
  }
}

console.log('🚀 Starting headless Playwright chat test...\n');
testChatFunctionality().then(() => {
  console.log('\n✅ Chat test completed');
}).catch(error => {
  console.error('\n❌ Chat test failed:', error);
});