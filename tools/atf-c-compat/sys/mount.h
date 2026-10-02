/* NK4-C 目标③：picolibc 无 <sys/mount.h>；atf-c/detail/fs.c 仅 #include 它（无符号依赖）。
 * 空 stub 供构建期编译通过。不参与生产镜像。 */
#ifndef ATF_COMPAT_SYS_MOUNT_H
#define ATF_COMPAT_SYS_MOUNT_H
#endif
