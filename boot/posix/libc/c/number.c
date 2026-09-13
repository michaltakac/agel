/* strtod and atof, in C: the floating-point unit is the process's, and
   the C side of the library is the one built for the machine's hard-float
   calling convention, so a double is returned where a C caller reads it.
   Decimal digits, a point, an exponent; no hexadecimal, no infinities. */
#include <ctype.h>
#include <stdlib.h>

double strtod(const char *text, char **end) {
    const char *at = text;
    while (isspace((unsigned char)*at)) {
        at++;
    }
    int negative = 0;
    if (*at == '-' || *at == '+') {
        negative = *at == '-';
        at++;
    }
    double value = 0;
    int digits = 0;
    while (*at >= '0' && *at <= '9') {
        value = value * 10 + (*at - '0');
        at++;
        digits++;
    }
    if (*at == '.') {
        double scale = 0.1;
        at++;
        while (*at >= '0' && *at <= '9') {
            value += (*at - '0') * scale;
            scale *= 0.1;
            at++;
            digits++;
        }
    }
    if (digits == 0) {
        if (end) {
            *end = (char *)text;
        }
        return 0;
    }
    if ((*at | 0x20) == 'e') {
        const char *probe = at + 1;
        int exponent_negative = 0;
        if (*probe == '-' || *probe == '+') {
            exponent_negative = *probe == '-';
            probe++;
        }
        if (*probe >= '0' && *probe <= '9') {
            int exponent = 0;
            while (*probe >= '0' && *probe <= '9') {
                if (exponent < 400) {
                    exponent = exponent * 10 + (*probe - '0');
                }
                probe++;
            }
            double factor = 1;
            for (int step = 0; step < exponent; step++) {
                factor *= 10;
            }
            value = exponent_negative ? value / factor : value * factor;
            at = probe;
        }
    }
    if (end) {
        *end = (char *)at;
    }
    return negative ? -value : value;
}

double atof(const char *text) { return strtod(text, NULL); }
