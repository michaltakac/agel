/* Floating point and the library calls DOOM needs: the unit the kernel
   turned on, math.h, strtod, fseek and ftell, remove, strcasecmp. Each
   check prints its result as a scaled integer when it fails, since the
   formatter has no %f, and the count of checks passed ends the report. */
#include <errno.h>
#include <math.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <strings.h>
#include <sys/stat.h>

static int passed;
static int failed;

static void check(int ok, const char *what, long value) {
    if (ok) {
        passed++;
    } else {
        failed++;
        printf("float: FAILED %s (%ld)\n", what, value);
    }
}

static long scaled(double value) { return (long)(value * 1000000.0); }

int main(void) {
    check(scaled(sqrt(2.0)) == 1414213, "sqrt 2", scaled(sqrt(2.0)));
    check(scaled(atan(1.0) * 4) == 3141592, "atan 1 times 4", scaled(atan(1.0) * 4));
    check(scaled(sin(M_PI / 6)) == 500000 || scaled(sin(M_PI / 6)) == 499999, "sin pi/6", scaled(sin(M_PI / 6)));
    check(scaled(cos(0)) == 1000000, "cos 0", scaled(cos(0)));
    check(floor(-2.5) == -3 && ceil(2.1) == 3, "floor and ceil", (long)floor(-2.5));
    check(scaled(fabs(-1.5)) == 1500000, "fabs", scaled(fabs(-1.5)));
    check(scaled(fmod(7.5, 2)) == 1500000, "fmod", scaled(fmod(7.5, 2)));
    check(pow(2, 10) == 1024 && scaled(pow(2, -2)) == 250000, "pow", (long)pow(2, 10));
    check(scaled(atof("2.5") * 4) == 10000000, "atof", scaled(atof("2.5")));
    char *end;
    double thousand = strtod(" 1e3x", &end);
    check(scaled(thousand) == 1000000000 && *end == 'x', "strtod", scaled(thousand));
    float ratio = 1.0f / 3.0f;
    check((int)(ratio * 3000) == 999 || (int)(ratio * 3000) == 1000, "float division", (long)(ratio * 3000));
    check(strcasecmp("Agel", "AGEL") == 0 && strncasecmp("Agelos", "AGEL", 4) == 0 &&
              strcasecmp("a", "b") < 0,
          "strcasecmp", strcasecmp("Agel", "AGEL"));
    /* A stream seeks and tells, then the file is removed. */
    FILE *file = fopen("float.txt", "w+");
    check(file != NULL, "fopen w+", errno);
    if (file != NULL) {
        fprintf(file, "0123456789");
        check(ftell(file) == 10, "ftell after writing", ftell(file));
        check(fseek(file, 4, SEEK_SET) == 0 && fgetc(file) == '4', "fseek set", ftell(file));
        check(ftell(file) == 5, "ftell after a read", ftell(file));
        check(fseek(file, -2, SEEK_CUR) == 0 && fgetc(file) == '3', "fseek cur", ftell(file));
        check(fseek(file, 0, SEEK_END) == 0 && ftell(file) == 10, "fseek end", ftell(file));
        rewind(file);
        check(fgetc(file) == '0', "rewind", ftell(file));
        fclose(file);
        struct stat info;
        check(remove("float.txt") == 0 && stat("float.txt", &info) < 0 && errno == ENOENT,
              "remove", errno);
    }
    printf("float: %d checks passed%s\n", passed, failed ? ", some FAILED" : "");
    return failed ? 1 : 0;
}
