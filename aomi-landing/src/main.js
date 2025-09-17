import './input.css'
import { Button, ChatContainer, TextSection, AnvilLogContainer, TerminalInput, ReadmeContainer, content } from './components.js'
import { ChatManager } from './utils/ChatManager.js'
import { AnvilManager } from './utils/AnvilManager.js'
import { WalletManager } from './utils/WalletManager.js'
import { ConnectionStatus } from './types/index.js'

console.log('Aomi Thoughts - Ready for Figma integration!');

// Simple client-side router
function router() {
  const path = window.location.pathname;

  if (path === '/blog') {
    showBlogPage();
  } else {
    showLandingPage();
  }
}

function showLandingPage() {
  document.getElementById('app').innerHTML = getLandingPageHTML();
  initializeLandingPage();
}

function showBlogPage() {
  document.getElementById('app').innerHTML = getBlogPageHTML();
  initializeBlogPage();
}

function getLandingPageHTML() {
  // Create component instances
  const githubBtn = new Button('Github ↗', 'github');
  const readmeTab = new Button('README', 'tab-inactive');
  const chatTab = new Button('chat', 'tab-active', { indicator: true });
  const anvilTab = new Button('anvil', 'tab-inactive', { indicator: true });
  const connectBtn = new Button('Connect Wallet', 'terminal-connect');

  const chatContainer = new ChatContainer(content.chat.messages);
  const asciiText = new TextSection('ascii', content.ascii);
  const introText = new TextSection('intro-description', content.intro.description);

  return `
    <div id="main-container" class="w-full flex px-10 pb-5 relative bg-white flex flex-col justify-start items-center overflow-hidden">
      <div data-breakpoint="Desktop" class="self-stretch flex flex-col justify-start items-center">
        <!-- Mobile Header -->
        <div class="mobile-nav w-full h-20 max-w-[1500px] pt-5 pb-8 flex justify-between items-center md:hidden">
          <img src="/assets/images/aomi-logo.svg" alt="Aomi" class="h-8 w-auto" />
        </div>

        <!-- Desktop Header -->
        <div class="desktop-nav w-full h-26 flex pt-5 pb-5 flex justify-between items-center px-4">
          <img src="/assets/images/aomi-logo.svg" alt="Aomi" class="h-15 w-auto" />
          <a href="https://github.com/aomi-labs" target="_blank" rel="noopener noreferrer" class="px-4 py-3 bg-black rounded-full flex justify-center items-center gap-0.5 hover:bg-gray-800">
            <div class="text-center justify-start pt-1 text-white text-sm font-light font-['Bauhaus_Chez_Display_2.0'] leading-tight">Github ↗</div>
          </a>
        </div>
      </div>

      <div class="w-full max-w-[1500px] flex flex-col justify-start items-center pt-10 pb-10">
        <div id="terminal-container" class="w-full max-w-[840px] h-[600px] bg-slate-900 rounded-xl shadow-[0px_16px_40px_0px_rgba(0,0,0,0.25),0px_4px_16px_0px_rgba(0,0,0,0.15)] border border-slate-700/50 overflow-hidden">
          <!-- Terminal Header -->
          <div class="terminal-header bg-gray-800 px-4 py-2 flex items-center justify-between rounded-tl-2xl rounded-tr-2xl">
            <div class="flex items-center space-x-4">
              <div class="flex space-x-2">
                <div class="w-[12px] h-[12px] bg-red-500 rounded-full"></div>
                <div class="w-[12px] h-[12px] bg-yellow-500 rounded-full"></div>
                <div class="w-[12px] h-[12px] bg-green-500 rounded-full"></div>
              </div>
              <!-- Tabs in Header -->
              <div class="flex items-center space-x-1">
                <div onclick="switchTab('readme')" id="readme-tab">${readmeTab.render()}</div>
                <div onclick="switchTab('chat')" id="chat-tab">${chatTab.render()}</div>
                <div onclick="switchTab('anvil')" id="anvil-tab">${anvilTab.render()}</div>
              </div>
            </div>

            <div class="flex items-center space-x-2">
              <span class="text-gray-400 text-xs connection-status">Connecting...</span>
              ${connectBtn.render()}
            </div>
          </div>

          <!-- Terminal Content -->
          <div class="terminal-content h-[560px]" id="terminal-content">
            ${chatContainer.render()}
          </div>
        </div>
      </div>

      <div class="self-stretch flex flex-col justify-start items-center">
        <div class="w-full max-w-[700px] pb-28 flex flex-col justify-start items-center">
          <div class="self-stretch pt-5 pb-14 flex flex-col justify-start items-start gap-12">
            <div class="self-stretch flex flex-col justify-start items-center gap-12">
              ${asciiText.render()}
              ${introText.render()}
            </div>
          </div>
        </div>
      </div>

      <div class="w-full flex justify-center">
        <div class="w-full pt-10 pb-5 border-t border-gray-200 flex flex-col justify-end items-start gap-20 px-4">
          <div class="self-stretch inline-flex justify-start items-end gap-10">
            <img src="/assets/images/a.svg" alt="A" class="w-24 h-10 object-contain" />
            <div class="flex-1 h-4"></div>
            <div class="justify-center text-lime-800 text-1.3xl font-light font-['Bauhaus_Chez_Display_2.0'] leading-none">All Rights Reserved</div>
          </div>
        </div>
      </div>
    </div>
  `;
}

