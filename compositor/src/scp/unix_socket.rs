//! Unix domain socket control message handling without nix crate.
//!
//! Provides SCM_RIGHTS (file descriptor passing) support using raw libc calls.

use std::io;
use std::os::unix::io::RawFd;

/// Receive a message with optional file descriptors via SCM_RIGHTS.
///
/// Returns (bytes_read, received_fds). If bytes_read is 0, the peer disconnected.
pub fn recvmsg_with_fds(
    fd: RawFd,
    buffer: &mut [u8],
    max_fds: usize,
) -> io::Result<(usize, Vec<RawFd>)> {
    let mut iov = libc::iovec {
        iov_base: buffer.as_mut_ptr() as *mut libc::c_void,
        iov_len: buffer.len(),
    };

    // Allocate space for control message (file descriptors)
    let cmsg_space = unsafe { libc::CMSG_SPACE((max_fds * std::mem::size_of::<RawFd>()) as u32) };
    let mut cmsg_buffer = vec![0u8; cmsg_space as usize];

    let mut msg: libc::msghdr = unsafe { std::mem::zeroed() };
    msg.msg_iov = &mut iov;
    msg.msg_iovlen = 1;
    msg.msg_control = cmsg_buffer.as_mut_ptr() as *mut libc::c_void;
    msg.msg_controllen = cmsg_buffer.len();

    // SAFETY: recvmsg is a standard POSIX call, msg structure is properly initialized
    let received = unsafe { libc::recvmsg(fd, &mut msg, 0) };

    if received < 0 {
        return Err(io::Error::last_os_error());
    }

    let mut fds = Vec::new();

    // Extract file descriptors from control message
    let mut cmsg = unsafe { libc::CMSG_FIRSTHDR(&msg) };
    while !cmsg.is_null() {
        let cmsg_ref = unsafe { &*cmsg };

        if cmsg_ref.cmsg_level == libc::SOL_SOCKET && cmsg_ref.cmsg_type == libc::SCM_RIGHTS {
            // SAFETY: CMSG_DATA points to the file descriptor array
            let data_ptr = unsafe { libc::CMSG_DATA(cmsg) } as *const RawFd;
            let data_len = cmsg_ref.cmsg_len as usize - unsafe { libc::CMSG_LEN(0) } as usize;
            let fd_count = data_len / std::mem::size_of::<RawFd>();

            for i in 0..fd_count {
                let fd = unsafe { *data_ptr.add(i) };
                fds.push(fd);
            }
        }

        cmsg = unsafe { libc::CMSG_NXTHDR(&msg, cmsg) };
    }

    Ok((received as usize, fds))
}

/// Get peer credentials (PID, UID, GID) from a Unix socket.
pub fn get_peer_credentials(fd: RawFd) -> io::Result<(u32, u32, u32)> {
    let mut cred: libc::ucred = unsafe { std::mem::zeroed() };
    let mut len = std::mem::size_of::<libc::ucred>() as libc::socklen_t;

    // SAFETY: getsockopt with SO_PEERCRED is standard on Linux
    let result = unsafe {
        libc::getsockopt(
            fd,
            libc::SOL_SOCKET,
            libc::SO_PEERCRED,
            &mut cred as *mut _ as *mut libc::c_void,
            &mut len,
        )
    };

    if result < 0 {
        return Err(io::Error::last_os_error());
    }

    Ok((cred.pid as u32, cred.uid, cred.gid))
}

/// Close a file descriptor, ignoring errors.
pub fn close_fd(fd: RawFd) {
    unsafe {
        libc::close(fd);
    }
}

/// Create a pipe (for data transfer).
///
/// Returns (read_fd, write_fd).
pub fn create_pipe() -> io::Result<(RawFd, RawFd)> {
    let mut fds = [0i32; 2];

    // SAFETY: pipe is a standard POSIX syscall
    let result = unsafe { libc::pipe(fds.as_mut_ptr()) };

    if result < 0 {
        return Err(io::Error::last_os_error());
    }

    Ok((fds[0], fds[1]))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::io::AsRawFd;
    use std::os::unix::net::UnixStream;

    #[test]
    fn gets_peer_credentials() {
        let (client, server) = UnixStream::pair().expect("create socket pair");

        let (pid, uid, gid) =
            get_peer_credentials(server.as_raw_fd()).expect("get peer credentials");

        // Should match our process
        assert_eq!(pid, std::process::id());
        assert_eq!(uid, unsafe { libc::getuid() });
        assert_eq!(gid, unsafe { libc::getgid() });
    }
}
