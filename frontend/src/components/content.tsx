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
    non_ascii2: "Thesis",

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
            "Real-time simulation is essential for LLM-generated actions to harden security. The risk of hallucination remains high for long-range tasks.",
            "Agentic runtime must prioritize high-throughput tool calling, robust state management, and parallelism to achieve performance in financial settlements."
        ],
    },
    {
        h2: "Agentic Software",
        paragraphs: [
            "Corporations need domain-specific AI. Rebuilding workflows with LLM augmentation tends to deliver higher ROI than generic autonomous agents.",
            "As the backbone of business operations, next-gen software will combine deterministic state with probabilistic reasoning while maintaining proprietary context. We call this agentic software.",  
            "Agentic software should be equipped with hybrid compute, routing high-autonomy inference to remote models and low-entropy sub-tasks to local GPUs.",
            "Proprietary tools and context will likely be the moat of AI startups, while revenue-per-token and tokens-per-cost will be the key benchmark."
        ]
    }
];

export const blogs: BlogEntry[] = [
    {
        slug: "stateless-execution-agentic-software",
        eyebrow: "Opinions",
        title: "Stateless Execution and Agentic Software",
        description: "Review crypto's failure modes in the context of agents, and how shifting focus to stateless execution puts us on a better track in the technology growth cycle.",
        body: "Stateless execution reframes how agentic software can remain deterministic while still benefiting from probabilistic language models. We explore the trade-offs across client engineering, transaction safety, and context compilation—highlighting why the stateless model offers better upgradability and sharper risk envelopes for autonomous systems.",
        imageSrc: "/assets/images/3.jpg",
        imageAlt: "Abstract rendering of blockchain nodes connected by light trails",
        publishedAt: "2024-08-11",
        cta: {
            label: "Read manifesto",
            href: "https://aomi-blogs.notion.site/stateless-execution-and-agentic-software"
        }
    },
    {
        slug: "from-aomis-to-llm-infrastructure",
        eyebrow: "Build Notes",
        title: "From Brittle aomis to LLM Infrastructure",
        description: "How we evolve to native execution support in blockchain light clients, optimized with context compilation and type safety in LLM processing.",
        body: "LLM infrastructure demands stronger guarantees than brittle abstractions can provide. In this post we break down our compiler-inspired approach to intent capture, the routing mesh that sits between wallet agents and chain simulators, and the instrumentation that keeps the whole pipeline observable.",
        imageSrc: "/assets/images/4.jpg",
        imageAlt: "Minimal chart illustration",
        publishedAt: "2024-09-04",
        cta: {
            label: "Read build notes",
            href: "https://aomi-blogs.notion.site/from-brittle-aomis-to-llm-infrastructure"
        }
    }
];
