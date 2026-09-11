/* Streams and formatting, in C because a C-variadic definition is not stable
   Rust. A FILE is a descriptor with a write buffer and a read buffer; the
   formatter handles %s %c %d %i %u %x %X %o %p %% with the flags - 0 +
   and space, a width, a precision, and the l, ll, z and h modifiers. */
#include <errno.h>
#include <fcntl.h>
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
    if (!fill(stream)) {
        return EOF;
    }
    return (unsigned char)stream->in[stream->in_at++];
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
                spec.width = spec.width * 10 + (*at++ - '0');
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
                    spec.precision = spec.precision * 10 + (*at++ - '0');
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
