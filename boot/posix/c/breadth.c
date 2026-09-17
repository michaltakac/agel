/* The library's breadth, checked from C: arguments, the heap reusing and
   joining blocks, the formatter's widths and flags, strings, numbers,
   sorting, streams over files with append and seek. Prints one line per
   area and "breadth: N checks passed". */
#include <assert.h>
#include <ctype.h>
#include <errno.h>
#include <fcntl.h>
#include <sys/stat.h>
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
    check(big != NULL, "the whole first heap is one block again");
    free(big);
    /* The heap grows by pages the supervisor maps, up to the process
       window; a request past the window is what ENOMEM is for now. */
    check(malloc(400 * 1024) != NULL, "the heap grows past its first pages");
    check(malloc(64 * 1024 * 1024) == NULL && errno == ENOMEM, "ENOMEM past the window");
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

    /* Append is chosen at each write, even after a seek or another writer. */
    int first = open("offsets", O_RDWR | O_CREAT | O_APPEND);
    int second = open("offsets", O_WRONLY | O_APPEND);
    check(first >= 0 && second >= 0, "append descriptors");
    check(write(first, "abc", 3) == 3, "first append");
    check(write(second, "def", 3) == 3, "independent append uses current end");
    check(lseek(first, 0, SEEK_SET) == 0, "seek append descriptor");
    check(write(first, "g", 1) == 1, "append ignores seek for writing");
    check(lseek(first, 0, SEEK_CUR) == 7, "append updates offset");
    check(lseek(first, 0, SEEK_SET) == 0, "read append contents");
    char appended[8] = {0};
    check(read(first, appended, 7) == 7 && strcmp(appended, "abcdefg") == 0, "append preserves both writers");
    close(first);
    close(second);

    /* Reusing a retained block after truncation must not disclose old data. */
    first = open("offsets", O_RDWR);
    check(first >= 0 && ftruncate(first, 2) == 0, "truncate before sparse write");
    check(lseek(first, 5000, SEEK_SET) == 5000, "seek across an unallocated block");
    check(write(first, "z", 1) == 1, "write beyond EOF");
    check(lseek(first, 0, SEEK_SET) == 0, "read sparse file");
    char sparse[5001];
    size_t used = 0;
    while (used < sizeof sparse) {
        ssize_t n = read(first, sparse + used, sizeof sparse - used);
        if (n <= 0) break;
        used += n;
    }
    int zeroed = used == sizeof sparse && sparse[0] == 'a' && sparse[1] == 'b' && sparse[5000] == 'z';
    for (size_t at = 2; at < 5000 && zeroed; at++) zeroed = sparse[at] == 0;
    check(zeroed, "the entire hole is zero, including retained bytes");
    close(first);
    check(unlink("offsets") == 0, "remove offsets fixture");
    check(write(0, "x", 1) == -1 && errno == EBADF, "stdin is not writable");
    printf("offsets: append across writers and seek, zero-filled holes, descriptor rights\n");

    printf("breadth: %d checks passed\n", checks);
    return 0;
}
