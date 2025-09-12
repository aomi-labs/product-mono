import { ChatComponent } from './ChatComponent';

export class TerminalLanding {
  private container: HTMLElement;
  private chatComponent: ChatComponent | null = null;
  private _activeTab: string = 'chatbot';
  
  constructor(container: HTMLElement) {
    this.container = container;
    this.init();
  }

  private init(): void {
    this.render();
    this.attachEventListeners();
    this.initializeChatbot();
  }

  private render(): void {
    this.container.innerHTML = `
      <!-- Top Navigation Header -->
      <header class="top-header">
        <div class="header-container">
          <div class="logo-section">
            <img src="/aomi-logo.svg" alt="Aomi" class="aomi-logo" />
          </div>
          <nav class="header-nav">
            <a href="https://github.com/aomi-labs" target="_blank" class="nav-link">Github ↗</a>
          </nav>
        </div>
      </header>

      <div class="terminal-container">
        <!-- Terminal Window -->
        <div class="terminal-window">
          <!-- Terminal Header -->
          <div class="terminal-header">
            <div class="window-controls">
              <div class="window-control red"></div>
              <div class="window-control yellow"></div>
              <div class="window-control green"></div>
            </div>
            <div class="terminal-title">AOMI Development Terminal</div>
            <div class="wallet-connection">
              <div class="wallet-status" id="wallet-status">Disconnected</div>
              <button class="wallet-btn" id="wallet-btn">Connect Wallet</button>
            </div>
          </div>

          <!-- Terminal Tabs -->
          <div class="terminal-tabs">
            <button class="terminal-tab" data-tab="anvil" id="anvil-tab">
              ⚡ Anvil Node
            </button>
            <button class="terminal-tab active" data-tab="chatbot" id="chatbot-tab">
              🤖 Chatbot + Claude Implementation
            </button>
          </div>

          <!-- Terminal Content -->
          <div class="terminal-content">
            <!-- Anvil Tab -->
            <div class="tab-pane" id="anvil-pane">
              <div class="anvil-panel">
                <div class="anvil-status">
                  <div class="status-indicator"></div>
                  <div class="anvil-info">
                    <h4>Mainnet Fork Active</h4>
                    <p>Safe testing environment - No real funds at risk</p>
                  </div>
                </div>
                <div class="anvil-logs">
                  <div class="log-entry info">
                    <span class="log-level">[INFO]</span>
                    <span class="log-time">19:34:12</span>
                    <span class="log-msg">Anvil node started on localhost:8545</span>
                  </div>
                  <div class="log-entry success">
                    <span class="log-level">[SUCCESS]</span>
                    <span class="log-time">19:34:13</span>
                    <span class="log-msg">Mainnet fork synced at block 18,500,000</span>
                  </div>
                  <div class="log-entry info">
                    <span class="log-level">[INFO]</span>
                    <span class="log-time">19:34:14</span>
                    <span class="log-msg">10 test accounts funded with 10,000 ETH each</span>
                  </div>
                  <div class="log-entry info">
                    <span class="log-level">[INFO]</span>
                    <span class="log-time">19:34:15</span>
                    <span class="log-msg">RPC endpoint: http://localhost:8545</span>
                  </div>
                  <div class="log-entry success">
                    <span class="log-level">[SUCCESS]</span>
                    <span class="log-time">19:34:16</span>
                    <span class="log-msg">Ready for contract deployment and testing</span>
                  </div>
                </div>
              </div>
            </div>

            <!-- Chatbot Tab -->
            <div class="tab-pane active" id="chatbot-pane">
              <div class="chat-logs-split">
                <!-- Chat Panel -->
                <div class="chat-panel">
                  <div class="chat-messages" id="chat-messages">
                    <div class="chat-message agent">
                      <span class="sender">AOMI</span>
                      <span class="content">Welcome! I'm your AI blockchain development assistant. I can help you build, test, and deploy smart contracts on our safe Anvil testnet environment.</span>
                      <span class="timestamp">19:34:10</span>
                    </div>
                    <div class="chat-message agent">
                      <span class="sender">AOMI</span>
                      <span class="content">Try asking me to: "Create a simple ERC20 token" or "Deploy a multisig wallet" - I'll handle the implementation!</span>
                      <span class="timestamp">19:34:11</span>
                    </div>
                  </div>
                  <div class="chat-input-container">
                    <input type="text" class="chat-input" id="chat-input" placeholder="Ask me to build something..." />
                    <button class="chat-send-btn" id="chat-send-btn">Send</button>
                  </div>
                </div>

                <!-- Claude Logs Panel -->
                <div class="logs-panel">
                  <div class="panel-header" style="padding: 12px 16px; border-bottom: 1px solid var(--terminal-border); background: rgba(255,255,255,0.02);">
                    <h4 style="margin: 0; color: var(--terminal-cyan); font-size: 14px;">Claude Code Implementation</h4>
                  </div>
                  <div class="claude-logs" id="claude-logs">
                    <div class="log-entry info">
                      <span class="log-level">[PLAN]</span>
                      <span class="log-msg">System initialized - Ready for development requests</span>
                    </div>
                    <div class="log-entry info">
                      <span class="log-level">[TOOLS]</span>
                      <span class="log-msg">Available: Solidity compiler, Foundry, OpenZeppelin contracts</span>
                    </div>
                    <div class="log-entry success">
                      <span class="log-level">[STATUS]</span>
                      <span class="log-msg">Anvil testnet connected - Safe deployment environment ready</span>
                    </div>
                  </div>
                </div>
              </div>
            </div>
          </div>
        </div>

        <!-- ASCII Art Section -->
        <div class="ascii-art-section">
          <div class="ascii-art" id="ascii-art">
 █████╗  ██████╗ ███╗   ███╗██╗
██╔══██╗██╔═══██╗████╗ ████║██║
███████║██║   ██║██╔████╔██║██║
██╔══██║██║   ██║██║╚██╔╝██║██║
██║  ██║╚██████╔╝██║ ╚═╝ ██║██║
╚═╝  ╚═╝ ╚═════╝ ╚═╝     ╚═╝╚═╝
          </div>
          <div class="ascii-tagline">AI-Powered Blockchain Development</div>
        </div>

        <!-- Content Sections -->
        <div class="content-sections">
          <!-- Intro Section -->
          <div class="content-section" id="intro-section">
            <h2>Introduction</h2>
            <p>AOMI is your AI-powered companion for blockchain development. We combine the intelligence of Claude Code with the power of Ethereum development tools to create a seamless development experience.</p>
            <p>Chat with our AI to build, test, and deploy smart contracts without the complexity. From simple tokens to complex DeFi protocols, AOMI handles the heavy lifting while you focus on innovation.</p>
            <ul>
              <li>Natural language smart contract development</li>
              <li>Safe testing environment with Anvil mainnet fork</li>
              <li>Real-time implementation with Claude Code</li>
              <li>Gas optimization and security best practices</li>
            </ul>
          </div>

          <!-- Visions Section -->
          <div class="content-section" id="visions-section">
            <h2>Vision</h2>
            <p>We envision a future where blockchain development is accessible to everyone, not just seasoned Solidity developers. AOMI democratizes smart contract creation through conversational AI.</p>
            
            <h3>Our Goals</h3>
            <ul>
              <li>Lower the barrier to entry for blockchain development</li>
              <li>Accelerate the pace of DeFi innovation</li>
              <li>Ensure security and best practices by default</li>
              <li>Create an educational platform for learning Web3 development</li>
            </ul>

            <h3>Technical Vision</h3>
            <ul>
              <li>Integration with major blockchain networks and L2 solutions</li>
              <li>Advanced smart contract auditing and optimization</li>
              <li>Cross-chain deployment capabilities</li>
              <li>Community-driven contract templates and patterns</li>
            </ul>
          </div>

          <!-- Journey Section -->
          <div class="content-section" id="journey-section">
            <h2>Our Journey</h2>
            <p>From concept to reality, AOMI represents months of research, development, and testing to create the most intuitive blockchain development experience.</p>

            <h3>Development Timeline</h3>
            <ul>
              <li><strong>Q3 2024:</strong> Initial concept and architecture design</li>
              <li><strong>Q4 2024:</strong> Core AI integration and smart contract templating</li>
              <li><strong>Q1 2025:</strong> Anvil integration and safe testing environment</li>
              <li><strong>Current:</strong> Public beta launch and community feedback</li>
            </ul>

            <h3>What's Next</h3>
            <ul>
              <li>Multi-chain support (Polygon, Arbitrum, Base)</li>
              <li>Advanced DeFi protocol templates</li>
              <li>Integration with popular Web3 tooling</li>
              <li>Community marketplace for AI-generated contracts</li>
            </ul>
          </div>

          <!-- Conversations Section -->
          <div class="content-section" id="conversations-section">
            <h2>Join the Conversation</h2>
            <p>Connect with our growing community of AI-powered blockchain developers. Share your creations, get help, and shape the future of Web3 development.</p>

            <h3>Community Links</h3>
            <ul>
              <li><a href="#" onclick="showPlaceholder('Discord')">Discord Community</a> - Chat with other AOMI developers</li>
              <li><a href="#" onclick="showPlaceholder('Twitter')">Twitter</a> - Follow us for updates and announcements</li>
              <li><a href="#" onclick="showPlaceholder('GitHub')">GitHub Repository</a> - Contribute to the open source project</li>
              <li><a href="#" onclick="showPlaceholder('Documentation')">Documentation</a> - Comprehensive guides and tutorials</li>
            </ul>

            <h3>Support & Contact</h3>
            <ul>
              <li><a href="#" onclick="showPlaceholder('Help')">Help Center</a> - FAQs and troubleshooting guides</li>
              <li><a href="#" onclick="showPlaceholder('Email')">Contact Us</a> - Get in touch with our team</li>
              <li><a href="#" onclick="showPlaceholder('Bug Report')">Report a Bug</a> - Help us improve AOMI</li>
            </ul>
          </div>
        </div>
      </div>
    `;
  }

