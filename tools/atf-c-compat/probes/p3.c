/* p3：加 malloc/free（_sbrk bump 堆 + atexit/fini_array 面） */
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
int main(int argc, char **argv) {
    (void)argc; (void)argv;
    char *p = malloc(64);
    if (!p) { printf("P3-MALLOC-FAIL\n"); return 1; }
    memset(p, 'x', 64);
    printf("P3-OK %c\n", p[63]);
    free(p);
    return 0;
}
