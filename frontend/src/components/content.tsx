import { BlogEntry } from "@/lib/types";

// Content Data
export const content = {
    intro: {
        title: "Consumer Crypto on Natural Language",
        description: "Aomi Labs is a research and engineering group that builds agentic software. We focus on transaction pipeline automation for public blockchains, developing chain-agnostic guardrails for LLMs to generate transactions with performance, scalability, and security."
    },
    ascii: ` ▄▄▄·       • ▌ ▄ ·. ▪
    ▐█ ▀█ ▪     ·██ ▐███▪██
    ▄█▀▀█  ▄█▀▄ ▐█ ▌▐▌▐█·▐█·
    ▐█ ▪▐▌▐█▌.▐▌██ ██▌▐█▌▐█▌
    ▀  ▀  ▀█▄▀▪▀▀  █▪▀▀▀▀▀▀`,

    ascii2: `▄▄▄█████▓ ██░ ██ ▓█████   ██████  ██▓  ██████ 
▓  ██▒ ▓▒▓██░ ██▒▓█   ▀ ▒██    ▒ ▓██▒▒██    ▒ 
▒ ▓██░ ▒░▒██▀▀██░▒███   ░ ▓██▄   ▒██▒░ ▓██▄   
░ ▓██▓ ░ ░▓█ ░██ ▒▓█  ▄   ▒   ██▒░██░  ▒   ██▒
  ▒██▒ ░ ░▓█▒░██▓░▒████▒▒██████▒▒░██░▒██████▒▒
  ▒ ░░    ▒ ░░▒░▒░░ ▒░ ░▒ ▒▓▒ ▒ ░░▓  ▒ ▒▓▒ ▒ ░
    ░     ▒ ░▒░ ░ ░ ░  ░░ ░▒  ░ ░ ▒ ░░ ░▒  ░ ░
  ░       ░  ░░ ░   ░   ░  ░  ░   ▒ ░░  ░  ░  
          ░  ░  ░   ░  ░      ░   ░        ░  `,

    ascii3: ` ▄▄▄▄    ██▓     ▒█████    ▄████ 
▓█████▄ ▓██▒    ▒██▒  ██▒ ██▒ ▀█▒
▒██▒ ▄██▒██░    ▒██░  ██▒▒██░▄▄▄░
▒██░█▀  ▒██░    ▒██   ██░░▓█  ██▓
░▓█  ▀█▓░██████▒░ ████▓▒░░▒▓███▀▒
░▒▓███▀▒░ ▒░▓  ░░ ▒░▒░▒░  ░▒   ▒ 
▒░▒   ░ ░ ░ ▒  ░  ░ ▒ ▒░   ░   ░ 
 ░    ░   ░ ░   ░ ░ ░ ▒  ░ ░   ░ 
 ░          ░  ░    ░ ░        ░ 
      ░                          `,
    
  conclusion: "Blockchains are a proving ground for practical AI automation. Structured on-chain data combined with chaotic market signals is best handled by state-bound agentic software. High-frequency trading and financial settlement stress-test security-critical, cost-aware intelligence."

};

export const bodies = [
    {
        h2: "Blockchain Architecture",
        paragraphs: [
            "Treat AI frameworks as a system substrate for blockchain clients. Low-level abstractions of agentic operations should remain protocol-agnostic, without involving bespoke interfaces.",
            "A deep execution layer per architecture, including EVM, Solana, Cosmos, allows seamless interoperability of intents across chains.",
            "Real-time simulation is essential LLM-generated actions to harden security. The risk of hullucination remains high for long-range tasks.",
            "Agentic runtime must prioritize high-throughput tool calling, robust state management, and parallelism to achieve performance in financial settlements."
        ],
    },
    {
        h2: "Agentic Software",
        paragraphs: [
            "Coorperations need domain-specific AI. Rebuilding workflows with LLM augmentation tend to bring higher ROI than generic autonomous agents.",
            "As the backbone of business operation, next-gen software will combine deterministic state with probabilistic reasoning, maintaining propreiatory context. We call this agentic software.",  
            "Agentic software should be equipped with hybrid compute, routing high-autonomy inference to remote models and low-entropy sub-tasks to local GPUs.",
            "Proprietary tools and context will likely be the moat of AI startups, while revenue-per-token and tokens-per-cost will be the key benchmark."
        ]
    }
];

export const blogs: BlogEntry[] = [
    {
        eyebrow: "Research Journal",
        title: "Formalizing Agentic Guardrails for On-Chain Execution",
        description: "How we stress-test transaction policies against adversarial prompts, combining symbolic analysis with real-time market data to keep LLM intents safe on mainnet.",
        imageSrc: "/assets/images/blured.png",
        imageAlt: "Abstract rendering of blockchain nodes connected by light trails"
    },
    {
        eyebrow: "Build Notes",
        title: "Observability Lessons from Scaling the Aomi Backend",
        description: "Tracing multi-chain workflows across Rust services and the chat gateway, and the instrumentation we deploy to keep latency predictable for power users.",
        imageSrc: "/assets/images/chart_icon.png",
        imageAlt: "Minimal chart illustration"
    },
    {
        eyebrow: "Product Update",
        title: "Introducing Adaptive Wallet Simulation",
        description: "A walkthrough of our new simulation layer that auto-selects the optimal chain fork and gas regime before the agent commits a transaction on behalf of the user.",
        imageSrc: "/assets/images/earth_icon.png",
        imageAlt: "Stylized planet icon"
    }
];
