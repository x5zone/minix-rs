/* p1：最小桥面——只用 _write/_exit（SYS_DIAGCTL/SYS_EXIT→PM），无 stdio/malloc */
extern int _write(int fd, const void *buf, unsigned long len);
extern void _exit(int code);
int main(int argc, char **argv) {
    (void)argc; (void)argv;
    _write(1, "P1-OK\n", 6);
    _exit(0);
    return 0;
}
