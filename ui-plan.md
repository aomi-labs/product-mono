# AOMI Landing Page UI Plan

## Overview
A terminal-styled landing page that embeds a chat interface with a developer-friendly aesthetic. The page showcases our AI-powered blockchain development platform with real-time interaction capabilities.

## Layout Structure

### Header Terminal Window
- **Style**: Dark terminal window with rounded corners and window controls (red, yellow, green dots)
- **Position**: Center of viewport, large enough to be the main focus
- **Colors**: Dark background (#1a1a1a), green text (#00ff00), amber accents (#ffb000)

### Upper Right Corner
- **Wallet Connection**: Button/widget to connect user's crypto wallet
- **Style**: Minimal, terminal-consistent design
- **States**: Disconnected, Connecting, Connected (with address truncation)

### Terminal Tabs
Two main tabs at the top of terminal window:

1. **Left Tab: "Anvil"**
   - Display launched Anvil node status
   - Show forked mainnet information
   - Real-time node logs/status
   - Safety message about using test funds

2. **Right Tab: "Chatbot + Claude Implementation Plan"** (Default active)
   - **Left Side**: Chat interface for user interaction
   - **Right Side**: Claude Code implementation logs
   - Split view within the tab
   - Real-time streaming of both chat and implementation steps

### ASCII Art Branding
- **Position**: Below terminal window, centered
- **Text**: "aomi" in large ASCII art style
- **Style**: Monospace font, consistent with terminal theme

### Content Sections
Below ASCII art, arranged vertically:

1. **Intro**
   - Brief description of AOMI
   - What makes it unique
   - Call-to-action to try the chat

2. **Visions**
   - Our goals and aspirations
   - Future of AI-powered blockchain development
   - Technical vision and roadmap

3. **Journey**
   - Development timeline
   - Milestones achieved
   - Current progress and next steps

4. **Conversations**
   - Links to documentation, GitHub, social media
   - Community resources
   - Contact information
   - For now: placeholder links

## Technical Requirements

### Responsive Design
- Mobile-first approach
- Terminal window scales appropriately
- Tabs collapse to dropdown on smaller screens
- Content sections stack properly

### Real-time Features
- WebSocket connection to backend for chat
- Live streaming of Claude implementation logs
- Anvil node status updates
- Connection status indicators

### State Management
- Chat history persistence
- Tab state management
- Wallet connection state
- Terminal window focus states

### Performance
- Lazy loading for content sections
- Efficient WebSocket message handling
- Smooth animations and transitions
- Terminal text streaming effects

## Implementation Priority
1. Basic terminal window layout
2. Tab system with chat interface
3. Wallet connection UI
4. ASCII art and branding
5. Content sections with placeholder content
6. Real-time features and WebSocket integration
7. Responsive design refinements
8. Polish and animations

## Design References
- Classic terminal emulators (xterm, iTerm2)
- Developer tools aesthetics
- Matrix/cyberpunk terminal themes
- Modern web terminal libraries


<div id="about" class="scroll-reveal scroll-reveal-delay-1 self-stretch text-center text-black text-sm font leading-[10px]">                                     ███ </div>
<div id="about" class="scroll-reveal scroll-reveal-delay-1 self-stretch text-center text-black text-sm font leading-[10px]">                                    ░░░  </div>
<div id="about" class="scroll-reveal scroll-reveal-delay-1 self-stretch text-center text-black text-sm font leading-[10px]">  ██████    ██████  █████████████   ████ </div>
<div id="about" class="scroll-reveal scroll-reveal-delay-1 self-stretch text-center text-black text-sm font leading-[10px]"> ░░░░░███  ███░░███░░███░░███░░███ ░░███ </div>
<div id="about" class="scroll-reveal scroll-reveal-delay-1 self-stretch text-center text-black text-sm font leading-[10px]">  ███████ ░███ ░███ ░███ ░███ ░███  ░███ </div>
<div id="about" class="scroll-reveal scroll-reveal-delay-1 self-stretch text-center text-black text-sm font leading-[10px]"> ███░░███ ░███ ░███ ░███ ░███ ░███  ░███ </div>
<div id="about" class="scroll-reveal scroll-reveal-delay-1 self-stretch text-center text-black text-sm font leading-[10px]">░░████████░░██████  █████░███ █████ █████</div>
<div id="about" class="scroll-reveal scroll-reveal-delay-1 self-stretch text-center text-black text-sm font leading-[10px]"> ░░░░░░░░  ░░░░░░  ░░░░░ ░░░ ░░░░░ ░░░░░ </div>

                                     ███ 
                                    ░░░  
  ██████    ██████  █████████████   ████ 
 ░░░░░███  ███░░███░░███░░███░░███ ░░███ 
  ███████ ░███ ░███ ░███ ░███ ░███  ░███ 
 ███░░███ ░███ ░███ ░███ ░███ ░███  ░███ 
░░████████░░██████  █████░███ █████ █████
 ░░░░░░░░  ░░░░░░  ░░░░░ ░░░ ░░░░░ ░░░░░