function getBlogPageHTML() {

}

// Navigation functions
function navigateToBlog() {
  window.history.pushState({}, '', '/blog');
  router();
  window.scrollTo(0, 0);
}

function navigateToHome() {
  window.history.pushState({}, '', '/');
  router();
  window.scrollTo(0, 0);
}

// Make functions globally accessible
window.navigateToBlog = navigateToBlog;
window.navigateToHome = navigateToHome;
window.switchTab = switchTab;

// Global manager instances
let chatManager = null;
let anvilManager = null;
let anvilLogContainer = null;
let walletManager = null;

// Wallet connection state
let walletConnectionState = {
  isConnected: false,
  hasPromptedNetworkSwitch: false,
  currentWalletNetwork: null,
  currentMcpNetwork: 'testnet', // Default MCP network
};

// Tab management
let currentTab = 'chat'; // 'chat', 'readme', 'anvil'
window.currentTab = currentTab;

// Tab switching function
function switchTab(tabName) {
  if (currentTab === tabName) return; // Already on this tab

  currentTab = tabName;
  window.currentTab = currentTab;

  // Update tab button styles
  updateTabStyles();

  // Update terminal content
  updateTerminalContent();
}

// Set up tab click handlers
function setupTabHandlers() {
  const buttons = document.querySelectorAll('.terminal-header button');
  buttons.forEach(button => {
    const text = button.textContent.toLowerCase().trim();
    if (['readme', 'chat', 'anvil'].includes(text)) {
      button.addEventListener('click', () => {
        switchTab(text);
      });
    }
  });
}

function updateTabStyles() {
  const buttons = document.querySelectorAll('.terminal-header button');
  buttons.forEach(button => {
    const text = button.textContent.toLowerCase().trim();
    if (['readme', 'chat', 'anvil'].includes(text)) {
      if (text === currentTab) {
        // Active tab styles
        button.className = 'text-sm rounded-[4px] border transition-colors px-3 py-0.5 w-[130px] h-6 bg-gray-500 text-white text-xs border-gray-500 border-0.2 hover:bg-gray-500';
      } else {
        // Inactive tab styles
        button.className = 'text-sm rounded-[4px] border transition-colors px-3 py-0.5 w-[130px] h-6 bg-gray-700 text-gray-300 text-xs border-gray-600 border-0.2 hover:bg-gray-600 hover:text-white';
      }
    }
  });
}

function updateTerminalContent() {
  const terminalContent = document.querySelector('.terminal-content');
  if (!terminalContent) return;

  if (currentTab === 'chat') {
    // Use ChatContainer component for consistency
    const chatContainer = new ChatContainer(content.chat.messages);
    terminalContent.innerHTML = chatContainer.render();

    // Reinitialize chat after content is loaded only if we're still on chat tab
    setTimeout(() => {
      if (currentTab === 'chat') {
        initializeChat();
      }
    }, 100);
  } else if (currentTab === 'readme') {
    // Use ReadmeContainer component for consistency
    const readmeContainer = new ReadmeContainer();
    terminalContent.innerHTML = readmeContainer.render();
  } else if (currentTab === 'anvil') {
    // Use AnvilLogContainer component for consistency
    if (!anvilLogContainer) {
      anvilLogContainer = new AnvilLogContainer();
    }
    terminalContent.innerHTML = anvilLogContainer.render();

    // Add clear button functionality
    setTimeout(() => {
      const clearBtn = document.querySelector('#clear-anvil-logs');
      if (clearBtn) {
        clearBtn.onclick = () => {
          if (anvilManager) {
            anvilManager.clearLogs();
            updateAnvilDisplay();
          }
        };
      }
    }, 0);
  }
}


