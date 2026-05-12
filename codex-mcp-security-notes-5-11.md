# MCP Security In Practice - 2026-05-11

Scope: practical notes from a web review of current MCP security discussion across official docs, GitHub advisories, Hacker News, and security blogs. This is meant to inform Glass Slipper's MCP design choices.

## Bottom Line

People do not really "solve" MCP security today. They reduce blast radius.

The practical industry posture is to treat MCP servers like plugins that can become local code-execution and data-exfiltration surfaces. The safer systems combine least privilege, human-visible approvals, sandboxing, server/tool pinning, narrow tool design, audit logs, and runtime monitoring. Model alignment and prompt wording are not considered sufficient security boundaries.

## What People Actually Do

### Treat MCP servers as installed software, not harmless API metadata

The official MCP security best-practices doc describes local MCP servers as binaries running on the user's machine with potential direct system access, and recommends consent, command visibility, sandboxing, restricted filesystem/network access, and use of stdio or authenticated IPC for local servers:

- https://modelcontextprotocol.io/docs/tutorials/security/security_best_practices

GitHub's `modelcontextprotocol/servers` security page also says those reference servers are educational examples, not production-ready implementations:

- https://github.com/modelcontextprotocol/servers/security

Practical implication: a Glass Slipper MCP server should be treated like a privileged local helper, not a passive model tool catalog.

### Avoid generic shell tools unless arbitrary code execution is acceptable

OX Security's 2026 MCP disclosure argues that STDIO/config-driven execution has propagated into RCE issues across MCP-enabled products, with many CVEs issued across the ecosystem:

- https://www.ox.security/blog/the-mother-of-all-ai-supply-chains-critical-systemic-vulnerability-at-the-core-of-the-mcp/
- https://www.ox.security/blog/mcp-supply-chain-advisory-rce-vulnerabilities-across-the-ai-ecosystem/

VentureBeat's coverage summarizes the practical recommendation: treat every MCP STDIO configuration as an untrusted execution surface and do not give servers full disk or shell privileges:

- https://venturebeat.com/security/mcp-stdio-flaw-200000-ai-agent-servers-exposed-ox-security-audit

Practical implication: Glass Slipper's MCP command tools should not execute arbitrary shell strings. If command execution remains, route through the existing classifier, apply timeouts and output caps, run in a bounded cwd, and kill process groups on timeout.

### Least privilege is the core mitigation

The official MCP authorization guidance recommends authorization for servers that access user-specific data, perform write operations, or touch enterprise/internal systems:

- https://modelcontextprotocol.io/docs/tutorials/security/authorization

The MCP security docs also recommend scope minimization, progressive privilege elevation, and avoiding catch-all scopes:

- https://modelcontextprotocol.io/docs/tutorials/security/security_best_practices

Invariant's GitHub MCP exploit writeup shows why coarse access is dangerous: an agent reading attacker-controlled public issues can be induced to access private repos and leak information back through a public repo. Their proposed mitigation includes runtime dataflow rules such as restricting an agent to one repository per session:

- https://invariantlabs.ai/blog/mcp-github-vulnerability

HN discussion around that incident also converges on least privilege and on the idea that any data available to the agent can be leaked if the workflow mixes private data, untrusted instructions, and an exfiltration tool:

- https://news.ycombinator.com/item?id=44097390

Practical implication: avoid tools that combine broad read access with broad write/network access in the same session.

### Human approval helps, but only if the UI is honest and specific

Simon Willison's MCP prompt-injection post highlights the MCP trust-and-safety recommendations: clients should show exposed tools, visibly indicate tool invocations, and present confirmation prompts. His recommendation is to treat those SHOULDs as MUSTs:

- https://simonwillison.net/2025/Apr/9/mcp-prompt-injection/

The same post also notes the hard part: users need interfaces that make it possible to understand what is about to happen. Hidden horizontal overflow, vague descriptions, or "always allow" behavior erase much of the value of human approval.

Practical implication: approvals need to show exact command, target path, target URL, repository, or other meaningful action details. "Allow local_summarize" is not enough if the hidden argument is `cat ~/.ssh/id_ed25519`.

### Tool descriptions and tool updates are part of the attack surface

