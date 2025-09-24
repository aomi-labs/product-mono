"use client";

import React, { createContext, useContext, useEffect, useMemo, useRef, useState } from 'react';
import ReactMarkdown from 'react-markdown';
import type { Components } from 'react-markdown';
import remarkGfm from 'remark-gfm';

const CodeBlockContext = createContext(false);

const hashString = (input: string): string => {
  let hash = 0;
  for (let i = 0; i < input.length; i += 1) {
    hash = (hash * 31 + input.charCodeAt(i)) | 0;
  }
  return Math.abs(hash).toString(16);
};

const MermaidDiagram: React.FC<{ code: string }> = ({ code }) => {
  const containerRef = useRef<HTMLDivElement>(null);
  const diagramIdRef = useMemo(() => `mermaid-${Math.random().toString(36).slice(2)}`, []);
  const [isLoaded, setIsLoaded] = useState(false);

  useEffect(() => {
    let cancelled = false;

    const renderMermaid = async () => {
      const container = containerRef.current;
      if (!container || typeof window === 'undefined') {
        return;
      }

      try {
        setIsLoaded(false);
        const mermaidModule = await import('mermaid');
        const mermaid = mermaidModule.default;
        mermaid.initialize({
          startOnLoad: false,
          theme: 'dark',
          themeVariables: {
            primaryColor: '#1e293b',
            primaryTextColor: '#e2e8f0',
            primaryBorderColor: '#475569',
            lineColor: '#64748b',
            secondaryColor: '#334155',
            tertiaryColor: '#0f172a',
            background: '#0f172a',
            mainBkg: '#1e293b',
            secondBkg: '#334155',
            tertiaryBkg: '#475569',
          },
        });

        const { svg } = await mermaid.render(diagramIdRef, code.trim());
        if (cancelled || !containerRef.current) return;

        containerRef.current.innerHTML = svg;
        const svgElement = containerRef.current.querySelector('svg');
        if (svgElement) {
          svgElement.setAttribute('width', '100%');
          svgElement.removeAttribute('height');
          svgElement.style.width = '100%';
          svgElement.style.maxWidth = '100%';
          svgElement.style.height = 'auto';
          svgElement.setAttribute('preserveAspectRatio', 'xMinYMin meet');
        }

        setIsLoaded(true);
      } catch (error) {
        console.error('Mermaid rendering error:', error);
        if (!containerRef.current) return;

        containerRef.current.innerHTML = '';
        const pre = document.createElement('pre');
        pre.className = 'bg-markdown-block border-markdown-border p-4 rounded text-xs text-markdown-text overflow-x-auto';
        const codeEl = document.createElement('code');
        codeEl.textContent = code;
        pre.appendChild(codeEl);
        containerRef.current.appendChild(pre);
        setIsLoaded(true);
      }
    };

    renderMermaid();

    return () => {
      cancelled = true;
    };
  }, [code, diagramIdRef]);

  return (
    <div className="my-5 flex justify-center">
      <div
        ref={containerRef}
        className={isLoaded ? 'mx-auto w-full overflow-x-auto' : 'w-full animate-pulse rounded bg-markdown-inline'}
      />
    </div>
  );
};

const Paragraph: Components['p'] = ({ children, className }) => {
  const classes = className ?? 'mt-5 mb-4 text-[13px] text-markdown-text';
  return <p className={classes}>{children}</p>;
};

const CodeRenderer: Components['code'] = ({ className, children, node, ...props }) => {
  const isInCodeBlock = useContext(CodeBlockContext);
  const classList = Array.isArray((node as { properties?: { className?: string[] } })?.properties?.className)
    ? ((node as { properties?: { className?: string[] } }).properties?.className ?? [])
    : [];
  const codeValue = String(children ?? '').replace(/\n$/, '');
  const isMermaid = classList.includes('language-mermaid');

  if (isMermaid) {
    const stableKey = `mermaid-${hashString(codeValue)}`;
    return <MermaidDiagram key={stableKey} code={codeValue} />;
  }

  if (isInCodeBlock) {
    const combined = className ? `${className} text-markdown-text` : 'text-markdown-text';
    return (
      <code className={combined} {...props}>
        {children}
      </code>
    );
  }

  return (
    <code className="inline rounded-sm bg-markdown-inline px-1.5 py-0.5 font-mono text-xs text-markdown-accent" {...props}>
      {children}
    </code>
  );
};

