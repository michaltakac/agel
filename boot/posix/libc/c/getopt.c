/* agel-libc: getopt, as POSIX describes it and as programs expect it:
   optind starts at 1, an option's argument is the rest of its word or the
   next word, "--" ends the options, and a lone "-" is an operand. */
#include <stdio.h>
#include <string.h>
#include <unistd.h>

int optind = 1;
int opterr = 1;
int optopt = 0;
char *optarg = NULL;

/* Where in argv[optind] the next option letter is; 0 means at a new word. */
static int at;

int getopt(int argc, char *const argv[], const char *options) {
    optarg = NULL;
    if (optind >= argc || argv[optind] == NULL) {
        return -1;
    }
    const char *word = argv[optind];
    if (at == 0) {
        if (word[0] != '-' || word[1] == '\0') {
            return -1;
        }
        if (word[1] == '-' && word[2] == '\0') {
            optind++;
            return -1;
        }
        at = 1;
    }
    int letter = (unsigned char)word[at++];
    const char *found = letter == ':' ? NULL : strchr(options, letter);
    if (found == NULL) {
        optopt = letter;
        if (opterr && options[0] != ':') {
            fprintf(stderr, "%s: unknown option -%c\n", argv[0], letter);
        }
        if (word[at] == '\0') {
            optind++;
            at = 0;
        }
        return '?';
    }
    if (found[1] == ':') {
        if (word[at] != '\0') {
            optarg = (char *)&word[at];
            optind++;
        } else if (optind + 1 < argc) {
            optarg = argv[optind + 1];
            optind += 2;
        } else {
            optopt = letter;
            optind++;
            at = 0;
            if (opterr && options[0] != ':') {
                fprintf(stderr, "%s: option -%c needs an argument\n", argv[0], letter);
            }
            return options[0] == ':' ? ':' : '?';
        }
        at = 0;
        return letter;
    }
    if (word[at] == '\0') {
        optind++;
        at = 0;
    }
    return letter;
}
