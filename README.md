# What is this

Cinderella makes small, open-weights models like Qwen 3.5 9B capable of performing
meaningful work on modern Macs (Apple Silicon / 18GB+ unified RAM).

On more capable machines with more unified memory, Cindy picks the "LLM that fits perfectly" and is even more capable.

# Tenets

- Small models are capable of performing meaningful development and testing work
- Small models need more guardrails, guidance, and constraints. A Claude Code-style interface doesn't work well with a 9B param model due to their relative lack of skill with dealing with ambiguity.
- Nobody has executed yet an open source / open weights harness with taste, judgement and tuning. We are talking sensible engineering tradeoffs, a slick UI and no fiddling with llama_serve parameters or quantizations (to cite just two knobs) to get started with excellent results.

# development goals

- Combine open source tools like llama_server and open weights models (currently focused on the Qwen family to constrain tool call work) into one already configured package
- Provide a library of runbooks, templates, or skills. Call them what you will; these act as guides or guardrails to keep 9B on track.
- Do the prompt engineering work with diligence. This is actually more important on small models (citation needed) due to hallucinations with conflicting instructions and the small context sizes in use.
- Define through iteration and tinkering the proper U/X for getting excellent results out of small language models (hint: it's not a Claude-code style repl).


# What exists already / doesn't exist

I see four components that should be bundled (this is discussed in DEV-GOALS md also)

1) What LLM fits on my hardware? there is the widely popular [llmfit](https://github.com/AlexsJones/llmfit) for this.
2) There is the harness. pi and opencode partially solve this. But a polished MacOS coding UI? I don't know of one.
3) local models have different strengths and weakness. ambiguity is tough. Need good prompting. Superpowers/gstack/gauntlette should be the default experience, especially with smaller models. Also, need reasonable llama-server params like temperature.
4) One-click install. No more fiddling with params. Nothing nails this either.

Aspirationally, Cinderella or Cindy bundles all of this stuff into one beautiful MacOS application.


# notes from 4-26

the following graph shows the 'cliff' as tokens/second drops when using a MoE model with some layers offloaded to CPU RAM.

The implication here is that MoE models may be more capable than dense models if only partially accelerated on the GPU....

![ngl-sweep-plot.png](ngl-sweep-plot.png)

