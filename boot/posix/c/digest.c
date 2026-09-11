/* sha256sum, built on an unmodified public-domain SHA-256 (third-party/):
   the digest of each named file, read through stdio. */
#include <stdio.h>
#include <string.h>
#include "third-party/sha256.h"

int main(int argc, char **argv) {
    if (argc < 2) {
        fprintf(stderr, "usage: %s FILE...\n", argv[0]);
        return 2;
    }
    for (int index = 1; index < argc; index++) {
        FILE *file = fopen(argv[index], "r");
        if (file == NULL) {
            perror(argv[index]);
            return 1;
        }
        SHA256_CTX context;
        sha256_init(&context);
        unsigned char buffer[128];
        size_t count;
        while ((count = fread(buffer, 1, sizeof buffer, file)) > 0) {
            sha256_update(&context, buffer, count);
        }
        fclose(file);
        unsigned char hash[SHA256_BLOCK_SIZE];
        sha256_final(&context, hash);
        for (size_t at = 0; at < sizeof hash; at++) {
            printf("%02x", hash[at]);
        }
        printf("  %s\n", argv[index]);
    }
    return 0;
}
