/* Streams and formatting, in C because a C-variadic definition is not stable
   Rust. A FILE is a descriptor with a write buffer and a read buffer; the
   formatter handles %s %c %d %i %u %x %X %o %p %f %e %g %% with the
   flags - 0 + and space, a width, a precision, and the l, ll, z and h
   modifiers. */
#include <errno.h>
#include <fcntl.h>
#include <limits.h>
#include <stdarg.h>
#include <stddef.h>
#include <stdio.h>
#include <string.h>
#include <unistd.h>

#define STREAMS 8
#define STREAM_BUFFER 512

struct __agel_file {
    int descriptor;
    int used;
    int error;
    int eof;
    size_t out_used;
    size_t in_used;
    size_t in_at;
    /* One character given back by the scanner; -1 when none. */
    int pushback;
    char out[STREAM_BUFFER];
    char in[STREAM_BUFFER];
};

static FILE streams[STREAMS];
FILE *stdin = &streams[0];
FILE *stdout = &streams[1];
FILE *stderr = &streams[2];
static int streams_ready;

static void streams_init(void) {
    if (!streams_ready) {
        streams_ready = 1;
        for (int index = 0; index < 3; index++) {
            streams[index].descriptor = index;
            streams[index].used = 1;
        }
    }
}

int fflush(FILE *stream) {
    streams_init();
    if (stream == NULL) {
        for (int index = 0; index < STREAMS; index++) {
            if (streams[index].used) {
                fflush(&streams[index]);
            }
        }
        return 0;
    }
    size_t done = 0;
    while (done < stream->out_used) {
        ssize_t written = write(stream->descriptor, stream->out + done, stream->out_used - done);
        if (written <= 0) {
            stream->error = 1;
            stream->out_used = 0;
            return EOF;
        }
        done += (size_t)written;
    }
    stream->out_used = 0;
    return 0;
}

void __agel_flush_all(void) {
    fflush(NULL);
}

FILE *fopen(const char *path, const char *mode) {
    streams_init();
    int flags;
    switch (mode[0]) {
    case 'r':
        flags = (mode[1] == '+' || (mode[1] && mode[2] == '+')) ? O_RDWR : O_RDONLY;
        break;
    case 'w':
        flags = O_CREAT | ((mode[1] == '+' || (mode[1] && mode[2] == '+')) ? O_RDWR : O_WRONLY);
        break;
    case 'a':
        flags = O_CREAT | O_APPEND | ((mode[1] == '+' || (mode[1] && mode[2] == '+')) ? O_RDWR : O_WRONLY);
        break;
    default:
        errno = EINVAL;
        return NULL;
    }
    FILE *stream = NULL;
    for (int index = 3; index < STREAMS; index++) {
        if (!streams[index].used) {
            stream = &streams[index];
            break;
        }
    }
    if (stream == NULL) {
        errno = EMFILE;
        return NULL;
    }
    int descriptor = open(path, flags);
    if (descriptor < 0) {
        return NULL;
    }
    memset(stream, 0, sizeof *stream);
    stream->descriptor = descriptor;
    stream->used = 1;
    return stream;
}

int fclose(FILE *stream) {
    if (stream == NULL || !stream->used) {
        return EOF;
    }
    int result = fflush(stream);
    if (stream->descriptor > 2) {
        close(stream->descriptor);
        stream->used = 0;
    }
    return result;
}

int fileno(FILE *stream) {
    return stream ? stream->descriptor : -1;
}

/* What the stream has taken from the descriptor and not given out: the
   read buffer's rest and a character given back. */
static long unread(FILE *stream) {
    return (long)(stream->in_used - stream->in_at) + (stream->pushback > 0 ? 1 : 0);
}

int fseek(FILE *stream, long offset, int whence) {
    if (stream == NULL || !stream->used) {
        errno = EBADF;
        return -1;
    }
    if (stream->out_used != 0 && fflush(stream) != 0) {
        return -1;
    }
    if (whence == SEEK_CUR) {
        offset -= unread(stream);
    }
    stream->in_used = 0;
    stream->in_at = 0;
    stream->pushback = 0;
    stream->eof = 0;
    return lseek(stream->descriptor, offset, whence) < 0 ? -1 : 0;
}

long ftell(FILE *stream) {
    if (stream == NULL || !stream->used) {
        errno = EBADF;
        return -1;
    }
    long at = lseek(stream->descriptor, 0, SEEK_CUR);
    if (at < 0) {
        return -1;
    }
    return at - unread(stream) + (long)stream->out_used;
}