  private attachEventListeners(): void {
    // Tab switching
    const tabs = document.querySelectorAll('.terminal-tab');
    tabs.forEach(tab => {
      tab.addEventListener('click', (e) => {
        const target = e.target as HTMLButtonElement;
        const tabName = target.dataset.tab;
        if (tabName) {
          this.switchTab(tabName);
        }
      });
    });

    // Wallet connection
    const walletBtn = document.getElementById('wallet-btn');
    walletBtn?.addEventListener('click', this.handleWalletConnect.bind(this));

    // Chat input
    const chatInput = document.getElementById('chat-input') as HTMLInputElement;
    const chatSendBtn = document.getElementById('chat-send-btn');
    
    chatInput?.addEventListener('keypress', (e) => {
      if (e.key === 'Enter') {
        this.handleChatSend();
      }
    });

    chatSendBtn?.addEventListener('click', this.handleChatSend.bind(this));

    // Window control animations
    const controls = document.querySelectorAll('.window-control');
    controls.forEach(control => {
      control.addEventListener('click', (e) => {
        const target = e.target as HTMLElement;
        if (target.classList.contains('red')) {
          this.minimizeTerminal();
        } else if (target.classList.contains('yellow')) {
          this.toggleMinimize();
        } else if (target.classList.contains('green')) {
          this.maximizeTerminal();
        }
      });
    });
  }

