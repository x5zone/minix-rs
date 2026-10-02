/* NK4-C 目标③：picolibc 的 <sys/dirent.h> 是 `#error "<dirent.h> not supported"` 占位
 * （顶层 <dirent.h> 正常但会 include <sys/dirent.h> 而撞上该 #error）。
 * 此头用唯一 guard ATF_COMPAT_SYS_DIRENT_H 经 -I 置前覆盖 picolibc 的 sys/dirent.h，
 * 让 atf-c/detail/fs.c（及 <dirent.h> include 链）构建期不再 #error；DIR/opendir 等声明
 * 为顺带提供（fs.c 本身不引用）。运行期真实现由 ③ syscall 桥（opendir/readdir ←
 * VFS getdents）提供。仅构建期用，不参与 minix-rs 生产镜像。 */
#ifndef ATF_COMPAT_SYS_DIRENT_H
#define ATF_COMPAT_SYS_DIRENT_H
#include <sys/types.h>
#define NAME_MAX 255
struct dirent { ino_t d_ino; char d_name[NAME_MAX + 1]; };
typedef struct { int fd; } DIR;
DIR *opendir(const char *);
struct dirent *readdir(DIR *);
int closedir(DIR *);
#endif
