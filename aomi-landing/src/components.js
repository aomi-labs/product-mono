// UI Component Classes for Aomi Landing Page

// Content Data
export const content = {
  intro: {
    title: "Consumer Crypto on Natural Language",
    description: "Aomi Labs is a research and engineering group that builds agentic software. We focus on transaction pipeline automation for public blockchains, developing chain-agnostic guardrails for LLMs to generate transactions with performance, scalability, and predictability."
  },
  ascii: ` ▄▄▄·       • ▌ ▄ ·. ▪
▐█ ▀█ ▪     ·██ ▐███▪██
▄█▀▀█  ▄█▀▄ ▐█ ▌▐▌▐█·▐█·
▐█ ▪▐▌▐█▌.▐▌██ ██▌▐█▌▐█▌
 ▀  ▀  ▀█▄▀▪▀▀  █▪▀▀▀▀▀▀`,
  chat: {
    messages: [
      {
        type: 'user',
        content: 'hello'
      },
      {
        type: 'assistant',
        content: `Hello! I'm here to help you with software development tasks in the terminal. I can assist with:

• Running commands and scripts
• Reading and editing code files
• Searching through codebases
• Version control operations
• And much more!

I can see you have several indexed codebases available:
• foameow at /Users/cecillazhang/Code/hiring/foameow
• mcp-server at /Users/cecillazhang/Code/hiring/mcp-server
• rtg-repl at /Users/cecillazhang/Code/hiring/rtg-repl

What would you like to work on today?`
      }
    ]
  }
};

// Button Component Class
export class Button {
  constructor(text, type = 'default', options = {}) {
    this.text = text;
    this.type = type;
    this.options = options;
  }

  render() {
    const baseClass = 'text-sm rounded-[4px] border transition-colors';
    const variants = {
      'tab-inactive': 'px-3 py-0.5 w-[130px] h-6 bg-gray-700 text-gray-300 text-xs border-gray-600 border-0.2 hover:bg-gray-600 hover:text-white',
      'tab-active': 'px-3 py-0.5 w-[130px] h-6 bg-gray-500 text-white  text-xs border-gray-500 border-0.2 hover:bg-gray-500',
      'github': 'px-4 py-3 bg-black text-white rounded-full hover:bg-gray-800',
      'terminal-connect': 'bg-green-600 text-white px-3 py-1 text-xs rounded-lg border-0 h-6 hover:bg-green-500',
    };

    const classes = `${baseClass} ${variants[this.type] || variants.default}`;

    if (this.type === 'tab-active' && this.options.indicator) {
      return `<button class="${classes}">
        <span class="w-2 h-2 bg-green-400 rounded-full"></span>
        <span>${this.text}</span>
      </button>`;
    }

    return `<button class="${classes}">${this.text}</button>`;
  }
}

// Anvil Log Container Component Class
export class AnvilLogContainer {
  constructor() {
    this.logs = [];
  }

  updateLogs(logs) {
    this.logs = logs;
  }

  render() {
    const logsHtml = this.logs.map(log => {
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
    }).join('');

    return `
      <div class="h-full bg-slate-900 flex flex-col">
        <div class="flex-1 p-4 overflow-y-auto overflow-x-hidden font-mono scrollbar-dark" id="anvil-logs-container">
          ${logsHtml || '<div class="text-gray-500 text-xs">No logs yet. Start Anvil to see activity...</div>'}
        </div>
        <div class="px-4 py-2 border-t border-gray-700">
          <div class="flex items-center justify-between text-xs text-gray-400">
            <span>Anvil Node Monitor</span>
            <button id="clear-anvil-logs" class="text-gray-400 hover:text-white px-2 py-1 rounded hover:bg-gray-700">
              Clear
            </button>
          </div>
        </div>
      </div>
    `;
  }
}

// Message Component Class
export class Message {
  constructor(type, content) {
    this.type = type;
    this.content = content;
  }