// Initialize chat functionality
function initializeChat() {
  const inputField = document.querySelector('#terminal-message-input');
  const messagesContainer = document.querySelector('#terminal-messages-container');
  const statusElement = document.querySelector('.connection-status');

  if (!inputField || !messagesContainer || !statusElement) {
    console.warn('Chat elements not found, skipping chat initialization');
    return;
  }

  // Initialize ChatManager with event handlers
  chatManager = new ChatManager({
    mcpServerUrl: 'http://localhost:8080',
    maxMessageLength: 2000,
    reconnectAttempts: 5,
    reconnectDelay: 3000,
  }, {
    onMessage: (message) => {
      updateTerminalMessages();
    },
    onConnectionChange: (status) => {
      updateConnectionStatus(status, statusElement);
      updateTabStyles(); // Update tab indicators
    },
    onError: (error) => {
      console.error('Chat error:', error);
      updateConnectionStatus(ConnectionStatus.ERROR, statusElement);
    },
    onTypingChange: (isTyping) => {
      // Could add typing indicator
    },
  });

  // Handle input field events
  inputField.addEventListener('keypress', (event) => {
    if (event.key === 'Enter' && !event.shiftKey) {
      event.preventDefault();
      sendMessage();
    }
  });

  // Connect to backend
  chatManager.connect();
}

// Initialize wallet functionality
function initializeWallet() {
  // Initialize WalletManager with event handlers
  walletManager = new WalletManager({
    onWalletConnected: (walletInfo) => {
      console.log('Wallet connected:', walletInfo);
      walletConnectionState.isConnected = true;
      walletConnectionState.currentWalletNetwork = walletInfo.networkName;
      
      // Update Connect Wallet button
      updateWalletButton(true, walletInfo.account);
      
      // Check if we need to prompt for network switching
      checkAndPromptNetworkSwitch(walletInfo.networkName);
    },
    onWalletDisconnected: () => {
      console.log('Wallet disconnected');
      walletConnectionState.isConnected = false;
      walletConnectionState.hasPromptedNetworkSwitch = false;
      walletConnectionState.currentWalletNetwork = null;
      
      // Update Connect Wallet button
      updateWalletButton(false);
    },
    onNetworkChanged: (networkInfo) => {
      console.log('Network changed:', networkInfo);
      walletConnectionState.currentWalletNetwork = networkInfo.networkName;
      walletConnectionState.hasPromptedNetworkSwitch = false; // Reset prompt flag
      
      // Check if we need to prompt for network switching
      checkAndPromptNetworkSwitch(networkInfo.networkName);
    },
    onError: (error) => {
      console.error('Wallet error:', error);
    },
  });

  // Set up Connect Wallet button click handler
  setupWalletButtonHandler();
  
  // Check if wallet is already connected
  walletManager.checkConnection().then((isConnected) => {
    if (isConnected) {
      const walletState = walletManager.getState();
      walletConnectionState.isConnected = true;
      walletConnectionState.currentWalletNetwork = walletState.networkName;
      updateWalletButton(true, walletState.account);
      
      // Check if we need to prompt for network switching
      checkAndPromptNetworkSwitch(walletState.networkName);
    }
  });
}

function setupWalletButtonHandler() {
  // Use event delegation since the button might be re-rendered
  document.addEventListener('click', (event) => {
    if (event.target.closest('.terminal-connect') || 
        event.target.textContent.includes('Connect Wallet') ||
        event.target.textContent.includes('Connected:')) {
      handleWalletButtonClick();
    }
  });
}

