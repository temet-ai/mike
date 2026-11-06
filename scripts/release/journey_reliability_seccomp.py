"""Pinned native-only default-deny syscall contract; never installed at import.

This deliberately narrower contract does not inherit a daemon's changing default
profile. Unknown syscall compatibility fails closed. Changes require review and
an explicit PROFILE_SHA256 update, not an automatic runtime fallback.
"""
import hashlib
import json

from journey_process import require

# Native Python, libc, PostgreSQL and Rust process/file/memory/timer primitives.
# No socket constructor, clone, namespace/admin API or indirect syscall broker
# belongs in this unconditional set; constrained rules follow separately.
BASE_SYSCALLS = tuple(sorted('''
accept accept4 access alarm arch_prctl bind brk capget chdir chmod chown
clock_getres clock_gettime clock_nanosleep close close_range connect copy_file_range
creat dup dup2 dup3 epoll_create epoll_create1 epoll_ctl epoll_pwait epoll_pwait2
epoll_wait eventfd eventfd2 execve execveat exit exit_group faccessat faccessat2
fadvise64 fallocate fchdir fchmod fchmodat fchown fchownat fcntl fdatasync
fgetxattr flistxattr flock fork fremovexattr fsetxattr fstat fstatfs fsync
ftruncate futex futimesat getcpu getcwd getdents getdents64 getegid geteuid getgid
getgroups getitimer getpeername getpgid getpgrp getpid getppid getpriority getrandom
getresgid getresuid getrlimit get_robust_list getrusage getsid getsockname getsockopt
gettid gettimeofday getuid getxattr inotify_add_watch inotify_init inotify_init1
inotify_rm_watch ioctl kill lchown lgetxattr link linkat listen listxattr
llistxattr lremovexattr lseek lsetxattr lstat madvise membarrier memfd_create
mincore mkdir mkdirat mlock mlock2 mlockall mmap mprotect mq_getsetattr mq_notify
mq_open mq_timedreceive mq_timedsend mq_unlink mremap msgctl msgget msgrcv msgsnd
msync munlock munlockall munmap nanosleep newfstatat open openat openat2 pause
pipe pipe2 pkey_alloc pkey_free pkey_mprotect poll ppoll prctl pread64 preadv
preadv2 prlimit64 pselect6 pwrite64 pwritev pwritev2 read readahead readlink
readlinkat readv recvfrom recvmmsg recvmsg removexattr rename renameat renameat2
restart_syscall rmdir rseq rt_sigaction rt_sigpending rt_sigprocmask rt_sigqueueinfo
rt_sigreturn rt_sigsuspend rt_sigtimedwait rt_tgsigqueueinfo sched_getaffinity
sched_getattr sched_getparam sched_get_priority_max sched_get_priority_min
sched_getscheduler sched_rr_get_interval sched_setaffinity sched_setparam
sched_yield select semctl semget semop semtimedop sendfile sendmmsg sendmsg sendto
setitimer setpgid setpriority setrlimit set_robust_list setsid setsockopt
set_tid_address setxattr shmat shmctl shmdt shmget shutdown sigaltstack signalfd
signalfd4 splice stat statfs statx symlink symlinkat sync sync_file_range syncfs
sysinfo tee tgkill time timer_create timer_delete timer_getoverrun timer_gettime
timer_settime timerfd_create timerfd_gettime timerfd_settime times tkill truncate
umask uname unlink unlinkat utime utimensat utimes vfork vmsplice wait4 waitid write writev
'''.split()))
# CLONE_NEWNS, NEWCGROUP, NEWUTS, NEWIPC, NEWUSER, NEWPID, NEWNET, NEWTIME.
NAMESPACE_MASK = 0x7E020080
PROFILE_SHA256 = '115da428eec824581c2af6a9623273226b3adec106dd9d949bb1aab9a2a643ce'


def profile():
    rules = [{'names': list(BASE_SYSCALLS), 'action': 'SCMP_ACT_ALLOW'}]
    for family in (1, 2, 10):  # AF_UNIX, AF_INET, AF_INET6 only.
        rules.append({'names': ['socket', 'socketpair'], 'action': 'SCMP_ACT_ALLOW',
                      'args': [{'index': 0, 'value': family, 'op': 'SCMP_CMP_EQ'}]})
    rules.extend((
        {'names': ['clone'], 'action': 'SCMP_ACT_ALLOW',
         'args': [{'index': 0, 'value': NAMESPACE_MASK, 'valueTwo': 0, 'op': 'SCMP_CMP_MASKED_EQ'}]},
        # libc may probe clone3 before using the inspectable clone flags API.
        {'names': ['clone3'], 'action': 'SCMP_ACT_ERRNO', 'errnoRet': 38},
    ))
    return {'defaultAction': 'SCMP_ACT_ERRNO', 'defaultErrnoRet': 1,
            'architectures': ['SCMP_ARCH_X86_64'], 'syscalls': rules}


def profile_bytes():
    return json.dumps(profile(), sort_keys=True, separators=(',', ':')).encode('ascii')


def pinned_profile_bytes():
    value = profile_bytes()
    require(hashlib.sha256(value).hexdigest() == PROFILE_SHA256, 'ISOLATED_SECCOMP_SOURCE_PIN')
    return value


def write_profile(path):
    value = pinned_profile_bytes()
    with path.open('xb') as output:
        output.write(value)
    path.chmod(0o600)


def security_options_contract(options):
    require(isinstance(options, list) and len(options) == 2 and all(isinstance(item, str) for item in options),
            'ISOLATED_SECCOMP_OPTIONS')
    privilege = [item for item in options if item in ('no-new-privileges', 'no-new-privileges=true')]
    policies = [item[len('seccomp='):] for item in options if item.startswith('seccomp=')]
    require(len(privilege) == len(policies) == 1, 'ISOLATED_SECCOMP_OPTIONS')
    expected = pinned_profile_bytes()
    try:
        actual = json.dumps(json.loads(policies[0]), sort_keys=True, separators=(',', ':')).encode('ascii')
    except (ValueError, UnicodeError) as error:
        from journey_process import JourneyError
        raise JourneyError('ISOLATED_SECCOMP_PROFILE') from error
    require(actual == expected, 'ISOLATED_SECCOMP_PROFILE')
    return PROFILE_SHA256