  private switchTab(tabName: string): void {
    // Update active tab
    document.querySelectorAll('.terminal-tab').forEach(tab => {
      tab.classList.remove('active');
    });
    document.getElementById(`${tabName}-tab`)?.classList.add('active');

    // Update active pane
    document.querySelectorAll('.tab-pane').forEach(pane => {
      pane.classList.remove('active');
    });
    document.getElementById(`${tabName}-pane`)?.classList.add('active');

    this._activeTab = tabName;

    // Initialize chatbot if switching to chatbot tab
    if (tabName === 'chatbot' && !this.chatComponent) {
      this.initializeChatbot();
    }
  }

  private initializeChatbot(): void {
    // This will be integrated with the existing ChatComponent
    // For now, we'll simulate the chat functionality
    console.log('Initializing chatbot component...');
  }

  private handleWalletConnect(): void {
    const walletBtn = document.getElementById('wallet-btn') as HTMLButtonElement;
    const walletStatus = document.getElementById('wallet-status') as HTMLElement;
    
    // Simulate wallet connection
    walletBtn.textContent = 'Connecting...';
    walletBtn.disabled = true;
    walletStatus.textContent = 'Connecting...';

    setTimeout(() => {
      walletBtn.textContent = '0x1234...5678';
      walletBtn.disabled = false;
      walletStatus.textContent = 'Connected';
      walletStatus.style.color = 'var(--terminal-green)';
    }, 2000);
  }

