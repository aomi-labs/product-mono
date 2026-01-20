// Data model for a chat message
class ChatMessage {
  role string // "user" or "assistant"
  content string
}

// Data model for conversation summary
class ConversationSummary {
  // High-level topic or goal of the conversation (e.g., "Token swap on Uniswap", "NFT minting on Base")
  title string

  // Specific details about what was discussed (e.g., "100 USDC to ETH", "deploying on Arbitrum")
  key_details string[]

  // Current state: where did the user leave off? What were they trying to accomplish?
  current_state string

  // A natural language summary to present to the user (1-2 sentences)
  user_friendly_summary string
}

// Data model for session title
class SessionTitle {
  // Concise title for the conversation (3-6 words, e.g., "ETH to USDC Swap", "Deploy ERC20 on Base")
  title string
}

// Function to generate a blockchain conversation summary from message history
function GenerateConversationSummary(messages: ChatMessage[]) -> ConversationSummary {
  client DefaultOpus4
  prompt #"
    You are summarizing a user's previous conversation about blockchain operations.
    This could involve any blockchain network (Ethereum, Base, Arbitrum, Optimism, etc.) and
    various operations like: token swaps, transfers, contract interactions, deployments,
    transaction analysis, balance checks, NFT operations, etc.

    Previous conversation:
    {% for msg in messages %}
    {{ msg.role }}: {{ msg.content }}
    {% endfor %}

    Analyze this conversation and extract:
    1. The main topic or goal (what blockchain operation were they discussing?)
    2. Specific key details (networks, tokens, amounts, addresses, contract names, etc.)
    3. The current state - where did they leave off? What were they trying to accomplish?
    4. A friendly, specific summary to greet them with (mention concrete details like network names, token amounts, etc.)

    Be specific and concrete. Include:
    - Blockchain networks if mentioned (e.g., "on Base", "on Arbitrum")
    - Token names and amounts (e.g., "100 USDC", "0.5 ETH")
    - Contract addresses or names if relevant
    - Specific operations (swap, deploy, transfer, mint, etc.)

    Don't be vague - provide actionable context so the user knows exactly what they were working on.

    {{ ctx.output_format }}
  "#
}

// Test with a sample token swap conversation
test test_swap_conversation {
  functions [GenerateConversationSummary]
  args {
    messages [
      {role: "user", content: "How do I swap tokens on Base?"},
      {role: "assistant", content: "You can swap tokens on Base using Uniswap V3. What tokens would you like to swap?"},
      {role: "user", content: "I want to swap 100 USDC for ETH"},
      {role: "assistant", content: "I can help you with that swap on Base network. Let me check the current rates..."}
    ]
  }
}

// Test with contract deployment conversation
test test_contract_deployment {
  functions [GenerateConversationSummary]
  args {
    messages [
      {role: "user", content: "I need to deploy an ERC20 token on Arbitrum"},
      {role: "assistant", content: "I can help you deploy an ERC20 token contract. What should the token name and symbol be?"},
      {role: "user", content: "Name: MyToken, Symbol: MTK, supply: 1000000"}
    ]
  }
}

// Function to generate a concise title for an active session
function GenerateTitle(messages: ChatMessage[]) -> SessionTitle {
  client DefaultOpus4
  prompt #"
    You are generating a concise title for an active conversation about blockchain operations.

    Current conversation:
    {% for msg in messages %}
    {{ msg.role }}: {{ msg.content }}
    {% endfor %}

    Generate a concise, specific title (3-6 words) that captures what this conversation is about.

    Guidelines:
    - Be specific: mention networks, tokens, or operations if discussed
    - Keep it short: 3-6 words maximum
    - Use title case (e.g., "ETH to USDC Swap on Base")
    - Focus on the main action or topic
    - Examples:
      * "ETH to USDC Swap"
      * "Deploy ERC20 on Arbitrum"
      * "NFT Minting Guide"
      * "Check Wallet Balance"
      * "Uniswap V3 Setup"

    If the conversation is just starting or unclear, use a generic title like:
    - "Blockchain Discussion"
    - "Getting Started"
    - "General Questions"

    {{ ctx.output_format }}
  "#
}

// Test title generation for swap conversation
test test_title_swap {
  functions [GenerateTitle]
  args {
    messages [
      {role: "user", content: "How do I swap tokens on Base?"},
      {role: "assistant", content: "You can swap tokens on Base using Uniswap V3. What tokens would you like to swap?"},
      {role: "user", content: "I want to swap 100 USDC for ETH"}
    ]
  }
}

// Test title generation for deployment
test test_title_deployment {
  functions [GenerateTitle]
  args {
    messages [
      {role: "user", content: "I need to deploy an ERC20 token on Arbitrum"},
      {role: "assistant", content: "I can help you deploy an ERC20 token contract."}
    ]
  }
}
