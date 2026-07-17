#![allow(unused_macros)]

macro_rules! copy_bytes_to_slice {
    ($bytes:expr, $dst:expr) => {
        if $bytes.len() > $dst.len() {
            true // too long return true
        } else {
            let _: &[u8] = $bytes;
            let src: *const u8 = $bytes.as_ptr().cast();
            let dst: *mut u8 = $dst.as_mut_ptr().cast();
            let len: usize = $bytes.len();
            unsafe {
                ::std::ptr::copy_nonoverlapping(src, dst, len);
            }
            false // success return false
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
            #[cfg(not(feature = "use-syscalls"))]
            match ::libc::$name($( $args ),*) {
                -1 => Err(::std::io::Error::last_os_error()),
                res => Ok(res)
            }

            #[cfg(feature = "use-syscalls")]
            match __use_syscalls!($name($( $args ),*)) {
                Err(e) => Err(::std::io::Error::from_raw_os_error(e.into_raw())),
                Ok(res) => Ok(res as ::libc::c_int),
            }
        }
    };

    (unsafe { $name:ident( $( $args:expr ),*  $(,)?) }) => {
        unsafe { syscall!($name($( $args ),*)) }
    };
}

// syscall signature definitions to match libc
#[cfg(feature = "use-syscalls")]
macro_rules! __use_syscalls {
    (fstat ($fd:expr, $statbuf:expr)) => {
        ::syscalls::syscall!(
            ::syscalls::Sysno::fstat,
            $fd as ::libc::c_int,
            $statbuf as *mut ::libc::stat
        )
    };

    (fstatfs ($fd:expr, $buf:expr)) => {
        ::syscalls::syscall!(
            ::syscalls::Sysno::fstatfs,
            $fd as ::libc::c_int,
            $buf as *mut ::libc::statfs
        )
    };

    (ioctl ($fd:expr, $op:expr, $argp:expr)) => {
        ::syscalls::syscall!(
            ::syscalls::Sysno::ioctl,
            $fd as ::libc::c_int,
            $op as ::libc::Ioctl,
            $argp as *mut _
        )
    };

    (open ($path:expr, $flags:expr)) => {
        ::syscalls::syscall!(
            ::syscalls::Sysno::open,
            $path as *const ::libc::c_char,
            $flags as ::libc::c_int
        )
    };
    (open ($path:expr, $flags:expr, $mode:expr)) => {
        ::syscalls::syscall!(
            ::syscalls::Sysno::open,
            $path as *const ::libc::c_char,
            $flags as ::libc::c_int,
            $mode as ::libc::mode_t
        )
    };

    (openat ($dirfd:expr, $path:expr, $flags:expr)) => {
        ::syscalls::syscall!(
            ::syscalls::Sysno::openat,
            $dirfd as ::libc::c_int,
            $path as *const ::libc::c_char,
            $flags as ::libc::c_int
        )
    };
    (openat ($dirfd:expr, $path:expr, $flags:expr, $mode:expr)) => {
        ::syscalls::syscall!(
            ::syscalls::Sysno::openat,
            $dirfd as ::libc::c_int,
            $path as *const ::libc::c_char,
            $flags as ::libc::c_int,
            $mode as ::libc::mode_t
        )
    };

    (statfs ($path:expr, $buf:expr)) => {
        ::syscalls::syscall!(
            ::syscalls::Sysno::statfs,
            $path as *const ::libc::c_char,
            $buf as *mut ::libc::statfs
        )
    };
}