void rewind(FILE *stream) {
    fseek(stream, 0, SEEK_SET);
    if (stream != NULL) {
        stream->error = 0;
    }
}

int feof(FILE *stream) {
    return stream->eof;
}

int ferror(FILE *stream) {
    return stream->error;
}

void clearerr(FILE *stream) {
    stream->eof = 0;
    stream->error = 0;
}

int fputc(int character, FILE *stream) {
    streams_init();
    if (stream->out_used == STREAM_BUFFER && fflush(stream) != 0) {
        return EOF;
    }
    stream->out[stream->out_used++] = (char)character;
    /* The console and pipes are line buffered: a line is seen when it ends. */
    if (character == '\n' && fflush(stream) != 0) {
        return EOF;
    }
    return (unsigned char)character;
}

int putc(int character, FILE *stream) {
    return fputc(character, stream);
}

int putchar(int character) {
    return fputc(character, stdout);
}

int fputs(const char *text, FILE *stream) {
    while (*text) {
        if (fputc(*text++, stream) == EOF) {
            return EOF;
        }
    }
    return 0;
}

int puts(const char *text) {
    if (fputs(text, stdout) == EOF || fputc('\n', stdout) == EOF) {
        return EOF;
    }
    return 1;
}

size_t fwrite(const void *data, size_t size, size_t count, FILE *stream) {
    const char *bytes = data;
    size_t total = size * count;
    for (size_t index = 0; index < total; index++) {
        if (fputc(bytes[index], stream) == EOF) {
            return index / (size ? size : 1);
        }
    }
    return count;
}

static int fill(FILE *stream) {
    if (stream->in_at < stream->in_used) {
        return 1;
    }
    ssize_t count = read(stream->descriptor, stream->in, STREAM_BUFFER);
    if (count < 0) {
        stream->error = 1;
        return 0;
    }
    if (count == 0) {
        stream->eof = 1;
        return 0;
    }
    stream->in_used = (size_t)count;
    stream->in_at = 0;
    return 1;
}

int fgetc(FILE *stream) {
    streams_init();
    if (stream->pushback > 0) {
        int character = stream->pushback - 1;
        stream->pushback = 0;
        return character;
    }
    if (!fill(stream)) {
        return EOF;
    }
    return (unsigned char)stream->in[stream->in_at++];
}

int ungetc(int character, FILE *stream) {
    if (character == EOF || stream->pushback > 0) {
        return EOF;
    }
    stream->pushback = (unsigned char)character + 1;
    stream->eof = 0;
    return (unsigned char)character;
}

int getc(FILE *stream) {
    return fgetc(stream);
}

int getchar(void) {
    return fgetc(stdin);
}

char *fgets(char *buffer, int size, FILE *stream) {
    if (size <= 0) {
        return NULL;
    }
    int at = 0;
    while (at < size - 1) {
        int character = fgetc(stream);
        if (character == EOF) {
            break;
        }
        buffer[at++] = (char)character;
        if (character == '\n') {
            break;
        }
    }
    if (at == 0) {
        return NULL;
    }
    buffer[at] = '\0';
    return buffer;
}

size_t fread(void *data, size_t size, size_t count, FILE *stream) {
    char *bytes = data;
    size_t total = size * count;
    size_t done = 0;
    while (done < total) {
        int character = fgetc(stream);
        if (character == EOF) {
            break;
        }
        bytes[done++] = (char)character;
    }
    return size ? done / size : 0;
}

/* ---- the formatter ---------------------------------------------------- */

struct sink {
    FILE *stream;
    char *buffer;
    size_t capacity;
    size_t written;
    int failed;
};

static void emit(struct sink *sink, char byte) {
    if (sink->stream) {
        if (fputc(byte, sink->stream) == EOF) {
            sink->failed = 1;
        }
    } else if (sink->written + 1 < sink->capacity) {
        sink->buffer[sink->written] = byte;
    }
    sink->written++;
}

/* A width or precision digit appended to `value`, held below a bound a
   buffer here could ever need, so a run of digits cannot overflow an int. */
static int clamp_digit(int value, int digit) {
    return value >= 100000 ? value : value * 10 + digit;
}

struct spec {
    int left;
    int zero;
    int plus;
    int space;
    int width;
    int precision;
};

