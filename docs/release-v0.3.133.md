# v0.3.133

Agel’s RSA serializer packs output bytes directly from 31-bit limbs.
It preserves leading sign padding and the representation of zero.

A hosted 1024-bit encryption returned the same ciphertext in 4,337,440
evaluation steps instead of 4,498,231. RSA checks against Python across
limb boundaries passed, as did the XTEA and ISAAC vectors. Native login
timing for this change is pending.
