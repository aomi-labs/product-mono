import './input.css'
import { Button, ChatContainer, TextSection, AnvilLogContainer, content } from './components.js'
import { ChatManager } from './utils/ChatManager.js'
import { AnvilManager } from './utils/AnvilManager.js'
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
    // Restore chat container
    terminalContent.innerHTML = `
      <div class="h-full flex flex-col bg-slate-900 rounded-b-xl overflow-hidden">
        <div id="terminal-messages-container" class="flex-1 p-4 overflow-y-auto scrollbar-dark">
          <!-- Messages will be populated here -->
        </div>
        <div class="border-t border-gray-700 p-4">
          <div class="flex items-center space-x-3">
            <div class="flex items-center space-x-2 text-gray-400 text-xs">
              <span>></span>
              <span>🔧</span>
              <span>📍</span>
              <span>Auto</span>
              <span>|</span>
              <span>📁</span>
              <span>🎤</span>
              <span>📎</span>
              <span>📧</span>
            </div>
            <input
              id="terminal-message-input"
              type="text"
              placeholder="type a message..."
              class="flex-1 bg-transparent border-none outline-none text-white text-sm placeholder-gray-500"
            />
            <div class="flex items-center space-x-2 text-xs text-gray-400">
              <span>auto (claude 4 sonnet)</span>
              <button class="hover:text-white">⬇️</button>
            </div>
          </div>
        </div>
      </div>
    `;
    // Reinitialize chat after content is loaded only if we're still on chat tab
    setTimeout(() => {
      if (currentTab === 'chat') {
        initializeChat();
      }
    }, 100);
  } else if (currentTab === 'readme') {
    // Show README content
    terminalContent.innerHTML = `
      <div class="h-full p-6 bg-slate-900 text-green-400 font-mono text-sm overflow-y-auto scrollbar-dark">
        <div class="space-y-4">
          <div class="text-lime-400 font-bold">README.md</div>
          <div class="text-gray-300">
            <p class="mb-4"># Aomi Labs</p>
            <p class="mb-4">A research and engineering group focused on building agentic software for blockchain automation.</p>
            <p class="mb-4">## Features</p>
            <ul class="ml-4 space-y-1 list-disc">
              <li>Transaction pipeline automation</li>
              <li>Chain-agnostic guardrails for LLMs</li>
              <li>Performance, scalability, and predictability</li>
              <li>Real-time blockchain monitoring</li>
            </ul>
            <p class="mt-4">## Get Started</p>
            <p class="text-blue-400">Click the 'chat' tab to interact with our AI assistant or 'anvil' to monitor blockchain activity.</p>
          </div>
        </div>
      </div>
    `;
  } else if (currentTab === 'anvil') {
    // Show anvil content
    terminalContent.innerHTML = anvilLogContainer ? anvilLogContainer.render() : new AnvilLogContainer().render();
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

function getReadmeContent() {
  return `
    <div class="h-full bg-slate-900 flex flex-col">
      <div class="flex-1 p-4 overflow-y-auto overflow-x-hidden font-mono scrollbar-dark text-gray-300 text-sm">
        <h1 class="text-white text-lg mb-4">🔧 Aomi Labs - EVM Chatbot</h1>

        <h2 class="text-blue-400 text-base mb-2">Features</h2>
        <ul class="list-disc list-inside mb-4 space-y-1">
          <li>AI-powered Ethereum operations assistant</li>
          <li>Smart contract interaction via natural language</li>
          <li>Real-time blockchain monitoring</li>
          <li>Anvil local node integration</li>
        </ul>

        <h2 class="text-blue-400 text-base mb-2">Getting Started</h2>
        <div class="bg-gray-800 p-3 rounded mb-4 font-mono text-xs">
          <p class="text-green-400"># Start the backend</p>
          <p>cargo run --bin backend</p>
          <p class="text-green-400 mt-2"># Start Anvil (optional)</p>
          <p>anvil --port 8545</p>
        </div>

        <h2 class="text-blue-400 text-base mb-2">Tabs</h2>
        <ul class="list-disc list-inside mb-4 space-y-1">
          <li><strong>Chat:</strong> Interact with the AI assistant</li>
          <li><strong>Anvil:</strong> Monitor local Ethereum node activity</li>
          <li><strong>README:</strong> This documentation</li>
        </ul>

        <p class="text-gray-400 text-xs mt-8">Built with ❤️ by Aomi Labs</p>
      </div>
    </div>
  `;
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
    // 2. It's the last message AND the bot is not typing (finished)
    const isLastMessage = index === state.messages.length - 1;
    const showBorder = !isLastMessage || (isLastMessage && !state.isTyping);
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