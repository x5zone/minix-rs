#include <stdio.h>
#include <string.h>
#include "md5.h"

static int chk(const char *label, const char *s, const char *exp) {
    char b[33];
    MD5Data((const unsigned char *)s, (unsigned int)strlen(s), b);
    int ok = (strcmp(b, exp) == 0);
    printf("%-16s -> %s  [%s] (want %s)\n", label, b, ok ? "OK" : "MISMATCH", exp);
    return ok;
}

int main(void) {
    int all = 1;
    /* RFC 1321 test suite */
    all &= chk("empty",       "", "d41d8cd98f00b204e9800998ecf8427e");
    all &= chk("a",           "a", "0cc175b9c0f1b6a831c399e269772661");
    all &= chk("abc",         "abc", "900150983cd24fb0d6963f7d28e17f72");
    all &= chk("message",     "message digest", "f96b697d7cb7938d525a2f31aaf161d0");
    all &= chk("alphabet",    "abcdefghijklmnopqrstuvwxyz", "c3fcd3d76192e4007dfb496cca67e13b");
    all &= chk("alnum",       "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789", "d174ab98d277d9f5a5611c2c9f419d9f");
    all &= chk("digits",      "12345678901234567890123456789012345678901234567890123456789012345678901234567890", "57edf4a22be3c955ac49da2e2107b67a");
    printf(all ? "MD5 SELF-TEST: ALL PASS\n" : "MD5 SELF-TEST: FAILED\n");
    return all ? 0 : 1;
}
