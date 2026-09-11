/* The library's breadth, checked from C: arguments, the heap reusing and
   joining blocks, the formatter's widths and flags, strings, numbers,
   sorting, streams over files with append and seek. Prints one line per
   area and "breadth: N checks passed". */
#include <assert.h>
#include <ctype.h>
#include <errno.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <unistd.h>

static int checks;

static void check(int condition, const char *what) {
    if (!condition) {
        printf("breadth: FAILED: %s\n", what);
        exit(1);
    }
    checks++;
}

static int compare_ints(const void *left, const void *right) {
    return *(const int *)left - *(const int *)right;
}

int main(int argc, char **argv) {
    printf("arguments: %d", argc);
    for (int index = 0; index < argc; index++) {
        printf(" [%s]", argv[index]);
    }
    printf("\n");
    check(argc == 3 && strcmp(argv[1], "one") == 0 && strcmp(argv[2], "two") == 0, "argv");

    /* The heap: a freed block is reused, neighbours join, realloc keeps data. */
    char *a = malloc(100);
    char *b = malloc(100);
    char *c = malloc(100);
    check(a && b && c, "malloc");
    strcpy(a, "first");
    free(b);
    char *d = malloc(50);
    check(d == b, "free block reused");
    free(d);
    free(c);
    char *e = malloc(220);
    check(e == b, "freed neighbours joined");
    a = realloc(a, 1000);
    check(a && strcmp(a, "first") == 0, "realloc keeps data");
    free(a);
    free(e);
    char *big = malloc(200 * 1024);
    check(big != NULL, "the whole arena is one block again");
    free(big);
    check(malloc(400 * 1024) == NULL && errno == ENOMEM, "ENOMEM past the arena");
    printf("heap: reuse, join, realloc, ENOMEM\n");

    /* The formatter. */
    char text[64];
    snprintf(text, sizeof text, "[%5d|%-5d|%05d|%+d|%x|%X|%o|%.3s|%c|%3lu|%%]", 42, 42, 42, 42, 255, 255, 8, "abcdef", 'z', 7ul);
    check(strcmp(text, "[   42|42   |00042|+42|ff|FF|10|abc|z|  7|%]") == 0, "snprintf formats");
    check(snprintf(text, 4, "%s", "truncated") == 9 && strcmp(text, "tru") == 0, "snprintf truncates");
    printf("formatter: %s\n", "widths, flags, precision, truncation");

    /* Strings and numbers. */
    check(strtol("  -0x1f rest", NULL, 0) == -31 && atoi("12ab") == 12, "strtol and atoi");
    check(strcmp(strstr("needle in haystack", "hay"), "haystack") == 0, "strstr");
    check(strrchr("a/b/c", '/')[1] == 'c' && toupper('q') == 'Q' && isdigit('7'), "strrchr and ctype");
    char joined[32] = "ab";
    strncat(strcat(joined, "cd"), "efgh", 2);
    check(strcmp(joined, "abcdef") == 0, "strcat and strncat");
    int numbers[] = {5, 3, 9, 1, 7};
    qsort(numbers, 5, sizeof numbers[0], compare_ints);
    check(numbers[0] == 1 && numbers[4] == 9, "qsort");
    printf("strings: strtol, strstr, strrchr, ctype, strcat, qsort\n");

    /* Streams over files: write, append, read back, seek. */
    FILE *out = fopen("log", "w");
    check(out != NULL, "fopen w");
    fprintf(out, "line %d\n", 1);
    fclose(out);
    out = fopen("log", "a");
    check(out != NULL, "fopen a");
    fputs("line 2\n", out);
    fclose(out);
    FILE *in = fopen("log", "r");
    check(in != NULL, "fopen r");
    char line[32];
    check(fgets(line, sizeof line, in) && strcmp(line, "line 1\n") == 0, "fgets first line");
    check(fgets(line, sizeof line, in) && strcmp(line, "line 2\n") == 0, "append wrote after the first");
    check(fgets(line, sizeof line, in) == NULL && feof(in), "EOF");
    check(lseek(fileno(in), 5, SEEK_SET) == 5, "lseek");
    char tail[8] = {0};
    check(read(fileno(in), tail, 2) == 2 && tail[0] == '1' && tail[1] == '\n', "read after seek");
    fclose(in);
    check(fopen("missing", "r") == NULL && errno == ENOENT, "fopen missing");
    printf("streams: fopen, fprintf, append, fgets, feof, lseek\n");

    printf("breadth: %d checks passed\n", checks);
    return 0;
}
