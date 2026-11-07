# v0.3.221

The native text workshop and reader conformance fixture give the
compiled Agel reader an explicit 20 million fuel budget. The reader
grew when it added map syntax, and the default budget stopped the
self-rebuild before it could compare the resulting IR.

The four reader tests pass, including rebuilding the reader and
compiler from source and checking the resulting IR against the
bootstrap. The managed runtime's default limit stays at four million.
