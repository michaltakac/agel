/* agel-libc: the environment, which is the process's own: nothing is
   inherited from :exec or a parent, so it starts empty and holds what the
   program puts in it, sixteen entries of at most 95 bytes. */
#include <errno.h>
#include <stdlib.h>
#include <string.h>

#define ENTRIES 16
#define BYTES 96

static char table[ENTRIES][BYTES];

static int matches(const char *entry, const char *name, size_t length) {
    return entry[0] != 0 && strncmp(entry, name, length) == 0 && entry[length] == '=';
}

char *getenv(const char *name) {
    size_t length = strlen(name);
    for (int index = 0; index < ENTRIES; index++) {
        if (matches(table[index], name, length)) {
            return &table[index][length + 1];
        }
    }
    return NULL;
}

int setenv(const char *name, const char *value, int overwrite) {
    size_t length = strlen(name);
    if (length == 0 || strchr(name, '=') != NULL) {
        errno = EINVAL;
        return -1;
    }
    if (length + 1 + strlen(value) + 1 > BYTES) {
        errno = ENOMEM;
        return -1;
    }
    int free_slot = -1;
    for (int index = 0; index < ENTRIES; index++) {
        if (matches(table[index], name, length)) {
            if (!overwrite) {
                return 0;
            }
            free_slot = index;
            break;
        }
        if (free_slot < 0 && table[index][0] == 0) {
            free_slot = index;
        }
    }
    if (free_slot < 0) {
        errno = ENOMEM;
        return -1;
    }
    memcpy(table[free_slot], name, length);
    table[free_slot][length] = '=';
    strcpy(&table[free_slot][length + 1], value);
    return 0;
}

int unsetenv(const char *name) {
    size_t length = strlen(name);
    for (int index = 0; index < ENTRIES; index++) {
        if (matches(table[index], name, length)) {
            table[index][0] = 0;
        }
    }
    return 0;
}

int putenv(char *string) {
    const char *equals = strchr(string, '=');
    if (equals == NULL) {
        errno = EINVAL;
        return -1;
    }
    char name[BYTES];
    size_t length = (size_t)(equals - string);
    if (length >= sizeof name) {
        errno = ENOMEM;
        return -1;
    }
    memcpy(name, string, length);
    name[length] = 0;
    return setenv(name, equals + 1, 1);
}
