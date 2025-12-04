// Data model for a chat message
class ChatMessage {
  role string // "user" or "assistant"
  content string
}

// Data model for conversation summary
class ConversationSummary {
  // High-level topic or goal of the conversation (e.g., "Token swap on Uniswap", "NFT minting on Base")
  main_topic string

  // Specific details about what was discussed (e.g., "100 USDC to ETH", "deploying on Arbitrum")
  key_details string[]

  // Current state: where did the user leave off? What were they trying to accomplish?
  current_state string

  // A natural language summary to present to the user (1-2 sentences)
  user_friendly_summary string
}

// Function to summarize a blockchain-related conversation from message history
function SummarizeConversation(messages: ChatMessage[]) -> ConversationSummary {
  client "anthropic/claude-3-5-haiku-20241022"
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
  functions [SummarizeConversation]
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
  functions [SummarizeConversation]
  args {
    messages [
      {role: "user", content: "I need to deploy an ERC20 token on Arbitrum"},
      {role: "assistant", content: "I can help you deploy an ERC20 token contract. What should the token name and symbol be?"},
      {role: "user", content: "Name: MyToken, Symbol: MTK, supply: 1000000"}
    ]
  }
}
