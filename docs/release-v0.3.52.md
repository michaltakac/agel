# v0.3.52 · Give repair retries the judge's reason

Agel's general repair agent now tells the text judge which checks the
reader and static analyzer already ran. The judge evaluates the proposed
source against the contract and observations. The replay gate evaluates
its behavior before activation.

When the judge refuses a change, the next writer prompt includes the
score, required minimum, and reported defect. The previous proposal
remains in the prompt and the raw judge reply remains in the repair
record. The approval rule still requires the configured score and no
reported defect.

The repair tests verify that a refusal returns to the writer with the
diagnostic and preserves the active version.
