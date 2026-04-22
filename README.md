# the thesis

An app that bundles 

a) llmfit
b) a polished MacOS coding UI like Cursor or Codex (or maybe even Claude Code)
c) One-click install as God intended -- bundle the model.
d) Decent model llama-server defaults

would be popular.

It would need to support tool calling. No Open WebUI chat nonsense.

# what exists

  hf agents run pi (HuggingFace) — uses llmfit to scan your hardware, auto-picks the right model/quant, downloads it, starts inference, launches a coding agent. One command. This
  is the most integrated thing that exists.

  Tier 1: Almost there

  ┌─────────────────────────┬────────────────┬──────────────────┬──────────────────┬──────────────────┐
  │         Product         │ Bundles model? │ Auto-detects HW? │   Agent tools?   │    One-click?    │
  ├─────────────────────────┼────────────────┼──────────────────┼──────────────────┼──────────────────┤
  │ hf-agents + Pi          │ Auto-downloads │ YES (llmfit)     │ File R/W, shell  │ Close            │
  ├─────────────────────────┼────────────────┼──────────────────┼──────────────────┼──────────────────┤
  │ Ollama 0.18.3 + VS Code │ ollama pull    │ Partial          │ YES (agent mode) │ 2 commands       │
  ├─────────────────────────┼────────────────┼──────────────────┼──────────────────┼──────────────────┤
  │ Bodega One              │ No (BYOM)      │ No               │ YES              │ Desktop app, $79 │
  └─────────────────────────┴────────────────┴──────────────────┴──────────────────┴──────────────────┘

  Tier 2: Great agents, you bring the model

  OpenCode (147K stars), Crush (Charmbracelet), Aider, Cline, Kilo Code, Roo Code, Goose, Open-Codex fork — all support Ollama/local endpoints, all have tool-calling. None bundle
  the model or auto-detect your hardware.

  The gap is still real

  Nobody ships one binary that does all four things:
  1. Detect hardware/RAM
  2. Download the right model
  3. Serve it with sane defaults (context size, no thinking tokens)
  4. Run a coding agent with tools

  hf agents run pi is the closest — but it's a Python CLI, not a polished desktop app. The "Cursor but local" product still doesn't exist as a one-click thing.

  The interesting tool is llmfit (11.7K stars) — it's the missing "which model fits my hardware" piece that everything else lacks. But it's a standalone recommender, not
  integrated into an agent.
