/* x86_64 startup-stub spawn fixture.
 *
 * Forks, execs the static child fixture under ptrace, then installs a synthetic
 * ps_strings descriptor in the child's stack and sets RBX to it — the kernel
 * birth contract on x86_64. The child's `_start` (startup-minix.c) must move
 * RBX to RDI and call startup_c_main, which prints argc/argv0 back through a
 * raw write(2). This proves the register handoff without needing a boot image.
 */
#include <errno.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/ptrace.h>
#include <sys/types.h>
#include <sys/user.h>
#include <sys/wait.h>
#include <unistd.h>

static int poke_word(pid_t pid, unsigned long addr, unsigned long value) {
    if (ptrace(PTRACE_POKEDATA, pid, (void *)addr, (void *)value) == -1)
        return -1;
    return 0;
}

static int poke_bytes(pid_t pid, unsigned long addr, const void *buf, size_t len) {
    const unsigned char *p = buf;
    for (size_t off = 0; off < len; off += sizeof(unsigned long)) {
        unsigned long word = 0;
        size_t n = len - off < sizeof(unsigned long) ? len - off : sizeof(unsigned long);
        memcpy(&word, p + off, n);
        if (poke_word(pid, addr + off, word) == -1)
            return -1;
    }
    return 0;
}

int main(int argc, char **argv) {
    if (argc != 2) {
        fprintf(stderr, "usage: %s <static-child-fixture>\n", argv[0]);
        return 2;
    }

    int fds[2];
    if (pipe(fds) != 0) { perror("pipe"); return 2; }

    pid_t pid = fork();
    if (pid < 0) { perror("fork"); return 2; }
    if (pid == 0) {
        close(fds[0]);
        if (dup2(fds[1], STDOUT_FILENO) < 0) _exit(127);
        close(fds[1]);
        if (ptrace(PTRACE_TRACEME, 0, 0, 0) == -1) _exit(127);
        execl(argv[1], argv[1], (char *)NULL);
        _exit(127);
    }

    close(fds[1]);
    int status = 0;
    if (waitpid(pid, &status, 0) < 0) { perror("waitpid"); return 2; }
    if (!WIFSTOPPED(status) || WSTOPSIG(status) != SIGTRAP) {
        fprintf(stderr, "FAIL: child did not stop at exec (status=%#x)\n", status);
        return 1;
    }

    struct user_regs_struct regs;
    if (ptrace(PTRACE_GETREGS, pid, 0, &regs) == -1) { perror("GETREGS"); return 1; }

    const unsigned long base = (unsigned long)((regs.rsp - 0x800) & ~0xfUL);
    const unsigned long argv_arr = base + 0x40;
    const unsigned long env_arr = base + 0x50;
    const unsigned long str_addr = base + 0x100;
    const char *arg0 = "probe-argv";

    unsigned long desc[4] = {
        argv_arr,
        1, /* n_argv; upper 32 bits are padding */
        env_arr,
        0, /* n_env */
    };
    if (poke_bytes(pid, base, desc, sizeof(desc)) == -1 ||
        poke_bytes(pid, argv_arr, (unsigned long[]){str_addr, 0}, 2 * sizeof(unsigned long)) == -1 ||
        poke_word(pid, env_arr, 0) == -1 ||
        poke_bytes(pid, str_addr, arg0, strlen(arg0) + 1) == -1) {
        perror("POKEDATA");
        return 1;
    }

    regs.rbx = base;
    if (ptrace(PTRACE_SETREGS, pid, 0, &regs) == -1) { perror("SETREGS"); return 1; }
    if (ptrace(PTRACE_CONT, pid, 0, 0) == -1) { perror("CONT"); return 1; }

    char out[512];
    size_t used = 0;
    ssize_t n;
    while ((n = read(fds[0], out + used, sizeof(out) - 1 - used)) > 0) {
        used += (size_t)n;
        if (used == sizeof(out) - 1) break;
    }
    out[used] = '\0';
    close(fds[0]);

    if (waitpid(pid, &status, 0) < 0) { perror("waitpid"); return 1; }
    if (!WIFEXITED(status) || WEXITSTATUS(status) != 0) {
        fprintf(stderr, "FAIL: child exit status=%#x\n", status);
        return 1;
    }
    if (!strstr(out, "FIXTURE ps=0x") ||
        !strstr(out, " argc=1 argv0=probe-argv")) {
        fprintf(stderr, "FAIL: unexpected child output: %s", out);
        return 1;
    }

    printf("PASS: x86_64 startup stub passed ps_strings in RBX -> RDI\n");
    printf("      child: %s", out);
    return 0;
}