static void pad(struct sink *sink, int count, char byte) {
    while (count-- > 0) {
        emit(sink, byte);
    }
}

static void emit_text(struct sink *sink, const char *text, size_t length, const struct spec *spec) {
    int padding = spec->width > (int)length ? spec->width - (int)length : 0;
    if (!spec->left) {
        pad(sink, padding, ' ');
    }
    for (size_t index = 0; index < length; index++) {
        emit(sink, text[index]);
    }
    if (spec->left) {
        pad(sink, padding, ' ');
    }
}

static void emit_number(struct sink *sink, unsigned long long value, int negative, unsigned base, int upper,
                        const struct spec *spec) {
    char digits[32];
    int count = 0;
    if (value == 0 && spec->precision != 0) {
        digits[count++] = '0';
    }
    while (value) {
        unsigned digit = (unsigned)(value % base);
        digits[count++] = (char)(digit < 10 ? '0' + digit : (upper ? 'A' : 'a') + digit - 10);
        value /= base;
    }
    int zeros = spec->precision > count ? spec->precision - count : 0;
    char sign = negative ? '-' : spec->plus ? '+' : spec->space ? ' ' : 0;
    int length = count + zeros + (sign ? 1 : 0);
    int padding = spec->width > length ? spec->width - length : 0;
    if (!spec->left && !(spec->zero && spec->precision < 0)) {
        pad(sink, padding, ' ');
    }
    if (sign) {
        emit(sink, sign);
    }
    if (!spec->left && spec->zero && spec->precision < 0) {
        pad(sink, padding, '0');
    }
    pad(sink, zeros, '0');
    while (count) {
        emit(sink, digits[--count]);
    }
    if (spec->left) {
        pad(sink, padding, ' ');
    }
}

/* At most this many digits after the point: a double carries no more
   information, and it bounds the buffers below. */
#define FRACTION_MAX 40

/* The digits of a non-negative integral double, most significant first,
   into `out` (which holds any double: 309 digits). Exact below 2^53, since
   each quotient is then a correctly rounded division of representable
   values; beyond that the digits are the double's own approximation. */
/* The integral part of a non-negative double; one at or above 2^52 is
   already integral, below that the conversion through long long is exact. */
static double integral(double value) {
    return value >= 4503599627370496.0 ? value : (double)(long long)value;
}

static int whole_digits(double whole, char *out) {
    char reversed[320];
    int count = 0;
    while (whole >= 1 && count < (int)sizeof reversed) {
        double quotient = integral(whole / 10);
        if (quotient * 10 > whole) {
            quotient -= 1;
        }
        int digit = (int)(whole - quotient * 10);
        reversed[count++] = (char)('0' + (digit < 0 ? 0 : digit > 9 ? 9 : digit));
        whole = quotient;
    }
    if (count == 0) {
        reversed[count++] = '0';
    }
    for (int index = 0; index < count; index++) {
        out[index] = reversed[count - 1 - index];
    }
    return count;
}

/* `precision` digits of a fraction in [0, 1), rounded half up on the digit
   after them; returns the carry into the units place. */
static int fraction_digits(double fraction, int precision, char *out) {
    for (int index = 0; index < precision; index++) {
        fraction *= 10;
        int digit = (int)fraction;
        out[index] = (char)('0' + digit);
        fraction -= digit;
    }
    int carry = fraction >= 0.5;
    for (int index = precision - 1; carry && index >= 0; index--) {
        if (out[index] == '9') {
            out[index] = '0';
        } else {
            out[index]++;
            carry = 0;
        }
    }
    return carry;
}

static void emit_padded(struct sink *sink, const char *text, int length, int negative, const struct spec *spec,
                        int zero_ok) {
    char sign = negative ? '-' : spec->plus ? '+' : spec->space ? ' ' : 0;
    int total = length + (sign ? 1 : 0);
    int padding = spec->width > total ? spec->width - total : 0;
    int zeros = zero_ok && spec->zero && !spec->left;
    if (!spec->left && !zeros) {
        pad(sink, padding, ' ');
    }
    if (sign) {
        emit(sink, sign);
    }
    if (zeros) {
        pad(sink, padding, '0');
    }
    for (int index = 0; index < length; index++) {
        emit(sink, text[index]);
    }
    if (spec->left) {
        pad(sink, padding, ' ');
    }
}

