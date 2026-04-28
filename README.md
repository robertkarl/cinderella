# What is this

Nobody has nailed the 'local AI LLM app' for Mac OS. The local Cursor.app.

I see four components that should be bundled (this is discussed in DEV-GOALS md also)

1) What LLM fits on my hardware? there is the widely popular [llmfit](https://github.com/AlexsJones/llmfit) for this.
2) There is the harness. pi and opencode partially solve this. But a polished MacOS coding UI? I don't know of one.
3) local models have different strengths and weakness. ambiguity is tough. Need good prompting. Superpowers/gstack/gauntlette should be the default experience, especially with smaller models. Also, need reasonable llama-server params like temperature.
4) One-click install. No more fiddling with params.

Aspirationally, Cinderella or Cindy bundles all of this stuff into one beautiful MacOS application.

# What exists today.

Cinderella is a very simple agent harness written in rust.

the code is in src.

See also DEV GOALS in [CINDY-DEV-GOALS.md](CINDY-DEV-GOALS.md)


# notes from 4-26

the following graph shows the 'cliff' as tokens/second drops when using a MoE model with some layers offloaded to CPU RAM.

The implication here is that MoE models may be more capable than dense models if only partially accelerated on the GPU....

![ngl-sweep-plot.png](ngl-sweep-plot.png)

