/* 目标③：picolibc 无 <dlfcn.h>；部分 minix3 libc 测试 #include 它但不真用 dlopen
 * （仅取 RTLD_* 宏/dlfcn 原型）。构建期最小声明，够编译；上机执行需真 dl* 或裁剪。 */
#ifndef ATF_COMPAT_DLFCN_H
#define ATF_COMPAT_DLFCN_H
#define RTLD_DEFAULT ((void *)0)
#define RTLD_NOW 2
#define RTLD_LAZY 1
void *dlopen(const char *, int);
void *dlsym(void *, const char *);
int dlclose(void *);
char *dlerror(void);
#endif