  private handleChatSend(): void {
    const chatInput = document.getElementById('chat-input') as HTMLInputElement;
    const message = chatInput.value.trim();
    
    if (!message) return;

    // Add user message to chat
    this.addChatMessage('user', message);
    chatInput.value = '';

    // Add implementation log
    this.addClaudeLog('info', `Processing request: "${message.substring(0, 50)}..."`);

    // Simulate AI response
    setTimeout(() => {
      this.addChatMessage('agent', 'I understand you want me to help with that. Let me break this down into steps and implement it for you.');
      this.addClaudeLog('success', 'Request analyzed - Beginning implementation');
    }, 1000);
  }

  private addChatMessage(sender: 'user' | 'agent', content: string): void {
    const messagesContainer = document.getElementById('chat-messages');
    if (!messagesContainer) return;

    const timestamp = new Date().toLocaleTimeString();
    const messageHtml = `
      <div class="chat-message ${sender}">
        <span class="sender">${sender === 'user' ? 'You' : 'AOMI'}</span>
        <span class="content">${content}</span>
        <span class="timestamp">${timestamp}</span>
      </div>
    `;

    messagesContainer.insertAdjacentHTML('beforeend', messageHtml);
    messagesContainer.scrollTop = messagesContainer.scrollHeight;
  }

  private addClaudeLog(level: 'info' | 'success' | 'warning' | 'error', message: string): void {
    const logsContainer = document.getElementById('claude-logs');
    if (!logsContainer) return;

    const logHtml = `
      <div class="log-entry ${level}">
        <span class="log-level">[${level.toUpperCase()}]</span>
        <span class="log-msg">${message}</span>
      </div>
    `;

    logsContainer.insertAdjacentHTML('beforeend', logHtml);
    logsContainer.scrollTop = logsContainer.scrollHeight;
  }

  private minimizeTerminal(): void {
    const terminal = document.querySelector('.terminal-window') as HTMLElement;
    terminal.style.transform = 'scale(0.8)';
    terminal.style.opacity = '0.8';
    setTimeout(() => {
      terminal.style.transform = 'scale(1)';
      terminal.style.opacity = '1';
    }, 300);
  }

  private toggleMinimize(): void {
    const content = document.querySelector('.terminal-content') as HTMLElement;
    content.style.display = content.style.display === 'none' ? 'block' : 'none';
  }

  private maximizeTerminal(): void {
    const terminal = document.querySelector('.terminal-window') as HTMLElement;
    terminal.style.transform = 'scale(1.02)';
    setTimeout(() => {
      terminal.style.transform = 'scale(1)';
    }, 200);
  }
}

// Global function for placeholder links
(window as any).showPlaceholder = function(linkName: string) {
  alert(`Coming soon: ${linkName} integration will be available in the next update!`);
};