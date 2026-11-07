# v0.3.124

Agel’s RSA setup starts Montgomery conversion at the highest whole limb power
known to be below the odd modulus. It skips modular doubling steps whose
result is already known.

The old and new implementations produced identical contexts for a 1024-bit
modulus. Hosted evaluation used 5,801,864 steps instead of 10,005,459.
The RSA oracle tests across limb boundaries, the XTEA and ISAAC vectors,
and the fragmented wire fixtures passed. Native timing is still pending.