/* Decimal exponent and mantissa in [1, 10) of a positive finite double. */
static int decompose(double *value) {
    int exponent = 0;
    while (*value >= 10) {
        *value /= 10;
        exponent++;
    }
    while (*value < 1) {
        *value *= 10;
        exponent--;
    }
    return exponent;
}

static int fixed_text(double value, int precision, char *text) {
    double whole = integral(value);
    char fraction[FRACTION_MAX];
    int carry = fraction_digits(value - whole, precision, fraction);
    int length = whole_digits(whole + carry, text);
    if (precision > 0) {
        text[length++] = '.';
        for (int index = 0; index < precision; index++) {
            text[length++] = fraction[index];
        }
    }
    return length;
}

static int exponent_text(double value, int precision, int upper, char *text) {
    int exponent = value == 0 ? 0 : decompose(&value);
    int lead = (int)value;
    char fraction[FRACTION_MAX];
    if (fraction_digits(value - lead, precision, fraction)) {
        lead++;
        if (lead == 10) {
            lead = 1;
            exponent++;
        }
    }
    int length = 0;
    text[length++] = (char)('0' + lead);
    if (precision > 0) {
        text[length++] = '.';
        for (int index = 0; index < precision; index++) {
            text[length++] = fraction[index];
        }
    }
    text[length++] = upper ? 'E' : 'e';
    text[length++] = exponent < 0 ? '-' : '+';
    int magnitude = exponent < 0 ? -exponent : exponent;
    if (magnitude >= 100) {
        text[length++] = (char)('0' + magnitude / 100);
    }
    text[length++] = (char)('0' + magnitude / 10 % 10);
    text[length++] = (char)('0' + magnitude % 10);
    return length;
}

/* Strip the trailing zeros of a fraction, and the point if nothing follows
   it, as %g does; an exponent suffix is kept. */
static int strip_zeros(char *text, int length) {
    int end = length;
    while (end > 0 && text[end - 1] != 'e' && text[end - 1] != 'E') {
        end--;
    }
    int mantissa = end > 0 ? end - 1 : length;
    int has_point = 0;
    for (int index = 0; index < mantissa; index++) {
        has_point |= text[index] == '.';
    }
    if (!has_point) {
        return length;
    }
    int cut = mantissa;
    while (cut > 0 && text[cut - 1] == '0') {
        cut--;
    }
    if (cut > 0 && text[cut - 1] == '.') {
        cut--;
    }
    for (int index = mantissa; index < length; index++) {
        text[cut++] = text[index];
    }
    return cut;
}

static void emit_double(struct sink *sink, double value, char conversion, const struct spec *spec) {
    int upper = conversion == 'F' || conversion == 'E' || conversion == 'G';
    int negative = value < 0;
    if (negative) {
        value = -value;
    }
    if (value != value) {
        emit_padded(sink, upper ? "NAN" : "nan", 3, 0, spec, 0);
        return;
    }
    if (value > 1.7976931348623157e308) {
        emit_padded(sink, upper ? "INF" : "inf", 3, negative, spec, 0);
        return;
    }
    int precision = spec->precision < 0 ? 6 : spec->precision > FRACTION_MAX ? FRACTION_MAX : spec->precision;
    char text[320 + FRACTION_MAX + 8];
    int length;
    if (conversion == 'f' || conversion == 'F') {
        length = fixed_text(value, precision, text);
    } else if (conversion == 'e' || conversion == 'E') {
        length = exponent_text(value, precision, upper, text);
    } else {
        int significant = precision == 0 ? 1 : precision;
        double mantissa = value;
        int exponent = value == 0 ? 0 : decompose(&mantissa);
        if (exponent < -4 || exponent >= significant) {
            length = exponent_text(value, significant - 1, upper, text);
        } else {
            length = fixed_text(value, significant - 1 - exponent, text);
        }
        length = strip_zeros(text, length);
    }
    emit_padded(sink, text, length, negative, spec, 1);
}

