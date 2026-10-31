// Lightweight Markdown renderer for chat messages.
//
// Everything is built as React nodes (never `dangerouslySetInnerHTML`), so model output cannot
// inject markup. Supports the subset agents actually emit: fenced code (with copy + token
// highlighting), headings, lists, tables, quotes, rules and common inline spans.

import { ReactNode, useMemo, useState } from 'react';
import { Check, Copy } from 'lucide-react';

type Block =
  | { kind: 'code'; lang: string; code: string }
  | { kind: 'heading'; level: number; text: string }
  | { kind: 'list'; ordered: boolean; items: string[] }
  | { kind: 'quote'; lines: string[] }
  | { kind: 'table'; header: string[]; rows: string[][] }
  | { kind: 'hr' }
  | { kind: 'para'; lines: string[] };

const CODE_KEYWORDS = new Set([
  'as', 'async', 'await', 'break', 'case', 'catch', 'class', 'const', 'continue', 'def', 'do',
  'else', 'enum', 'export', 'extends', 'false', 'fn', 'for', 'from', 'function', 'if', 'impl',
  'import', 'in', 'interface', 'let', 'loop', 'match', 'mod', 'mut', 'new', 'None', 'null',
  'pub', 'return', 'self', 'Some', 'static', 'struct', 'super', 'switch', 'this', 'throw',
  'trait', 'true', 'try', 'type', 'typeof', 'use', 'var', 'void', 'where', 'while', 'yield',
]);

/** Split a fenced-code body into highlighted spans (comments, strings, numbers, keywords). */
function highlight(code: string, lang: string): ReactNode[] {
  const hashComments = ['py', 'python', 'sh', 'bash', 'zsh', 'yaml', 'yml', 'toml', 'rb'].includes(
    lang.toLowerCase()
  );
  const pattern = new RegExp(
    [
      /(\/\/[^\n]*|\/\*[\s\S]*?\*\/)/.source,
      hashComments ? /(#[^\n]*)/.source : null,
      /("(?:[^"\\]|\\.)*"|'(?:[^'\\]|\\.)*'|`(?:[^`\\]|\\.)*`)/.source,
      /\b(\d+(?:\.\d+)?)\b/.source,
      /\b([A-Za-z_$][\w$]*)\b/.source,
    ]
      .filter(Boolean)
      .join('|'),
    'g'
  );

  const nodes: ReactNode[] = [];
  let last = 0;
  let key = 0;
  for (let m = pattern.exec(code); m !== null; m = pattern.exec(code)) {
    if (m.index > last) nodes.push(code.slice(last, m.index));
    const [token, comment, hash, string, number, word] = m;
    if (comment || hash) {
      nodes.push(
        <span key={key++} className="text-ink-2 italic">
          {token}
        </span>
      );
    } else if (string) {
      nodes.push(
        <span key={key++} className="text-success">
          {token}
        </span>
      );
    } else if (number) {
      nodes.push(
        <span key={key++} className="text-info">
          {token}
        </span>
      );
    } else if (word && CODE_KEYWORDS.has(word)) {
      nodes.push(
        <span key={key++} className="text-accent font-medium">
          {token}
        </span>
      );
    } else {
      nodes.push(token);
    }
    last = m.index + token.length;
  }
  if (last < code.length) nodes.push(code.slice(last));
  return nodes;
}

function CodeBlock({ code, lang }: { code: string; lang: string }) {
  const [copied, setCopied] = useState(false);

  const copy = async () => {
    try {
      await navigator.clipboard.writeText(code);
      setCopied(true);
      window.setTimeout(() => setCopied(false), 1500);
    } catch {
      // Clipboard unavailable (insecure context) — leave the button unchanged.
    }
  };

  return (
    <div className="my-2 overflow-hidden rounded-panel border border-line bg-surface-2">
      <div className="flex items-center justify-between gap-2 px-2.5 py-1 text-micro text-ink-3">
        <span className="font-mono">{lang || 'text'}</span>
        <button
          type="button"
          onClick={copy}
          className="flex items-center gap-1 transition-colors duration-fast hover:text-ink"
          aria-label="Copy code"
        >
          {copied ? <Check className="h-3 w-3" /> : <Copy className="h-3 w-3" />}
          <span>{copied ? 'copied' : 'copy'}</span>
        </button>
      </div>
      <pre className="overflow-x-auto px-2.5 py-2 font-mono text-micro leading-relaxed text-ink">
        <code>{highlight(code, lang)}</code>
      </pre>
    </div>
  );
}

