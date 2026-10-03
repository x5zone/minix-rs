/* p2：加 tinystdio printf（FILE/iob/putc→_write 腿 + exit flush） */
#include <stdio.h>
int main(int argc, char **argv) {
    (void)argc; (void)argv;
    printf("P2-OK %d\n", 42);
    fflush(stdout);
    return 0;
}