static void format(struct sink *sink, const char *format, va_list arguments) {
    for (const char *at = format; *at; at++) {
        if (*at != '%') {
            emit(sink, *at);
            continue;
        }
        at++;
        struct spec spec = {0, 0, 0, 0, 0, -1};
        for (;; at++) {
            if (*at == '-') {
                spec.left = 1;
            } else if (*at == '0') {
                spec.zero = 1;
            } else if (*at == '+') {
                spec.plus = 1;
            } else if (*at == ' ') {
                spec.space = 1;
            } else {
                break;
            }
        }
        if (*at == '*') {
            spec.width = va_arg(arguments, int);
            if (spec.width < 0) {
                spec.left = 1;
                spec.width = -spec.width;
            }
            at++;
        } else {
            while (*at >= '0' && *at <= '9') {
                spec.width = clamp_digit(spec.width, *at++ - '0');
            }
        }
        if (*at == '.') {
            at++;
            spec.precision = 0;
            if (*at == '*') {
                spec.precision = va_arg(arguments, int);
                at++;
            } else {
                while (*at >= '0' && *at <= '9') {
                    spec.precision = clamp_digit(spec.precision, *at++ - '0');
                }
            }
        }
        int longs = 0;
        int shorts = 0;
        for (;; at++) {
            if (*at == 'l') {
                longs++;
            } else if (*at == 'z' || *at == 'j' || *at == 't') {
                longs = 1;
            } else if (*at == 'h') {
                shorts++;
            } else {
                break;
            }
        }
        switch (*at) {
        case 's': {
            const char *text = va_arg(arguments, const char *);
            if (text == NULL) {
                text = "(null)";
            }
            size_t length = strlen(text);
            if (spec.precision >= 0 && (size_t)spec.precision < length) {
                length = (size_t)spec.precision;
            }
            emit_text(sink, text, length, &spec);
            break;
        }
        case 'c': {
            char character = (char)va_arg(arguments, int);
            emit_text(sink, &character, 1, &spec);
            break;
        }
        case 'd':
        case 'i': {
            long long value = longs >= 2 ? va_arg(arguments, long long)
                              : longs == 1 ? va_arg(arguments, long)
                                           : va_arg(arguments, int);
            if (shorts == 1) {
                value = (short)value;
            } else if (shorts >= 2) {
                value = (signed char)value;
            }
            unsigned long long magnitude = value < 0 ? (unsigned long long)(-(value + 1)) + 1 : (unsigned long long)value;
            emit_number(sink, magnitude, value < 0, 10, 0, &spec);
            break;
        }
        case 'u':
        case 'x':
        case 'X':
        case 'o': {
            unsigned long long value = longs >= 2 ? va_arg(arguments, unsigned long long)
                                       : longs == 1 ? va_arg(arguments, unsigned long)
                                                    : va_arg(arguments, unsigned);
            if (shorts == 1) {
                value = (unsigned short)value;
            } else if (shorts >= 2) {
                value = (unsigned char)value;
            }
            unsigned base = *at == 'u' ? 10 : *at == 'o' ? 8 : 16;
            emit_number(sink, value, 0, base, *at == 'X', &spec);
            break;
        }
        case 'f':
        case 'F':
        case 'e':
        case 'E':
        case 'g':
        case 'G':
            emit_double(sink, va_arg(arguments, double), *at, &spec);
            break;
        case 'p': {
            unsigned long long value = (unsigned long long)(size_t)va_arg(arguments, void *);
            emit(sink, '0');
            emit(sink, 'x');
            emit_number(sink, value, 0, 16, 0, &spec);
            break;
        }
        case '%':
            emit(sink, '%');
            break;
        case '\0':
            at--;
            break;
        default:
            emit(sink, '%');
            emit(sink, *at);
            break;
        }
    }
}

int vfprintf(FILE *stream, const char *text, va_list arguments) {
    streams_init();
    struct sink sink = {stream, NULL, 0, 0, 0};
    format(&sink, text, arguments);
    return sink.failed ? -1 : (int)sink.written;
}

int fprintf(FILE *stream, const char *text, ...) {
    va_list arguments;
    va_start(arguments, text);
    int result = vfprintf(stream, text, arguments);
    va_end(arguments);
    return result;
}

int vprintf(const char *text, va_list arguments) {
    return vfprintf(stdout, text, arguments);
}

int printf(const char *text, ...) {
    va_list arguments;
    va_start(arguments, text);
    int result = vfprintf(stdout, text, arguments);
    va_end(arguments);
    return result;
}

int vsnprintf(char *buffer, size_t capacity, const char *text, va_list arguments) {
    struct sink sink = {NULL, buffer, capacity, 0, 0};
    format(&sink, text, arguments);
    if (capacity > 0) {
        buffer[sink.written < capacity ? sink.written : capacity - 1] = '\0';
    }
    return (int)sink.written;
}

