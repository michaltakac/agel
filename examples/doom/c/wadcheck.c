/* Reads a WAD the way the engine does and says what it sees: the length by
   seeking to the end, the header, the first entries of the directory. */
#include <stdio.h>
#include <string.h>

int main(int argc, char **argv) {
    const char *path = argc > 1 ? argv[1] : "/data/doom1.wad";
    FILE *file = fopen(path, "rb");
    if (!file) {
        printf("wadcheck: cannot open %s\n", path);
        return 1;
    }
    fseek(file, 0, SEEK_END);
    long length = ftell(file);
    printf("wadcheck: length %ld\n", length);
    fseek(file, 0, SEEK_SET);
    unsigned char header[12];
    size_t got = fread(header, 1, 12, file);
    unsigned lumps = header[4] | header[5] << 8 | header[6] << 16 | (unsigned)header[7] << 24;
    unsigned table = header[8] | header[9] << 8 | header[10] << 16 | (unsigned)header[11] << 24;
    printf("wadcheck: header %zu bytes %c%c%c%c lumps %u table at %u\n", got, header[0], header[1], header[2], header[3], lumps, table);
    if (fseek(file, (long)table, SEEK_SET) != 0) {
        printf("wadcheck: seek to the table failed\n");
        return 2;
    }
    printf("wadcheck: tell after the seek %ld\n", ftell(file));
    for (int index = 0; index < 4; index++) {
        unsigned char entry[16];
        got = fread(entry, 1, 16, file);
        char name[9];
        memcpy(name, entry + 8, 8);
        name[8] = 0;
        unsigned start = entry[0] | entry[1] << 8 | entry[2] << 16 | (unsigned)entry[3] << 24;
        unsigned size = entry[4] | entry[5] << 8 | entry[6] << 16 | (unsigned)entry[7] << 24;
        printf("wadcheck: entry %d got %zu name %s at %u size %u\n", index, got, name, start, size);
    }
    fclose(file);
    return 0;
}