  render() {
    const icon = this.type === 'user' ? '➜' : '🤖';
    const iconColor = this.type === 'user' ? 'text-blue-400' : 'text-green-400';
    const textColor = this.type === 'user' ? 'text-white' : 'text-gray-300';

    const formattedContent = this.formatContent(this.content);

    return `
      <div class="chat-array mb-4 pb-2 border-b border-gray-700/50">
        <div class="flex items-start space-x-3">
          <span class="${iconColor} text-md">${icon}</span>
          <div class="${textColor} text-[11px] space-y-2 py-1 leading-relaxed">
            ${formattedContent}
          </div>
        </div>
      </div>
    `;
  }

  formatContent(content) {
    if (this.type === 'user') {
      return `<span>${content}</span>`;
    }

    // Format assistant messages with lists and paragraphs
    const lines = content.split('\n').filter(line => line.trim());
    let formatted = '';
    let inList = false;

    for (const line of lines) {
      if (line.startsWith('•')) {
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
}

// Terminal Input Component Class
export class TerminalInput {
  constructor() {
    this.prompt = '📁 ~ hello ❯';
    this.placeholder = 'type a message...';
    this.model = 'auto (claude 4 sonnet)';
  }

  render() {
    return `
      <div class="px-2 py-2 font-mono">
        <div class="mb-2 bg-slate-800 border border-slate-600 rounded-md px-3 py-2 focus-within:outline-none focus-within:ring-1 focus-within:ring-blue-500 focus-within:border-blue-500">
          <!-- Top icon row -->
          <div class="flex items-center space-x-3 text-xs text-gray-400 mb-3">
            <span>></span>
            <span class="text-blue-400">🔧</span>
            <span class="text-gray-300">📍</span>
            <span>Auto</span>
            <span class="text-gray-600">|</span>
            <span>📁</span>
            <span>🎤</span>
            <span>📎</span>
            <span>📧</span>
          </div>

          <!-- Rectangular input panel -->
          <div class="mb-3">
            <input
              type="text"
              placeholder="${this.placeholder}"
              class="w-full bg-slate-800 rounded-md px-3 py-1 text-sm text-gray-300 placeholder-gray-500 text-xs focus:outline-none"
              id="terminal-message-input"
            />
          </div>

          <!-- Bottom row with model selector -->
          <div class="flex items-center justify-between">
            <div class="flex items-center space-x-2">
              <span class="text-gray-400 text-xs">${this.model}</span>
              <button class="px-1 py-0.5 rounded-md hover:bg-gray-700 text-xs">⬇️</button>
            </div>
          </div>
        </div>
      </div>
    `;
  }
}

// Chat Container Component Class
export class ChatContainer {
  constructor(messages = []) {
    this.messages = messages;
    this.input = new TerminalInput();
  }

  addMessage(type, content) {
    this.messages.push(new Message(type, content));
  }

  render() {
    const messagesHtml = this.messages.map(msg =>
      msg instanceof Message ? msg.render() : new Message(msg.type, msg.content).render()
    ).join('');

    return `
      <div class="h-full bg-slate-900 flex flex-col">
        <div class="flex-1 p-4 overflow-y-auto overflow-x-hidden font-mono scrollbar-dark" id="terminal-messages-container">
          ${messagesHtml}
        </div>
        ${this.input.render()}
      </div>
    `;
  }
}

// Text Section Component Class
export class TextSection {
  constructor(type, content, options = {}) {
    this.type = type;
    this.content = content;
    this.options = options;
  }

  render() {
    switch (this.type) {
      case 'ascii':
        return `<div class="ascii-art scroll-reveal scroll-reveal-delay-1 text-center font-mono text-sm text-black whitespace-pre">${this.content}</div>`;

      case 'intro-title':
        return `<div id="about" class="scroll-reveal scroll-reveal-delay-1 self-stretch text-center text-black text-6xl font-bold font-['Bauhaus_Chez_Display_2.0'] leading-[54px]">${this.content}</div>`;

      case 'intro-description':
        return `<div class="scroll-reveal scroll-reveal-delay-2 self-stretch text-left text-black text-sm font-light font-['DotGothic16'] tracking-wide">${this.content}</div>`;

      default:
        return `<div>${this.content}</div>`;
    }
  }
}