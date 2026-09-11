/* printf, in C because a C-variadic definition is not stable Rust. Formats
   into a fixed buffer and writes it; conversions: %s %d %i %u %x %c %ld %lu
   %lx %zu %zd %p %%. Anything else is copied through unchanged. */
#include <stdarg.h>
#include <stddef.h>
#include <stdio.h>
#include <string.h>
#include <unistd.h>

#define BUFFER_BYTES 512

struct sink {
    char bytes[BUFFER_BYTES];
    size_t used;
    int written;
    int failed;
};

static void flush(struct sink *sink) {
    if (sink->used > 0 && !sink->failed) {
        if (write(1, sink->bytes, sink->used) != (long)sink->used) {
            sink->failed = 1;
        }
        sink->written += (int)sink->used;
        sink->used = 0;
    }
}

static void put(struct sink *sink, char byte) {
    if (sink->used == BUFFER_BYTES) {
        flush(sink);
    }
    sink->bytes[sink->used++] = byte;
}

static void put_text(struct sink *sink, const char *text) {
    while (*text) {
        put(sink, *text++);
    }
}

static void put_unsigned(struct sink *sink, unsigned long value, unsigned base) {
    char digits[24];
    size_t count = 0;
    do {
        unsigned digit = (unsigned)(value % base);
        digits[count++] = (char)(digit < 10 ? '0' + digit : 'a' + digit - 10);
        value /= base;
    } while (value);
    while (count) {
        put(sink, digits[--count]);
    }
}

static void put_signed(struct sink *sink, long value) {
    if (value < 0) {
        put(sink, '-');
        put_unsigned(sink, (unsigned long)(-(value + 1)) + 1, 10);
    } else {
        put_unsigned(sink, (unsigned long)value, 10);
    }
}

int printf(const char *format, ...) {
    struct sink sink = {{0}, 0, 0, 0};
    va_list arguments;
    va_start(arguments, format);
    for (const char *at = format; *at; at++) {
        if (*at != '%') {
            put(&sink, *at);
            continue;
        }
        at++;
        int is_long = 0;
        if (*at == 'l' || *at == 'z') {
            is_long = 1;
            at++;
        }
        switch (*at) {
        case 's': {
            const char *text = va_arg(arguments, const char *);
            put_text(&sink, text ? text : "(null)");
            break;
        }
        case 'd':
        case 'i':
            put_signed(&sink, is_long ? va_arg(arguments, long) : (long)va_arg(arguments, int));
            break;
        case 'u':
            put_unsigned(&sink, is_long ? va_arg(arguments, unsigned long) : (unsigned long)va_arg(arguments, unsigned), 10);
            break;
        case 'x':
            put_unsigned(&sink, is_long ? va_arg(arguments, unsigned long) : (unsigned long)va_arg(arguments, unsigned), 16);
            break;
        case 'p':
            put_text(&sink, "0x");
            put_unsigned(&sink, (unsigned long)va_arg(arguments, void *), 16);
            break;
        case 'c':
            put(&sink, (char)va_arg(arguments, int));
            break;
        case '%':
            put(&sink, '%');
            break;
        case '\0':
            at--;
            break;
        default:
            put(&sink, '%');
            put(&sink, *at);
            break;
        }
    }
    va_end(arguments);
    flush(&sink);
    return sink.failed ? -1 : sink.written;
}
