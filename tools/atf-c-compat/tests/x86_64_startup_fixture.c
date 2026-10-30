/* x86_64 startup-stub child fixture.
 *
 * Linked with startup-minix.c under STARTUP_MINIX_TEST, so the startup object
 * provides `_start` (RBX = ps_strings -> RDI -> startup_c_main) and this file
 * provides `startup_c_main`. No libc: the child reports the descriptor it was
 * handed through a raw write(2), then raw exit(2), so the only ABI under test
 * is the kernel birth contract (RBX) and the SysV call into C.
 */
#include <stddef.h>

struct fixture_ps_strings {
    char **argv_str;
    int n_argv;
    char **env_str;
    int n_env;
};

static long raw_write(int fd, const void *buf, unsigned long len) {
    long ret;
    __asm__ volatile("syscall"
                     : "=a"(ret)
                     : "a"(1L), "D"((long)fd), "S"(buf), "d"(len)
                     : "rcx", "r11", "memory");
    return ret;
}

static void raw_exit(int code) {
    __asm__ volatile("syscall"
                     :
                     : "a"(60L), "D"((long)code)
                     : "rcx", "r11", "memory");
    __builtin_unreachable();
}

static int append_str(char *out, int at, const char *s) {
    while (*s) out[at++] = *s++;
    return at;
}

static int append_hex(char *out, int at, unsigned long v) {
    static const char digits[] = "0123456789abcdef";
    out[at++] = '0';
    out[at++] = 'x';
    int started = 0;
    for (int shift = 60; shift >= 0; shift -= 4) {
        int d = (int)((v >> shift) & 0xf);
        if (d || started || shift == 0) {
            out[at++] = digits[d];
            started = 1;
        }
    }
    return at;
}

static int append_dec(char *out, int at, int v) {
    char tmp[16];
    int n = 0;
    if (v < 0) { out[at++] = '-'; v = -v; }
    do { tmp[n++] = (char)('0' + v % 10); v /= 10; } while (v);
    while (n) out[at++] = tmp[--n];
    return at;
}

void startup_c_main(unsigned long ps_addr) {
    char out[160];
    int at = 0;
    const struct fixture_ps_strings *ps =
        (const struct fixture_ps_strings *)ps_addr;
    at = append_str(out, at, "FIXTURE ps=");
    at = append_hex(out, at, ps_addr);
    at = append_str(out, at, " argc=");
    at = append_dec(out, at, ps->n_argv);
    at = append_str(out, at, " argv0=");
    if (ps->argv_str && ps->n_argv > 0 && ps->argv_str[0])
        at = append_str(out, at, ps->argv_str[0]);
    else
        at = append_str(out, at, "<null>");
    out[at++] = '\n';
    raw_write(1, out, (unsigned long)at);
    raw_exit(0);
}