async function handleWalletButtonClick() {
  if (walletConnectionState.isConnected) {
    // Show wallet info or disconnect options
    console.log('Wallet already connected');
    return;
  }

  try {
    await walletManager.connect();
  } catch (error) {
    console.error('Failed to connect wallet:', error);
    // Could show user-friendly error message
  }
}

function updateWalletButton(isConnected, account = null) {
  const connectBtn = document.querySelector('.terminal-connect');
  if (!connectBtn) return;

  if (isConnected && account) {
    const shortAccount = `${account.substring(0, 6)}...${account.substring(account.length - 4)}`;
    connectBtn.textContent = `Connected: ${shortAccount}`;
    connectBtn.className = connectBtn.className.replace('bg-green-600 hover:bg-green-500', 'bg-blue-600 hover:bg-blue-500');
  } else {
    connectBtn.textContent = 'Connect Wallet';
    connectBtn.className = connectBtn.className.replace('bg-blue-600 hover:bg-blue-500', 'bg-green-600 hover:bg-green-500');
  }
}

function checkAndPromptNetworkSwitch(walletNetworkName) {
  // Don't prompt if we've already prompted for this connection session
  if (walletConnectionState.hasPromptedNetworkSwitch) {
    return;
  }

  // Map wallet network to MCP network name
  const mcpNetworkName = walletManager.mapToMcpNetworkName(walletNetworkName);
  
  // Don't prompt if the networks already match
  if (mcpNetworkName === walletConnectionState.currentMcpNetwork) {
    return;
  }

  // Mark that we've prompted to avoid repeated prompts
  walletConnectionState.hasPromptedNetworkSwitch = true;
  
  // Send message to agent about network mismatch
  if (chatManager && chatManager.state.connectionStatus === ConnectionStatus.CONNECTED) {
    const networkSwitchPrompt = `I've detected that your wallet is connected to ${walletNetworkName} network, but the system is currently configured for ${walletConnectionState.currentMcpNetwork}. Would you like me to switch the system network to match your wallet (${mcpNetworkName})?`;
    
    // Add system message to prompt user
    setTimeout(() => {
      if (chatManager) {
        // Simulate system message by directly updating chat state
        addSystemMessage(networkSwitchPrompt);
      }
    }, 1000); // Small delay to ensure chat is ready
  }
}

function addSystemMessage(message) {
  if (!chatManager) return;
  
  // Get current messages and add system message
  const currentState = chatManager.getState();
  const newMessage = {
    sender: 'system',
    content: message,
    timestamp: new Date().toISOString()
  };
  
  // Update state with new system message
  const updatedMessages = [...currentState.messages, newMessage];
  chatManager.updateState({ messages: updatedMessages });
}

// Function for agent to call when user confirms network switch
function switchMcpNetwork(networkName) {
  if (!chatManager) {
    console.error('ChatManager not initialized');
    return;
  }

  // Send network switch command to MCP server
  const switchCommand = `set_network ${networkName}`;
  
  // Send as a system command (not user message)
  fetch(`${chatManager.config.mcpServerUrl}/api/mcp-command`, {
    method: 'POST',
    headers: {
      'Content-Type': 'application/json',
    },
    body: JSON.stringify({ 
      command: 'set_network',
      args: { network: networkName }
    }),
  })
  .then(response => response.json())
  .then(data => {
    console.log('Network switch response:', data);
    walletConnectionState.currentMcpNetwork = networkName;
    
    // Add confirmation message
    addSystemMessage(`✅ System network switched to ${networkName}`);
  })
  .catch(error => {
    console.error('Failed to switch network:', error);
    addSystemMessage(`❌ Failed to switch network: ${error.message}`);
  });
}

// Make function globally accessible for agent
window.switchMcpNetwork = switchMcpNetwork;

// Initialize Anvil functionality
function initializeAnvil() {
  anvilLogContainer = new AnvilLogContainer();

  // Initialize AnvilManager with event handlers
  anvilManager = new AnvilManager({
    anvilUrl: 'http://localhost:8545',
    checkInterval: 2000,
    maxLogEntries: 100,
  }, {
    onStatusChange: (isConnected) => {
      // Update tab indicator when connection status changes
      updateTabStyles();
    },
    onNewLog: (log) => {
      updateAnvilDisplay();
    },
    onError: (error) => {
      console.warn('Anvil error:', error);
    },
  });

  // Start monitoring
  anvilManager.start();
}

