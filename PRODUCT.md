# Product

<!-- impeccable:product-schema 1 -->

## Platform

web

## Users

Primary: people who run Synbot for themselves — developers and technical operators who install a single binary (or the desktop app) on their own machine or a small server and use the web console as their daily agent workbench. They arrive already knowing what an agent, a tool call, an approval, and a channel are; they are not browsing, they are operating.

Secondary (confirmed for this redesign): people receiving a distributed build — they open the console for the first time without the author's context, so first-run, empty, and error states must explain themselves without a guide standing next to them.

## Product Purpose

Synbot is a personal AI agent runtime written in Rust: it answers on chat channels (Feishu, DingTalk, Telegram, Discord, Matrix, email, web), runs tools inside isolated sandboxes with approval, keeps daily and long-term memory, executes persistent workflows, loads wasm plugins and MCP servers, and exposes the same agent to editors through ACP.

The web console is the console for that runtime: talk to the agent with live streaming, approvals, and project-aware tooling, and administer channels, sessions, cron, workflows, roles, skills, plugins, security policy, config, and logs. Success means the operator can see what the agent is doing, why, and what it changed — faster than reading logs.

## Positioning

One small portable binary that is both the agent and its control plane: no gateway port is exposed to the internet, every tool runs in a native or container sandbox, and every sensitive action passes an approval gate that can be answered from any channel. The console is not a demo shell around a hosted model — it is the admin surface of a self-hosted runtime, and it must read like it owns the machine.

## Operating Context

- Shipped two ways: a Tauri desktop app (sidecar binary, one-time Bearer token over IPC) and a browser console served by `synbot start` on `127.0.0.1:18888` with HTTP Basic auth.
- Long sessions next to a terminal and an editor; the agent writes files in the workspace, so diffs, file trees, and deliverables are read in place.
- The chat surface carries three peer views — Chat, Terminal (xterm), Team (subagents) — plus a project panel with the workspace file tree, symbol search, and turn diffs, a deliverable preview (docx/xlsx/pptx/pdf), and inline approval cards.
- Two languages (zh/en) and six colour schemes in light and dark are part of the shipped product, not preferences added later.
- Operators may run several chats and agents in parallel and switch between them while a turn streams.

## Capabilities and Constraints

- Runtime constraints that the UI must respect: a single loop lock per agent turn (commands like `/stop`, `/clear`, `/status` are answered out-of-band), an event stream (`/ws/agent` v2) with `seq` replay, and one global admin session (`web_admin`).
- Retained by decision: full zh/en i18n (≈800 keys, both locales must stay complete), six colour schemes plus light/dark, all chat views and panels, and the desktop IPC contract (sidecar, Bearer token, embedded `web/dist`).
- Stack: React 19 + Vite + TypeScript + Tailwind with CSS-variable colour tokens (`--color-*`), react-router, lucide icons; built into `web/dist` and embedded into the binary for release.
- Existing surface inventory: Overview, Chat, Channels, Sessions, Cron, Workflows, Roles, Skills, Plugins, Security, Config, Logs, Login.

## Brand Commitments

- Name: Synbot. Voice: technical, direct, lowercase-unpretentious — it talks about tools, sandboxes, approvals, and tokens without marketing adjectives.
- No mascots, no gamification, no celebratory confetti; trust is earned by visibility of real state.
- Standing visual preference (confirmed): the console follows the category standard — a neutral modern SaaS workbench measured against **Linear** and **Vercel**. Convention is the commitment: executed at full fidelity, without irony or decorative quirk. The six colour schemes become six accent ramps over one neutral scale; light and dark are both first-class.

## Evidence on Hand

- Real capability surface: `src/` (agent loop, events, tools, sandbox, channels, workflow, plugin, web), `readme.MD`, `docs/` (getting-started, user-guide, developer-guide), `licenses/`.
- Real UI code and copy: `web/src` (13 routes, ~4.3k lines of pages, i18n locales).
- Absent and must not be fabricated: customer logos, testimonials, benchmarks, pricing, or "trusted by" proof. No marketing claims anywhere in the console.

## Product Principles

1. State is the product: every surface must show what is actually running, waiting, or failing, with the real numbers behind it.
2. The agent's work is inspectable: plans, tool calls, diffs, and deliverables are first-class, never hidden behind a spinner.
3. Approval is a conversation, not a modal trap: an interrupt that can be answered from any channel must stay legible and quiet.
4. Density over decoration: the operator scans many rows, logs, and events; ornament that costs a row is a defect.
5. Explain the first run: a stranger installing the binary must be able to tell, from the console alone, what to configure next.

## Accessibility & Inclusion

- Keyboard reachability and a visible focus ring on every interactive element (`:focus-visible`).
- Both locales must fit the same layout: zh labels are short, en labels run long — no fixed-width controls that clip English.
- Motion must respect `prefers-reduced-motion`; streaming output must stay readable when animation is disabled.
