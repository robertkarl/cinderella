Cinderella is a very simple agent harness written in rust.

the code is in src.

See also DEV GOALS in [[CINDY-DEV-GOALS.md]]


the following graph shows the 'cliff' as tokens/second drops when using a MoE model with some layers offloaded to CPU RAM.

The implication here is that MoE models may be more capable than dense models if only partially accelerated on the GPU....

![ngl-sweep-plot.png]

