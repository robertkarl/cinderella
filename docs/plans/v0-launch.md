# v0 Launch Checklist

Date: 2026-05-05

## The demo-defining feature (do first)

1. **System awareness skill** — periodic poll of memory pressure, top processes by resource usage, inference speed baseline + degradation detection
2. **Surface it in the UI** — Cindy proactively says "Final Cut is eating 9GB, I'm running 35% slower than baseline"
3. **35B MoE model support** — model selection logic based on hardware tier, not just the hardcoded 9B

## The one workflow, nailed

4. **Network debug polish** — make it friendly enough that someone who isn't you can use it without guidance
5. **Wire up token rate display** — it's already calculated in llm.rs, just disconnected from the TUI

## Minimum viable distribution

6. **Single static page at robertkarl.net/cinderella** — email capture + what it is + a video of the demo
7. **One blog post on that page** — building Cindy, the local-first thesis, whatever feels natural

## Onboarding (minimal, not PDF)

8. **First-run experience in the app itself** — not a PDF. Show what Cindy can do when you open it. Iterate after real people try it.

## Cut from v0

- ~~Custom domain~~ (robertkarl.net is fine)
- ~~Substack / separate blog~~ (one page)
- ~~Cron workflow~~ (v0.1)
- ~~Mass summarize workflow~~ (v0.1)
- ~~PDF documentation~~ (replaced by in-app onboarding)
