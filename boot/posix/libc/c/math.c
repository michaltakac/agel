/* The floating-point functions a program that mostly counts in fixed
   point still asks for: exact where the unit is (square root by Newton's
   method to the last bit it holds, floor, ceil, fmod), and series with
   argument reduction for atan, sin and cos, good to about 1e-12, which is
   more than DOOM's tables ask. */
#include <math.h>

double fabs(double value) { return value < 0 ? -value : value; }
float fabsf(float value) { return value < 0 ? -value : value; }

double sqrt(double value) {
    if (value <= 0) {
        return value == 0 ? 0 : 0.0 / 0.0;
    }
    double guess = value > 1 ? value : 1;
    for (int step = 0; step < 64; step++) {
        double next = 0.5 * (guess + value / guess);
        if (next == guess) {
            break;
        }
        guess = next;
    }
    return guess;
}

float sqrtf(float value) { return (float)sqrt(value); }

double floor(double value) {
    if (value >= 9007199254740992.0 || value <= -9007199254740992.0) {
        return value;
    }
    long long whole = (long long)value;
    return (double)whole > value ? (double)(whole - 1) : (double)whole;
}

double ceil(double value) { return -floor(-value); }

double fmod(double value, double divisor) {
    if (divisor == 0) {
        return 0.0 / 0.0;
    }
    double quotient = value / divisor;
    double whole = quotient < 0 ? ceil(quotient) : floor(quotient);
    return value - whole * divisor;
}

/* atan on [0, tan(pi/8)] by its series, which converges fast there;
   larger arguments are folded with atan(x) = pi/2 - atan(1/x) and
   atan(x) = pi/4 + atan((x - 1) / (x + 1)). */
static double atan_small(double x) {
    double term = x;
    double sum = x;
    double square = x * x;
    for (int n = 1; n < 40; n++) {
        term *= -square;
        double addend = term / (2 * n + 1);
        if (addend == 0) {
            break;
        }
        sum += addend;
    }
    return sum;
}

double atan(double value) {
    if (value < 0) {
        return -atan(-value);
    }
    if (value > 1) {
        return M_PI / 2 - atan(1 / value);
    }
    if (value > 0.41421356237309503) {
        return M_PI / 4 + atan_small((value - 1) / (value + 1));
    }
    return atan_small(value);
}

double atan2(double y, double x) {
    if (x > 0) {
        return atan(y / x);
    }
    if (x < 0) {
        return y >= 0 ? atan(y / x) + M_PI : atan(y / x) - M_PI;
    }
    if (y > 0) {
        return M_PI / 2;
    }
    if (y < 0) {
        return -M_PI / 2;
    }
    return 0;
}

/* sin on [-pi, pi] by its series after reduction by 2 pi. */
double sin(double value) {
    double reduced = fmod(value, 2 * M_PI);
    if (reduced > M_PI) {
        reduced -= 2 * M_PI;
    } else if (reduced < -M_PI) {
        reduced += 2 * M_PI;
    }
    double term = reduced;
    double sum = reduced;
    double square = reduced * reduced;
    for (int n = 1; n < 30; n++) {
        term *= -square / ((2 * n) * (2 * n + 1));
        if (term == 0) {
            break;
        }
        sum += term;
    }
    return sum;
}

double cos(double value) { return sin(value + M_PI / 2); }

/* Integer exponents by squaring; nothing here asks for the others. */
double pow(double base, double exponent) {
    long long whole = (long long)exponent;
    if ((double)whole != exponent) {
        return 0.0 / 0.0;
    }
    double result = 1;
    double factor = base;
    unsigned long long count = whole < 0 ? (unsigned long long)(-whole) : (unsigned long long)whole;
    while (count) {
        if (count & 1) {
            result *= factor;
        }
        factor *= factor;
        count >>= 1;
    }
    return whole < 0 ? 1 / result : result;
}