/** Index of the line ending the table separator row, or -1 when `lines[i]` is not a table head. */
function tableStart(lines: string[], i: number): number {
  const header = lines[i];
  const separator = lines[i + 1];
  if (!header?.includes('|') || !separator) return -1;
  return /^\s*\|?[\s:|-]+\|[\s:|-]*$/.test(separator) && separator.includes('-') ? i + 2 : -1;
}

function splitRow(row: string): string[] {
  return row
    .trim()
    .replace(/^\|/, '')
    .replace(/\|$/, '')
    .split('|')
    .map((cell) => cell.trim());
}

function parseBlocks(source: string): Block[] {
  const lines = source.replace(/\r\n/g, '\n').split('\n');
  const blocks: Block[] = [];
  let i = 0;

  while (i < lines.length) {
    const line = lines[i];

    if (!line.trim()) {
      i += 1;
      continue;
    }

    const fence = /^\s*```(\w*)/.exec(line);
    if (fence) {
      const lang = fence[1];
      const body: string[] = [];
      i += 1;
      while (i < lines.length && !/^\s*```/.test(lines[i])) {
        body.push(lines[i]);
        i += 1;
      }
      i += 1; // closing fence (or end of input)
      blocks.push({ kind: 'code', lang, code: body.join('\n') });
      continue;
    }

    const heading = /^(#{1,6})\s+(.*)$/.exec(line);
    if (heading) {
      blocks.push({ kind: 'heading', level: heading[1].length, text: heading[2] });
      i += 1;
      continue;
    }

    if (/^\s*([-*_])\s*\1\s*\1[\s\-*_]*$/.test(line)) {
      blocks.push({ kind: 'hr' });
      i += 1;
      continue;
    }

    const tableRows = tableStart(lines, i);
    if (tableRows > 0) {
      const header = splitRow(lines[i]);
      i = tableRows;
      const rows: string[][] = [];
      while (i < lines.length && lines[i].includes('|') && lines[i].trim()) {
        rows.push(splitRow(lines[i]));
        i += 1;
      }
      blocks.push({ kind: 'table', header, rows });
      continue;
    }

    if (/^\s*>\s?/.test(line)) {
      const quoted: string[] = [];
      while (i < lines.length && /^\s*>\s?/.test(lines[i])) {
        quoted.push(lines[i].replace(/^\s*>\s?/, ''));
        i += 1;
      }
      blocks.push({ kind: 'quote', lines: quoted });
      continue;
    }

    const listItem = /^\s*(?:([-*+])|(\d+)[.)])\s+/;
    if (listItem.test(line)) {
      const ordered = Boolean(/^\s*\d+[.)]\s+/.exec(line));
      const items: string[] = [];
      while (i < lines.length && listItem.test(lines[i])) {
        items.push(lines[i].replace(listItem, ''));
        i += 1;
      }
      blocks.push({ kind: 'list', ordered, items });
      continue;
    }

    const paragraph: string[] = [];
    while (
      i < lines.length &&
      lines[i].trim() &&
      !/^\s*```/.test(lines[i]) &&
      !/^(#{1,6})\s+/.test(lines[i]) &&
      !/^\s*>\s?/.test(lines[i]) &&
      !listItem.test(lines[i])
    ) {
      paragraph.push(lines[i]);
      i += 1;
    }
    blocks.push({ kind: 'para', lines: paragraph });
  }

  return blocks;
}

function safeHref(href: string): string | undefined {
  return /^(https?:\/\/|mailto:|#|\/)/i.test(href) ? href : undefined;
}

function renderInline(text: string, keyPrefix: string): ReactNode[] {
  const pattern =
    /`([^`]+)`|\*\*([^*]+)\*\*|\*([^*\n]+)\*|~~([^~]+)~~|\[([^\]]+)\]\(([^)\s]+)\)|(https?:\/\/[^\s<)]+)/g;
  const nodes: ReactNode[] = [];
  let last = 0;
  let key = 0;

  for (let m = pattern.exec(text); m !== null; m = pattern.exec(text)) {
    if (m.index > last) nodes.push(text.slice(last, m.index));
    const [token, code, bold, italic, strike, label, href, bareUrl] = m;
    const id = `${keyPrefix}-${key++}`;
    if (code) {
      nodes.push(
        <code key={id} className="rounded bg-surface-2 px-1 py-0.5 font-mono text-[0.9em] text-ink">
          {code}
        </code>
      );
    } else if (bold) {
      nodes.push(
        <strong key={id} className="font-semibold">
          {bold}
        </strong>
      );
    } else if (italic) {
      nodes.push(<em key={id}>{italic}</em>);
    } else if (strike) {
      nodes.push(
        <span key={id} className="line-through opacity-70">
          {strike}
        </span>
      );
    } else if (label && href) {
      const url = safeHref(href);
      nodes.push(
        url ? (
          <a
            key={id}
            href={url}
            target="_blank"
            rel="noreferrer noopener"
            className="text-accent underline decoration-accent/40 underline-offset-2 hover:decoration-accent"
          >
            {label}
          </a>
        ) : (
          <span key={id}>{token}</span>
        )
      );
    } else if (bareUrl) {
      nodes.push(
        <a
          key={id}
          href={bareUrl}
          target="_blank"
          rel="noreferrer noopener"
          className="break-all text-accent underline decoration-accent/40 underline-offset-2 hover:decoration-accent"
        >
          {bareUrl}
        </a>
      );
    } else {
      nodes.push(token);
    }
    last = m.index + token.length;
  }

  if (last < text.length) nodes.push(text.slice(last));
  return nodes;
}

