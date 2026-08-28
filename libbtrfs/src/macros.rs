#![allow(unused_macros)]

macro_rules! copy_bytes_to_c_array {
    ($bytes:expr, $dst:expr) => {
        if $bytes.len() < $dst.len() {
            let _: &[u8] = $bytes;
            let src: *const u8 = $bytes.as_ptr().cast();
            let dst: *mut u8 = $dst.as_mut_ptr().cast();
            let len: usize = $bytes.len();
            unsafe {
                ::std::ptr::copy_nonoverlapping(src, dst, len);
            }
            true // success return true
        } else {
            false // too long return false
        }
    };
}

macro_rules! error {
    ( $kind:ident ) => {
        Err(::std::io::Error::from(::std::io::ErrorKind::$kind))
    };
}

// for implmeting iterator next trait
macro_rules! try_or_some_err {
    ($expr:expr $(,)?) => {
        match $expr {
            ::std::result::Result::Ok(val) => val,
            ::std::result::Result::Err(err) => {
                return Some(::std::result::Result::Err(::std::convert::From::from(err)));
            }
        }
    };
}

macro_rules! syscall {
    ($name:ident($( $args:expr ),* $(,)?)) => {
        {
            #[cfg(feature = "use-syscalls")]
            match syscall!(@$name( $($args),* )) {
                Err(e) => Err(::std::io::Error::from_raw_os_error(e.into_raw())),
                Ok(res) => Ok(res as ::libc::c_int),
            }
            #[cfg(not(feature = "use-syscalls"))]
            match ::libc::$name( $($args),* ) {
                -1 => Err(::std::io::Error::last_os_error()),
                res => Ok(res)
            }
        }
    };

    (unsafe { $name:ident( $( $args:expr ),*  $(,)?) }) => {
        unsafe { syscall!($name($( $args ),*)) }
    };

    // =======================================================================
    // Define system calls to match libc signatures
    // =======================================================================

    (@fallocate ( $fd:expr, $mode:expr, $offset:expr, $len:expr )) => {
        ::syscalls::syscall! {
            ::syscalls::Sysno::fallocate,
            $fd as ::libc::c_int,
            $mode as ::libc::c_int,
            $offset as ::libc::off_t,
            $len as ::libc::off_t
        }
    };

    (@fchmodat ( $dirfd:expr, $pathname:expr, $mode:expr, $flags:expr )) => {
        ::syscalls::syscall! {
            ::syscalls::Sysno::fchmodat,
            $dirfd as ::libc::c_int,
            $pathname as *const ::libc::c_char,
            $mode as ::libc::mode_t,
            $flags as ::libc::c_int
        }
    };

    (@fchownat ( $dirfd:expr, $pathname:expr, $owner:expr, $group:expr, $flags:expr )) => {
        ::syscalls::syscall! {
            ::syscalls::Sysno::fchownat,
            $dirfd as ::libc::c_int,
            $pathname as *const ::libc::c_char,
            $owner as ::libc::uid_t,
            $group as ::libc::gid_t,
            $flags as ::libc::c_int
        }
    };

    (@fcntl ( $fd:expr, $cmd:expr )) => {
        ::syscalls::syscall! {
            ::syscalls::Sysno::fcntl,
            $fd as ::libc::c_int,
            $cmd as ::libc::c_int,
        }
    };

    (@fcntl ( $fd:expr, $cmd:expr, $arg:expr)) => {
        ::syscalls::syscall! {
            ::syscalls::Sysno::fcntl,
            $fd as ::libc::c_int,
            $cmd as ::libc::c_int,
            $arg as ::libc::c_int
        }
    };

    (@fstat ( $fd:expr, $statbuf:expr )) => {
        ::syscalls::syscall! {
            ::syscalls::Sysno::fstat,
            $fd as ::libc::c_int,
            $statbuf as *mut ::libc::stat
        }
    };

    (@ftruncate ( $fd:expr, $length:expr )) => {
        ::syscalls::syscall! {
            ::syscalls::Sysno::ftruncate,
            $fd as ::libc::c_int,
            $length as ::libc::off_t
        }
    };

    (@fstatfs ( $fd:expr, $buf:expr )) => {
        ::syscalls::syscall! {
            ::syscalls::Sysno::fstatfs,
            $fd as ::libc::c_int,
            $buf as *mut ::libc::statfs
        }
    };

    (@ioctl ( $fd:expr, $op:expr, $argp:expr )) => {
        ::syscalls::syscall! {
            ::syscalls::Sysno::ioctl,
            $fd as ::libc::c_int,
            $op as ::libc::Ioctl,
            $argp as *mut _
        }
    };

    (@linkat ( $olddirfd:expr, $oldpath:expr, $newdirfd:expr, $newpath:expr, $flags:expr )) => {
        ::syscalls::syscall! {
            ::syscalls::Sysno::linkat,
            $olddirfd as ::libc::c_int,
            $oldpath as *const ::libc::c_char,
            $newdirfd as ::libc::c_int,
            $newpath as *const ::libc::c_char,
            $flags as ::libc::c_int
        }
    };

    (@lremovexattr ( $path:expr, $name:expr )) => {
        ::syscalls::syscall! {
            ::syscalls::Sysno::lremovexattr,
            $path as *const ::libc::c_char,
            $name as *const ::libc::c_char
        }
    };

    (@lsetxattr ( $path:expr, $name:expr, $value:expr, $size:expr, $flags:expr )) => {
        ::syscalls::syscall! {
            ::syscalls::Sysno::lsetxattr,
            $path as *const ::libc::c_char,
            $name as *const ::libc::c_char,
            $value as *const ::libc::c_void,
            $size as ::libc::size_t,
            $flags as  ::libc::c_int
        }
    };

    (@mknodat ( $dirfd:expr, $pathname:expr, $mode:expr, $dev:expr )) => {
        ::syscalls::syscall! {
            ::syscalls::Sysno::mknodat,
            $dirfd as ::libc::c_int,
            $pathname as *const ::libc::c_char,
            $mode as ::libc::mode_t,
            $dev as ::libc::dev_t
        }
    };

    (@mkdirat ( $dirfd:expr, $pathname:expr, $mode:expr )) => {
        ::syscalls::syscall! {
            ::syscalls::Sysno::mkdirat,
            $dirfd as ::libc::c_int,
            $pathname as *const ::libc::c_char,
            $mode as ::libc::mode_t
        }
    };

    (@pwrite ( $fd:expr, $buf:expr, $count:expr, $offset:expr )) => {
        ::syscalls::syscall! {
            ::syscalls::Sysno::pwrite64,
            $fd as ::libc::c_int,
            $buf as *const ::libc::c_void,
            $count as ::libc::size_t,
            $offset as ::libc::off_t
        }
    };

    (@open ($path:expr, $flags:expr)) => {
        ::syscalls::syscall! {
            ::syscalls::Sysno::open,
            $path as *const ::libc::c_char,
            $flags as ::libc::c_int
        }
    };

    (@open ($path:expr, $flags:expr, $mode:expr)) => {
        ::syscalls::syscall {
            ::syscalls::Sysno::open,
            $path as *const ::libc::c_char,
            $flags as ::libc::c_int,
            $mode as ::libc::mode_t
        }
    };

    (@openat ($dirfd:expr, $path:expr, $flags:expr)) => {
        ::syscalls::syscall! {
            ::syscalls::Sysno::openat,
            $dirfd as ::libc::c_int,
            $path as *const ::libc::c_char,
            $flags as ::libc::c_int
        }
    };

    (@openat ($dirfd:expr, $path:expr, $flags:expr, $mode:expr)) => {
        ::syscalls::syscall! {
            ::syscalls::Sysno::openat,
            $dirfd as ::libc::c_int,
            $path as *const ::libc::c_char,
            $flags as ::libc::c_int,
            $mode as ::libc::mode_t
        }
    };

    (@renameat2 ($olddirfd:expr, $oldpath:expr, $newdirfd:expr, $newpath:expr, $flags:expr)) => {
        ::syscalls::syscall! {
            ::syscalls::Sysno::renameat2,
            $olddirfd as ::libc::c_int,
            $oldpath as *const ::libc::c_char,
            $newdirfd as ::libc::c_int,
            $newpath as *const ::libc::c_char,
            $flags as ::libc::c_uint
        }
    };

    (@statfs ( $path:expr, $buf:expr )) => {
        ::syscalls::syscall! {
            ::syscalls::Sysno::statfs,
            $path as *const ::libc::c_char,
            $buf as *mut ::libc::statfs
        }
    };

    (@symlinkat ( $target:expr, $newdirfd:expr, $linkpath:expr )) => {
        ::syscalls::syscall! {
            ::syscalls::Sysno::symlinkat,
            $target as *const ::libc::c_char,
            $newdirfd as ::libc::c_int,
            $linkpath as *const ::libc::c_char
        }
    };

    (@unlinkat ( $dirfd:expr, $pathname:expr, $flags:expr )) => {
        ::syscalls::syscall! {
            ::syscalls::Sysno::unlinkat,
            $dirfd as ::libc::c_int,
            $pathname as *const ::libc::c_char,
            $flags as ::libc::c_int
        }
    };

    (@utimensat ( $dirfd:expr, $path:expr, $times:expr, $flags:expr )) => {
        ::syscalls::syscall! {
            ::syscalls::Sysno::utimensat,
            $dirfd as ::libc::c_int,
            $path as *const ::libc::c_char,
            $times as *const ::libc::timespec,
            $flags as ::libc::c_int
        }
    };
}
