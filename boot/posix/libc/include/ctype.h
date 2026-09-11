/* agel-libc: character classes, for the C locale. */
#ifndef AGEL_CTYPE_H
#define AGEL_CTYPE_H

int isspace(int character);
int isdigit(int character);
int isalpha(int character);
int isalnum(int character);
int isupper(int character);
int islower(int character);
int isprint(int character);
int isxdigit(int character);
int toupper(int character);
int tolower(int character);

#endif