const HEADING_CLASS = [
  'text-section font-semibold mt-3 mb-1',
  'text-body font-semibold mt-3 mb-1',
  'text-small font-semibold mt-2 mb-1',
  'text-small font-semibold mt-2 mb-1',
  'text-small font-semibold mt-2 mb-1',
  'text-small font-semibold mt-2 mb-1',
];

export default function Markdown({
  content,
  className = '',
}: {
  content: string;
  className?: string;
}) {
  const blocks = useMemo(() => parseBlocks(content), [content]);

  return (
    <div className={`text-body break-words ${className}`}>
      {blocks.map((block, index) => {
        switch (block.kind) {
          case 'code':
            return <CodeBlock key={index} code={block.code} lang={block.lang} />;
          case 'heading': {
            const Tag = (`h${Math.min(block.level + 2, 6)}` as unknown) as 'h3';
            return (
              <Tag key={index} className={HEADING_CLASS[block.level - 1]}>
                {renderInline(block.text, `h${index}`)}
              </Tag>
            );
          }
          case 'hr':
            return <hr key={index} className="my-3 border-line" />;
          case 'quote':
            return (
              <blockquote
                key={index}
                className="my-2 border-l border-line pl-3 text-ink-2"
              >
                {block.lines.map((line, j) => (
                  <div key={j}>{renderInline(line, `q${index}-${j}`)}</div>
                ))}
              </blockquote>
            );
          case 'list': {
            const items = block.items.map((item, j) => (
              <li key={j}>{renderInline(item, `l${index}-${j}`)}</li>
            ));
            return block.ordered ? (
              <ol key={index} className="my-1 pl-5 list-decimal space-y-0.5">
                {items}
              </ol>
            ) : (
              <ul key={index} className="my-1 pl-5 list-disc space-y-0.5">
                {items}
              </ul>
            );
          }
          case 'table':
            return (
              <div key={index} className="my-2 overflow-x-auto">
                <table className="min-w-full border border-line text-micro">
                  <thead className="bg-surface-2">
                    <tr>
                      {block.header.map((cell, j) => (
                        <th key={j} className="border border-line px-2 py-1 text-left font-medium">
                          {renderInline(cell, `th${index}-${j}`)}
                        </th>
                      ))}
                    </tr>
                  </thead>
                  <tbody>
                    {block.rows.map((row, j) => (
                      <tr key={j}>
                        {row.map((cell, k) => (
                          <td key={k} className="border border-line px-2 py-1 align-top">
                            {renderInline(cell, `td${index}-${j}-${k}`)}
                          </td>
                        ))}
                      </tr>
                    ))}
                  </tbody>
                </table>
              </div>
            );
          default:
            return (
              <p key={index} className="my-1 whitespace-pre-wrap">
                {renderInline(block.lines.join('\n'), `p${index}`)}
              </p>
            );
        }
      })}
    </div>
  );
}