function updateAnvilDisplay() {
  if (!anvilManager || !anvilLogContainer) return;

  const logs = anvilManager.getLogs();
  anvilLogContainer.updateLogs(logs);

  // Only update if Anvil tab is active
  if (currentTab === 'anvil') {
    const logsContainer = document.querySelector('#anvil-logs-container');
    if (logsContainer) {
      // Check if user was at bottom before updating
      const wasAtBottom = Math.abs(logsContainer.scrollHeight - logsContainer.clientHeight - logsContainer.scrollTop) < 50;

      logsContainer.innerHTML = anvilLogContainer.logs.map(log => {
        const typeColors = {
          'system': 'text-green-400',
          'info': 'text-blue-400',
          'block': 'text-purple-400',
          'tx': 'text-yellow-400',
          'tx-detail': 'text-gray-400',
          'error': 'text-red-400',
          'warning': 'text-orange-400',
        };

        const textColor = typeColors[log.type] || 'text-gray-300';

        return `
          <div class="anvil-log-entry mb-1">
            <div class="flex items-start space-x-2">
              <span class="text-gray-500 text-xs min-w-[60px] font-mono">${log.timestamp}</span>
              <div class="${textColor} text-xs font-mono leading-relaxed">
                ${log.message}
              </div>
            </div>
          </div>
        `;
      }).join('') || '<div class="text-gray-500 text-xs">No logs yet. Start Anvil to see activity...</div>';

      // Auto-scroll to bottom if user was at bottom
      if (wasAtBottom) {
        setTimeout(() => {
          logsContainer.scrollTop = logsContainer.scrollHeight;
        }, 0);
      }
    }
  }
}

function sendMessage() {
  const inputField = document.querySelector('#terminal-message-input');
  if (!inputField || !chatManager) return;

  const message = inputField.value.trim();
  if (!message) return;

  // Clear input
  inputField.value = '';

  // Send message through ChatManager
  chatManager.sendMessage(message);
}

function updateTerminalMessages() {
  if (!chatManager) return;

  const messagesContainer = document.querySelector('#terminal-messages-container');
  if (!messagesContainer) return;

  // Check if user was at bottom before updating
  const wasAtBottom = Math.abs(messagesContainer.scrollHeight - messagesContainer.clientHeight - messagesContainer.scrollTop) < 50;

  const state = chatManager.getState();

  // Convert backend messages to terminal format
  let messagesHTML = '';

  state.messages.forEach((msg, index) => {
    const icon = msg.sender === 'user' ? '👧 ➜' : msg.sender === 'system' ? '🔧' : '🤖';
    const iconColor = msg.sender === 'user' ? 'text-blue-400' : msg.sender === 'system' ? 'text-yellow-400' : 'text-green-400';
    const textColor = msg.sender === 'user' ? 'text-white' : msg.sender === 'system' ? 'text-yellow-300' : 'text-gray-300';

    const formattedContent = formatMessageContent(msg.content, msg.sender);

    // Only show border if:
    // 1. It's not the last message, OR
    // 2. It's the last message AND the bot is still typing (not finished)
    const isLastMessage = index === state.messages.length - 1;
    const showBorder = !isLastMessage || (isLastMessage && state.isTyping);
    messagesHTML += `
      <div class="chat-array mb-4">
        <div class="flex items-start space-x-3">
          <span class="${iconColor} ml-1 text-md">${icon}</span>
          <div class="${textColor} text-[11px] space-y-2 py-1 px-1 leading-relaxed">
            ${formattedContent}
          </div>
        </div>
        ${showBorder ? '<div class="ml-8 mr-6 mt-4 border-b border-gray-700/50"></div>' : ''}
      </div>
    `;
  });

  messagesContainer.innerHTML = messagesHTML;

  // Only scroll to bottom if user was already at bottom (following the conversation)
  if (wasAtBottom) {
    setTimeout(() => {
      messagesContainer.scrollTop = messagesContainer.scrollHeight;
    }, 0);
  }
}

