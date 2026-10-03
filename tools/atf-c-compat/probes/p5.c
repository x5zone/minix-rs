#include <stdlib.h>
extern int _write(int fd, const void *buf, unsigned long len);
extern void _exit(int code);
static void hx(unsigned long long v){ char b[2]; for(int s=60;s>=0;s-=4){unsigned d=(v>>s)&0xf;b[0]=(char)(d<10?'0'+d:'a'+d-10);_write(1,b,1);} }
int main(void){
    srandom(0);
    for (int i = 0; i < 3; i++) { long r = random(); _write(1,"R=",2); hx((unsigned long long)r); _write(1," ",2); }
    _write(1,"\n",1); _exit(0); return 0;
}
