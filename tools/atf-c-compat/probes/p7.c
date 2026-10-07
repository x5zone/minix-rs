/* p7：heap 布局探针——打印 &heap 区（经 _sbrk 观测）与深写，判
 * nano-malloc 的 sbrk 是否落 bump 堆、bump 容量门是否生效（D3a 定性）。 */
#include <stdlib.h>
#include <string.h>
#include <stdio.h>
extern int _write(int fd, const void *buf, unsigned long len);
extern void _exit(int code);
static void hx(unsigned long long v){ char b[2]; for(int s=60;s>=0;s-=4){unsigned d=(v>>s)&0xf;b[0]=(char)(d<10?'0'+d:'a'+d-10);_write(1,b,1);} }
int main(void){
    /* 三连 300KB malloc（总和 900KB < 1MB bump；再一发 300KB 触发容量门） */
    void *a = malloc(300000); void *b = malloc(300000); void *c = malloc(300000);
    printf("P7 a=%lx b=%lx c=%lx\n", (unsigned long)a, (unsigned long)b, (unsigned long)c);
    if (!a || !b || !c) { printf("P7-NULL\n"); _exit(0); }
    memset(a, 'a', 300000); memset(b, 'b', 300000); memset(c, 'c', 300000);
    void *d = malloc(300000);
    printf("P7 d=%lx\n", (unsigned long)d);
    if (d) memset(d, 'd', 300000);   /* 越 bump 容量后的写：若返回非 NULL 且越 heap 尾=门失效 */
    printf("P7-OK\n");
    fflush(stdout);
    _exit(0);
    return 0;
}
