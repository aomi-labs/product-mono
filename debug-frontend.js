const { exec } = require('child_process');
const path = require('path');

// Simple headless browser test using node
async function testFrontendWithCurl() {
  console.log('🔍 Testing frontend with curl and examining structure...\n');

  return new Promise((resolve, reject) => {
    exec('curl -s http://localhost:3000/', (error, stdout, stderr) => {
      if (error) {
        console.error('❌ Error fetching frontend:', error.message);
        reject(error);
        return;
      }

      console.log('✅ Frontend loaded successfully');
      console.log('📄 Page title:', stdout.match(/<title>(.*?)<\/title>/)?.[1] || 'Not found');

      // Check for React hydration
      const hasReact = stdout.includes('__NEXT_DATA__') || stdout.includes('_next');
      console.log('⚛️ React/Next.js detected:', hasReact ? 'Yes' : 'No');

      // Check for chat-related content
      const chatKeywords = ['chat', 'message', 'terminal', 'input'];
      const foundKeywords = chatKeywords.filter(keyword =>
        stdout.toLowerCase().includes(keyword.toLowerCase())
      );
      console.log('💬 Chat-related keywords found:', foundKeywords.length > 0 ? foundKeywords : 'None');

      // Check for UI components
      const hasButton = stdout.includes('button') || stdout.includes('btn');
      const hasInput = stdout.includes('<input') || stdout.includes('<textarea');
      console.log('🔲 Has buttons:', hasButton ? 'Yes' : 'No');
      console.log('📝 Has input fields:', hasInput ? 'Yes' : 'No');

      // Check for JavaScript bundles
      const jsFiles = stdout.match(/_next\/static\/chunks\/[^"]+\.js/g) || [];
      console.log('📦 JavaScript bundles found:', jsFiles.length);

      // Look for specific component classes or IDs
      const componentPattern = /class="([^"]*(?:chat|terminal|message|container)[^"]*)"/gi;
      const componentClasses = [];
      let match;
      while ((match = componentPattern.exec(stdout)) !== null) {
        componentClasses.push(match[1]);
      }
      console.log('🎨 Component classes found:', componentClasses.length > 0 ? componentClasses : 'None');

      // Check for potential loading states
      const hasLoading = stdout.includes('loading') || stdout.includes('spinner');
      console.log('⏳ Loading states found:', hasLoading ? 'Yes' : 'No');

      // Look for error messages
      const hasError = stdout.includes('error') || stdout.includes('Error');
      console.log('❌ Error content found:', hasError ? 'Yes' : 'No');

      console.log('\n📊 Summary:');
      console.log('- Frontend is responding and serving content');
      console.log('- React/Next.js appears to be working');
      if (foundKeywords.length === 0) {
        console.log('⚠️  No chat-related keywords found in initial HTML');
        console.log('   This suggests the chat UI might be:');
        console.log('   1. Rendered client-side after hydration');
        console.log('   2. Hidden or not loaded yet');
        console.log('   3. Using different naming conventions');
      }

      resolve();
    });
  });
}

async function testBackendState() {
  console.log('\n🔍 Testing backend state...\n');

  return new Promise((resolve, reject) => {
    exec('curl -s http://localhost:8080/api/state', (error, stdout, stderr) => {
      if (error) {
        console.error('❌ Error fetching backend state:', error.message);
        reject(error);
        return;
      }

      try {
        const state = JSON.parse(stdout);
        console.log('✅ Backend state retrieved successfully');
        console.log('📊 Current state:');
        console.log('  - Messages:', state.messages?.length || 0);
        console.log('  - Processing:', state.is_processing || false);
        const readiness = state.readiness || {};
        console.log('  - Readiness phase:', readiness.phase || 'unknown');
        if (readiness.detail) {
          console.log('  - Readiness detail:', readiness.detail);
        }
        console.log('  - Pending wallet TX:', state.pending_wallet_tx ? 'Yes' : 'No');

        if (state.messages && state.messages.length > 0) {
          console.log('\n💬 Recent messages:');
          state.messages.slice(-3).forEach((msg, idx) => {
            console.log(`  ${idx + 1}. ${msg.sender}: ${msg.content.substring(0, 50)}...`);
          });
        }
      } catch (parseError) {
        console.error('❌ Failed to parse backend state:', parseError.message);
        console.log('Raw response:', stdout.substring(0, 200) + '...');
      }

      resolve();
    });
  });
}

async function main() {
  try {
    await testFrontendWithCurl();
    await testBackendState();

    console.log('\n🎯 Next Steps for Debugging:');
    console.log('1. Open browser developer tools at http://localhost:3000');
    console.log('2. Check console for JavaScript errors');
    console.log('3. Look for network requests to localhost:8080');
    console.log('4. Verify if ChatManager is connecting successfully');
    console.log('5. Check if React components are rendering properly');

  } catch (error) {
    console.error('Test failed:', error);
  }
}

main();