int snprintf(char *buffer, size_t capacity, const char *text, ...) {
    va_list arguments;
    va_start(arguments, text);
    int result = vsnprintf(buffer, capacity, text, arguments);
    va_end(arguments);
    return result;
}

int vsprintf(char *buffer, const char *text, va_list arguments) {
    return vsnprintf(buffer, (size_t)-1, text, arguments);
}

int sprintf(char *buffer, const char *text, ...) {
    va_list arguments;
    va_start(arguments, text);
    int result = vsnprintf(buffer, (size_t)-1, text, arguments);
    va_end(arguments);
    return result;
}

void perror(const char *prefix) {
    if (prefix && *prefix) {
        fprintf(stderr, "%s: %s\n", prefix, strerror(errno));
    } else {
        fprintf(stderr, "%s\n", strerror(errno));
    }
}

/* ------------------------------------------------------------------------
   The scanner: %d %i %u %x %X %o %s %c %% %n, a width, the l, ll and h
   modifiers, whitespace in the format matching any amount of input
   whitespace. It reads one character at a time from a source that can give
   one back: a string, or a stream through ungetc.
   ------------------------------------------------------------------------ */

struct source {
    const char *text;
    FILE *stream;
    int consumed;
};

static int take(struct source *source) {
    int character;
    if (source->text) {
        character = (unsigned char)*source->text;
        if (character == 0) {
            return EOF;
        }
        source->text++;
    } else {
        character = fgetc(source->stream);
        if (character == EOF) {
            return EOF;
        }
    }
    source->consumed++;
    return character;
}

static void give_back(struct source *source, int character) {
    if (character == EOF) {
        return;
    }
    if (source->text) {
        source->text--;
    } else {
        ungetc(character, source->stream);
    }
    source->consumed--;
}

static int is_space(int character) {
    return character == ' ' || character == '\t' || character == '\n' || character == '\r' ||
           character == '\f' || character == '\v';
}

static int digit_value(int character, unsigned base) {
    unsigned value;
    if (character >= '0' && character <= '9') {
        value = (unsigned)(character - '0');
    } else if (character >= 'a' && character <= 'z') {
        value = (unsigned)(character - 'a' + 10);
    } else if (character >= 'A' && character <= 'Z') {
        value = (unsigned)(character - 'A' + 10);
    } else {
        return -1;
    }
    return value < base ? (int)value : -1;
}

static void skip_space(struct source *source) {
    int character;
    while ((character = take(source)) != EOF && is_space(character)) {
    }
    give_back(source, character);
}

/* A number in `base` (0 for %i's choice), at most `width` characters:
   1 when one was read, 0 when the input did not start one, -1 at the end. */
static int scan_number(struct source *source, unsigned base, int width, unsigned long long *value,
                       int *negative) {
    int character = take(source);
    int used = 0;
    *negative = 0;
    *value = 0;
    if (character == EOF) {
        return -1;
    }
    if ((character == '-' || character == '+') && width != 1) {
        *negative = character == '-';
        character = take(source);
        used++;
    }
    if (base == 0 || base == 16) {
        if (character == '0' && (width < 0 || used + 1 < width)) {
            int next = take(source);
            if (next == 'x' || next == 'X') {
                base = 16;
                character = take(source);
                used += 2;
            } else {
                give_back(source, next);
                if (base == 0) {
                    base = 8;
                }
            }
        } else if (base == 0) {
            base = 10;
        }
    }
    int digits = 0;
    while (character != EOF && (width < 0 || used < width)) {
        int digit = digit_value(character, base);
        if (digit < 0) {
            break;
        }
        /* A number wider than the type saturates rather than wrapping. */
        if (*value > (ULONG_MAX - (unsigned)digit) / (unsigned)base) {
            *value = ULONG_MAX;
        } else {
            *value = *value * base + (unsigned)digit;
        }
        digits++;
        used++;
        character = take(source);
    }
    give_back(source, character);
    return digits > 0 ? 1 : 0;
}

