# Third-party sources built unmodified

`sha256.c` and `sha256.h` are Brad Conte's SHA-256 from
<https://github.com/B-Con/crypto-algorithms> (commit `cfbde48414ba`),
which that repository releases into the public domain. They are here,
byte for byte as published, as the proof that a C source written for
other systems builds against `agel-libc` and runs on Agel; `../digest.c`
is the program around them.
