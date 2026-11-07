# v0.3.166

The native pair runner checks the donor's policy source digests before
starting either arm. `--source-root` lets both arms pin the same explicit
Agel source tree when reproducing a transfer from a historical donor.

Preflight rejected the current checkout for an older donor and accepted
a source tree with its matching combat policy.