static int scan(struct source *source, const char *format, va_list arguments) {
    int assigned = 0;
    for (const char *at = format; *at; at++) {
        if (is_space((unsigned char)*at)) {
            skip_space(source);
            continue;
        }
        if (*at != '%') {
            int character = take(source);
            if (character != (unsigned char)*at) {
                give_back(source, character);
                return assigned;
            }
            continue;
        }
        at++;
        int suppress = 0;
        if (*at == '*') {
            suppress = 1;
            at++;
        }
        int width = -1;
        while (*at >= '0' && *at <= '9') {
            width = clamp_digit(width < 0 ? 0 : width, *at - '0');
            at++;
        }
        int longs = 0;
        int shorts = 0;
        while (*at == 'l' || *at == 'h' || *at == 'z') {
            if (*at == 'l' || *at == 'z') {
                longs++;
            } else {
                shorts++;
            }
            at++;
        }
        char conversion = *at;
        if (conversion == 0) {
            return assigned;
        }
        if (conversion == '%') {
            skip_space(source);
            int character = take(source);
            if (character != '%') {
                give_back(source, character);
                return assigned;
            }
            continue;
        }
        if (conversion == 'n') {
            if (!suppress) {
                *va_arg(arguments, int *) = source->consumed;
            }
            continue;
        }
        if (conversion == 'c') {
            int count = width < 0 ? 1 : width;
            char *out = suppress ? NULL : va_arg(arguments, char *);
            for (int index = 0; index < count; index++) {
                int character = take(source);
                if (character == EOF) {
                    return assigned == 0 && index == 0 ? EOF : assigned;
                }
                if (out) {
                    out[index] = (char)character;
                }
            }
            if (!suppress) {
                assigned++;
            }
            continue;
        }
        skip_space(source);
        if (conversion == 's') {
            char *out = suppress ? NULL : va_arg(arguments, char *);
            int used = 0;
            int character = take(source);
            if (character == EOF) {
                return assigned == 0 ? EOF : assigned;
            }
            while (character != EOF && !is_space(character) && (width < 0 || used < width)) {
                if (out) {
                    out[used] = (char)character;
                }
                used++;
                character = take(source);
            }
            give_back(source, character);
            if (out) {
                out[used] = 0;
            }
            if (!suppress) {
                assigned++;
            }
            continue;
        }
        unsigned base;
        int is_signed = 0;
        switch (conversion) {
        case 'd': base = 10; is_signed = 1; break;
        case 'i': base = 0; is_signed = 1; break;
        case 'u': base = 10; break;
        case 'x': case 'X': base = 16; break;
        case 'o': base = 8; break;
        default: return assigned;
        }
        unsigned long long value;
        int negative;
        int got = scan_number(source, base, width, &value, &negative);
        if (got < 0) {
            return assigned == 0 ? EOF : assigned;
        }
        if (got == 0) {
            return assigned;
        }
        if (suppress) {
            continue;
        }
        if (is_signed) {
            long long number = negative ? -(long long)value : (long long)value;
            if (longs >= 2) {
                *va_arg(arguments, long long *) = number;
            } else if (longs == 1) {
                *va_arg(arguments, long *) = (long)number;
            } else if (shorts) {
                *va_arg(arguments, short *) = (short)number;
            } else {
                *va_arg(arguments, int *) = (int)number;
            }
        } else {
            unsigned long long number = negative ? (unsigned long long)-(long long)value : value;
            if (longs >= 2) {
                *va_arg(arguments, unsigned long long *) = number;
            } else if (longs == 1) {
                *va_arg(arguments, unsigned long *) = (unsigned long)number;
            } else if (shorts) {
                *va_arg(arguments, unsigned short *) = (unsigned short)number;
            } else {
                *va_arg(arguments, unsigned *) = (unsigned)number;
            }
        }
        assigned++;
    }
    return assigned;
}

int vsscanf(const char *text, const char *format, va_list arguments) {
    struct source source = {text, NULL, 0};
    return scan(&source, format, arguments);
}

int sscanf(const char *text, const char *format, ...) {
    va_list arguments;
    va_start(arguments, format);
    int result = vsscanf(text, format, arguments);
    va_end(arguments);
    return result;
}

int vfscanf(FILE *stream, const char *format, va_list arguments) {
    streams_init();
    struct source source = {NULL, stream, 0};
    return scan(&source, format, arguments);
}

int fscanf(FILE *stream, const char *format, ...) {
    va_list arguments;
    va_start(arguments, format);
    int result = vfscanf(stream, format, arguments);
    va_end(arguments);
    return result;
}

int scanf(const char *format, ...) {
    va_list arguments;
    va_start(arguments, format);
    int result = vfscanf(stdin, format, arguments);
    va_end(arguments);
    return result;
}