const PreRenderer: Components['pre'] = ({ children, ...props }) => {
  const childArray = React.Children.toArray(children);
  const singleChild = childArray.length === 1 ? childArray[0] : null;

  if (React.isValidElement(singleChild) && singleChild.type === MermaidDiagram) {
    return <>{singleChild}</>;
  }

  return (
    <CodeBlockContext.Provider value={true}>
      <pre
        className="mt-5 mb-4 overflow-x-auto rounded-sm border-markdown-border bg-markdown-block p-3 text-[12px] leading-relaxed text-markdown-text"
        {...props}
      >
        {childArray}
      </pre>
    </CodeBlockContext.Provider>
  );
};

const BlockquoteRenderer: Components['blockquote'] = ({ children }) => {
  const childArray = React.Children.toArray(children);

  const extractText = (node: React.ReactNode): string => {
    if (typeof node === 'string') return node;
    if (React.isValidElement(node)) {
      return React.Children.toArray(node.props.children).map(extractText).join('');
    }
    return '';
  };

  const firstChildText = extractText(childArray[0] ?? '').trim();
  const calloutMatch = firstChildText.match(/^\[!([A-Z]+)]\s*(.*)$/i);

  if (calloutMatch) {
    const [, rawType, restText] = calloutMatch;
    const title = rawType.charAt(0).toUpperCase() + rawType.slice(1).toLowerCase();
    const remainingChildren = childArray.slice(1);

    const normalizedChildren = React.Children.map(remainingChildren, (child) => {
      if (!React.isValidElement(child)) {
        return child;
      }

      if (child.type === Paragraph) {
        return (
          <Paragraph className="mt-1 text-[12px] leading-relaxed text-markdown-text">
            {child.props.children}
          </Paragraph>
        );
      }

      return child;
    });

    return (
      <div className="my-4 rounded-sm border border-markdown-callout-border bg-markdown-callout-bg p-4 text-[12px] text-markdown-text">
        <div className="mb-2 flex items-center gap-2 font-semibold uppercase tracking-wide text-markdown-accent">
          <span className="text-base">⚠️</span>
          {title}
        </div>
        <div className="space-y-2 text-[12px] text-markdown-muted">
          {restText && <p className="mt-1 leading-relaxed text-markdown-text">{restText}</p>}
          {normalizedChildren}
        </div>
      </div>
    );
  }

  return (
    <blockquote className="ml-3 my-4 border-l-2 border-markdown-border bg-markdown-inline px-4 py-2 text-[12px] text-markdown-muted">
      {children}
    </blockquote>
  );
};

const githubMarkdownComponents: Components = {
  h1: ({ children }) => (
    <h1 className="mb-5 border-b border-markdown-border pb-3 text-2xl font-semibold text-markdown-text">{children}</h1>
  ),
  h2: ({ children }) => (
    <h2 className="mt-6 mb-4 border-b border-markdown-border pb-3 text-xl font-semibold text-markdown-text">{children}</h2>
  ),
  h3: ({ children }) => <h3 className="mt-5 mb-3 text-[17px] font-semibold text-markdown-text">{children}</h3>,
  p: Paragraph,
  ul: ({ children }) => <ul className="mb-4 ml-6 list-disc space-y-1 text-[13px] text-markdown-text">{children}</ul>,
  ol: ({ children }) => <ol className="mb-4 ml-6 list-decimal space-y-1 text-[13px] text-markdown-text">{children}</ol>,
  li: ({ children }) => <li className="leading-relaxed text-[13px] text-markdown-text">{children}</li>,
  a: ({ href, children }) => (
    <a href={href} className="text-markdown-accent underline hover:text-markdown-accent-hover" target="_blank" rel="noreferrer">
      {children}
    </a>
  ),
  pre: PreRenderer,
  code: CodeRenderer,
  blockquote: BlockquoteRenderer,
  table: ({ children }) => (
    <div className="mt-6 mb-4 overflow-x-auto rounded-sm border-markdown-border bg-markdown-code-bg">
      <table className="w-full border-collapse text-sm text-markdown-text">{children}</table>
    </div>
  ),
  thead: ({ children }) => (
    <thead className="bg-markdown-inline text-left text-markdown-text">{children}</thead>
  ),
  tbody: ({ children }) => <tbody>{children}</tbody>,
  th: ({ children }) => (
    <th className="border-b border-markdown-border px-4 py-2 font-semibold text-markdown-text">{children}</th>
  ),
  td: ({ children }) => <td className="border-b border-markdown-border px-4 py-2 text-markdown-muted">{children}</td>,
  hr: () => <hr className="my-6 border-markdown-border" />,
};

export interface GithubMarkdownProps {
  content: string;
}

export const GithubMarkdown: React.FC<GithubMarkdownProps> = ({ content }) => (
  <ReactMarkdown remarkPlugins={[remarkGfm]} components={githubMarkdownComponents}>
    {content}
  </ReactMarkdown>
);