function formatMessageContent(content, sender) {
  if (sender === 'user') {
    return `<span>${content}</span>`;
  }

  // Use Snarkdown to parse markdown for bot messages
  if (typeof snarkdown !== 'undefined') {
    const markdownHtml = snarkdown(content);

    // Add some basic styling classes to the generated HTML
    return markdownHtml
      .replace(/<ul>/g, '<ul class="ml-4 space-y-1">')
      .replace(/<ol>/g, '<ol class="ml-4 space-y-1 list-decimal">')
      .replace(/<li>/g, '<li class="text-gray-300">')
      .replace(/<p>/g, '<p class="mb-2">')
      .replace(/<code>/g, '<code class="bg-gray-800 px-1 py-0.5 rounded text-green-400 font-mono text-xs">')
      .replace(/<pre>/g, '<pre class="bg-gray-800 p-3 rounded mt-1 mb-1 overflow-x-auto">')
      .replace(/<strong>/g, '<strong class="text-white font-semibold">')
      .replace(/<em>/g, '<em class="text-blue-300 italic">');
  }

  // Fallback to original formatting if snarkdown is not available
  const lines = content.split('\n').filter(line => line.trim());
  let formatted = '';
  let inList = false;

  for (const line of lines) {
    if (line.startsWith('•') || line.startsWith('-')) {
      if (!inList) {
        formatted += '<ul class="ml-4 space-y-1">';
        inList = true;
      }
      formatted += `<li>${line}</li>`;
    } else {
      if (inList) {
        formatted += '</ul>';
        inList = false;
      }
      formatted += `<p>${line}</p>`;
    }
  }

  if (inList) {
    formatted += '</ul>';
  }

  return formatted;
}

function updateConnectionStatus(status, statusElement) {
  if (!statusElement) return;

  switch (status) {
    case ConnectionStatus.CONNECTED:
      statusElement.textContent = 'Connected';
      statusElement.className = 'text-green-400 text-xs connection-status';
      break;
    case ConnectionStatus.CONNECTING:
      statusElement.textContent = 'Connecting...';
      statusElement.className = 'text-yellow-400 text-xs connection-status';
      break;
    case ConnectionStatus.DISCONNECTED:
      statusElement.textContent = 'Disconnected';
      statusElement.className = 'text-gray-400 text-xs connection-status';
      break;
    case ConnectionStatus.ERROR:
      statusElement.textContent = 'Connection Error';
      statusElement.className = 'text-red-400 text-xs connection-status';
      break;
  }
}

// Initialize landing page functionality
function initializeLandingPage() {
  // Set up tab click handlers
  setupTabHandlers();

  // Initialize wallet functionality
  initializeWallet();

  // Initialize chat functionality
  initializeChat();

  // Initialize Anvil functionality
  initializeAnvil();

  const observer = new IntersectionObserver((entries) => {
    entries.forEach(entry => {
      if (entry.isIntersecting) {
        entry.target.classList.add('animate-in');
        observer.unobserve(entry.target);
      }
    });
  }, {
    threshold: 0.1,
    rootMargin: '0px 0px -50px 0px'
  });

  document.querySelectorAll('.scroll-reveal, .slide-in-right').forEach(el => {
    observer.observe(el);
  });

  // Handle window resize for responsive elements
  let resizeTimeout;
  window.addEventListener('resize', () => {
    clearTimeout(resizeTimeout);
    resizeTimeout = setTimeout(() => {
      // Trigger a reflow to ensure responsive elements update properly
      const ipadContainer = document.querySelector('.ipad-container');
      if (ipadContainer) {
        ipadContainer.style.display = 'none';
        ipadContainer.offsetHeight; // Force reflow
        ipadContainer.style.display = '';
      }
    }, 100);
  });
}

// Initialize blog page functionality
function initializeBlogPage() {
  const recordImage = document.getElementById('record-image');

  if (recordImage) {
    let ticking = false;

    function updateRecordRotation() {
      const scrollTop = window.pageYOffset || document.documentElement.scrollTop;
      const rotation = scrollTop * 0.5;
      recordImage.style.transform = `rotate(${rotation}deg)`;
      ticking = false;
    }

    function requestTick() {
      if (!ticking) {
        requestAnimationFrame(updateRecordRotation);
        ticking = true;
      }
    }

    window.addEventListener('scroll', requestTick, { passive: true });
  }
}

// Handle browser back/forward buttons
window.addEventListener('popstate', router);

// Initialize on page load
document.addEventListener('DOMContentLoaded', router);