Invariant's tool-poisoning research describes malicious tool descriptions, cross-server tool shadowing, and "rug pull" changes where a trusted server later changes its tool description:

- https://invariantlabs.ai/blog/mcp-security-notification-tool-poisoning-attacks

Simon Willison summarizes the same risk: malicious instructions can be hidden in tool descriptions that are visible to the LLM but not obvious to users:

- https://simonwillison.net/2025/Apr/9/mcp-prompt-injection/

Mitigations people discuss include showing AI-visible tool descriptions to users, hashing/pinning tool definitions, warning when descriptions change, and keeping separate trust boundaries between MCP servers.

Practical implication: Glass Slipper should keep its MCP tool descriptions static, short, auditable, and pinned through the app bundle. The companion app can validate that Claude's configured MCP binary points at the expected bundled helper.

### SSRF and network egress are first-class MCP concerns

The official MCP security best-practices doc calls out SSRF risks and recommends HTTPS enforcement, blocking private/reserved IP ranges, validating redirects, avoiding manual IP validation, and using egress proxies for server deployments:

- https://modelcontextprotocol.io/docs/tutorials/security/security_best_practices

Practical implication: `local_web_fetch` should not fetch arbitrary URLs by default. It should enforce scheme, size, redirect, and private-network restrictions, or be removed until those controls exist.

### Use scanners, proxies, and guardrails as compensating controls

Invariant's MCP-Scan supports scanning MCP configs, detecting prompt injection/tool poisoning, proxy-mode monitoring, tool pinning, PII/secrets checks, and custom guardrail policies:

- https://invariantlabs-ai.github.io/docs/mcp-scan/
- https://invariantlabs.ai/blog/introducing-mcp-scan

Trail of Bits' `mcp-context-protector` is a wrapper/proxy that adds trust-on-first-use pinning, tool-description validation, quarantine for suspicious responses, and ANSI control-sequence sanitization:

- https://blog.trailofbits.com/2025/07/28/we-built-the-security-layer-mcp-always-needed/
- https://www.trailofbits.com/mcp/

These are useful layers, but they do not replace least privilege and sandboxing.

Practical implication: Glass Slipper should be compatible with wrappers/proxies, but the product should not depend on users installing one to be safe.

### Enterprises turn MCP into an inventory and governance problem

OWASP's MCP Top 10 frames common risks as token/secret exposure, privilege escalation through scope creep, tool poisoning, supply-chain tampering, command injection/execution, prompt injection, and context over-sharing:

- https://owasp.org/www-project-mcp-top-10/

The realistic enterprise posture is:

- approved server registry
- signed or pinned versions
- scoped, short-lived credentials
- no long-lived secrets in configs or logs
- audit logs for every tool call
- egress controls
- sandboxed execution
- review before adding new MCP servers
- continuous scanning and runtime monitoring

Practical implication: a one-click installer should not silently mark a stale or mismatched MCP config as healthy. It should validate the exact binary path and repair drift.

## Applied To Glass Slipper

I would not ship the current MCP command tools as-is.

Recommended changes:

1. Remove generic shell execution from MCP, or route it through the existing bash classifier with explicit policy, cwd, timeout, output caps, and process-group cleanup.
2. Prefer purpose-specific tools over command strings. For example, `summarize_build_log_file` is safer than `local_summarize(command: "...")`.
3. Treat all MCP tool inputs as untrusted. Validate paths, URLs, sizes, and enum values.
4. Sandbox the MCP process and any subprocesses with minimal filesystem and network access.
5. Block localhost/private-network SSRF in web-fetch style tools unless explicitly enabled.
6. Pin bundled MCP binaries and validate `~/.claude.json` against the current app bundle path.
7. Log every tool call with arguments, result size, duration, and denial reason.
8. Avoid "always allow" workflows for tools that read secrets, write files, run commands, or make network requests.
9. Keep tool descriptions static and auditable; warn on any drift.
10. Assume model alignment will fail under indirect prompt injection and put security controls outside the model.

## Design Rule Of Thumb

If a malicious README, GitHub issue, web page, email, or log line could make the model call a tool, then that tool must be safe under adversarial input.

For Glass Slipper, that means command execution and arbitrary web fetch are the two MCP surfaces to fix first.
