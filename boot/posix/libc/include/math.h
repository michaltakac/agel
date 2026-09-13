/* agel-libc: what a program that mostly counts in fixed point still asks
   of the floating-point unit, which the kernel enables for processes on
   x86-64 and AArch64 (v0.2.70). No errno, no NaN handling beyond what the
   unit does. */
#ifndef AGEL_MATH_H
#define AGEL_MATH_H
#define M_PI 3.14159265358979323846
#define HUGE_VAL (1.0 / 0.0)
double fabs(double value);
float fabsf(float value);
double sqrt(double value);
float sqrtf(float value);
double floor(double value);
double ceil(double value);
double fmod(double value, double divisor);
double atan(double value);
double atan2(double y, double x);
double sin(double value);
double cos(double value);
double pow(double base, double exponent);
#endif
