//! Unix domain socket control message handling without nix crate.
//!
//! Provides SCM_RIGHTS (file descriptor passing) support using raw libc calls.

use std::io;
use std::os::unix::io::RawFd;

/// Receive a message with optional file descriptors via SCM_RIGHTS.
///
/// Returns (bytes_read, received_fds). If bytes_read is 0, the peer disconnected.
///
/// Descriptors arrive with close-on-exec already set: the compositor spawns child
/// processes, and a client's descriptor must not be inheritable by them.
/// Truncated control data is an error rather than a silent partial read, because
/// the descriptors that did not fit are lost with no way to tell which.
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
    let received = unsafe { libc::recvmsg(fd, &mut msg, libc::MSG_CMSG_CLOEXEC) };

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

    // Control data did not fit. The kernel closed the descriptors it could not
    // deliver, so the surviving ones belong to a message whose descriptor set is
    // now incomplete — close them too rather than act on a partial set.
    if msg.msg_flags & libc::MSG_CTRUNC != 0 {
        for fd in fds {
            close_fd(fd);
        }
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "SCP control message was truncated",
        ));
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

/// Send a message with optional file descriptors via SCM_RIGHTS.
///
/// Returns the number of payload bytes accepted by the kernel. Descriptors are
/// only attached to the first `sendmsg`, so callers must send a whole frame in a
/// single call when they pass any.
pub fn sendmsg_with_fds(fd: RawFd, payload: &[u8], fds: &[RawFd]) -> io::Result<usize> {
    if fds.len() > MAX_FDS_PER_MESSAGE {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "too many descriptors for one SCP frame",
        ));
    }

    let mut iov = libc::iovec {
        iov_base: payload.as_ptr() as *mut libc::c_void,
        iov_len: payload.len(),
    };

    let mut msg: libc::msghdr = unsafe { std::mem::zeroed() };
    msg.msg_iov = &mut iov;
    msg.msg_iovlen = 1;

    // The control buffer must outlive the `sendmsg` call below.
    let cmsg_space =
        unsafe { libc::CMSG_SPACE(u32::try_from(size_of_val(fds)).unwrap_or(0)) } as usize;
    let mut cmsg_buffer = vec![0_u8; cmsg_space];

    if !fds.is_empty() {
        msg.msg_control = cmsg_buffer.as_mut_ptr() as *mut libc::c_void;
        msg.msg_controllen = cmsg_space;

        // SAFETY: msg_control points at a correctly sized CMSG_SPACE buffer.
        let cmsg = unsafe { libc::CMSG_FIRSTHDR(&msg) };
        if cmsg.is_null() {
            return Err(io::Error::other("failed to build SCM_RIGHTS header"));
        }
        // SAFETY: cmsg points into cmsg_buffer, which is large enough for `fds`.
        unsafe {
            (*cmsg).cmsg_level = libc::SOL_SOCKET;
            (*cmsg).cmsg_type = libc::SCM_RIGHTS;
            (*cmsg).cmsg_len = libc::CMSG_LEN(u32::try_from(size_of_val(fds)).unwrap_or(0)) as _;
            std::ptr::copy_nonoverlapping(
                fds.as_ptr(),
                libc::CMSG_DATA(cmsg) as *mut RawFd,
                fds.len(),
            );
        }
    }

    // SAFETY: sendmsg is a standard POSIX call and msg is fully initialized.
    let sent = unsafe { libc::sendmsg(fd, &msg, libc::MSG_NOSIGNAL) };
    if sent < 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(sent as usize)
}

/// Maximum descriptors the transport will attach to or accept from one frame.
pub const MAX_FDS_PER_MESSAGE: usize = 4;

/// Close a file descriptor, ignoring errors.
pub fn close_fd(fd: RawFd) {
    unsafe {
        libc::close(fd);
    }
}

/// Size of the file behind a descriptor, in bytes.
///
/// A client declares how large its shared-memory pool is, but the descriptor is
/// what the renderer will actually map. This is how the declaration gets checked
/// against the truth instead of being taken on the client's word.
pub fn fd_size(fd: RawFd) -> io::Result<u64> {
    let mut stat: libc::stat = unsafe { std::mem::zeroed() };

    // SAFETY: fstat fills a caller-provided stat buffer for an open descriptor.
    let result = unsafe { libc::fstat(fd, &mut stat) };
    if result < 0 {
        return Err(io::Error::last_os_error());
    }

    u64::try_from(stat.st_size)
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidData, "descriptor has a negative size"))
}

/// Create a close-on-exec pipe (for data transfer).
///
/// Returns (read_fd, write_fd).
pub fn create_pipe() -> io::Result<(RawFd, RawFd)> {
    let mut fds = [0i32; 2];

    // SAFETY: pipe2 is a standard Linux syscall and fds has room for two.
    let result = unsafe { libc::pipe2(fds.as_mut_ptr(), libc::O_CLOEXEC) };

    if result < 0 {
        return Err(io::Error::last_os_error());
    }

    Ok((fds[0], fds[1]))
}

/// Create an eventfd used to wake a blocked client thread.
pub fn create_eventfd() -> io::Result<RawFd> {
    // SAFETY: eventfd is a standard Linux syscall.
    let fd = unsafe { libc::eventfd(0, libc::EFD_CLOEXEC | libc::EFD_NONBLOCK) };
    if fd < 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(fd)
}

/// Signal an eventfd. Saturation is harmless: the reader only needs to learn
/// that at least one event is pending.
pub fn signal_eventfd(fd: RawFd) {
    let value: u64 = 1;
    // SAFETY: writing 8 bytes to an eventfd is the documented interface.
    unsafe {
        libc::write(fd, std::ptr::addr_of!(value).cast(), size_of::<u64>());
    }
}

/// Drain an eventfd counter so the next wait blocks again.
pub fn drain_eventfd(fd: RawFd) {
    let mut value: u64 = 0;
    // SAFETY: reading 8 bytes from a non-blocking eventfd either succeeds or
    // fails with EAGAIN, both of which are fine here.
    unsafe {
        libc::read(fd, std::ptr::addr_of_mut!(value).cast(), size_of::<u64>());
    }
}

/// Readiness reported by [`poll_readable`].
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Readiness {
    /// The primary descriptor has data or a status change pending.
    pub primary: bool,
    /// The wake descriptor was signaled.
    pub wake: bool,
    /// The primary descriptor's peer hung up or errored.
    pub hangup: bool,
}

/// Wait until either descriptor is readable, or the timeout elapses.
///
/// Retries on `EINTR` so signal delivery does not look like a hangup.
pub fn poll_readable(
    primary: RawFd,
    wake: RawFd,
    timeout: std::time::Duration,
) -> io::Result<Readiness> {
    let timeout_ms = i32::try_from(timeout.as_millis()).unwrap_or(i32::MAX);

    loop {
        let mut fds = [
            libc::pollfd {
                fd: primary,
                events: libc::POLLIN,
                revents: 0,
            },
            libc::pollfd {
                fd: wake,
                events: libc::POLLIN,
                revents: 0,
            },
        ];

        // SAFETY: poll is a standard POSIX call over a two-entry array.
        let result = unsafe { libc::poll(fds.as_mut_ptr(), 2, timeout_ms) };
        if result < 0 {
            let error = io::Error::last_os_error();
            if error.kind() == io::ErrorKind::Interrupted {
                continue;
            }
            return Err(error);
        }

        let hangup_mask = libc::POLLHUP | libc::POLLERR | libc::POLLNVAL;
        return Ok(Readiness {
            primary: fds[0].revents & libc::POLLIN != 0,
            wake: fds[1].revents & libc::POLLIN != 0,
            hangup: fds[0].revents & hangup_mask != 0,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::io::AsRawFd;
    use std::os::unix::net::UnixStream;

    #[test]
    fn gets_peer_credentials() {
        let (_client, server) = UnixStream::pair().expect("create socket pair");

        let (pid, uid, gid) =
            get_peer_credentials(server.as_raw_fd()).expect("get peer credentials");

        // Should match our process
        assert_eq!(pid, std::process::id());
        assert_eq!(uid, unsafe { libc::getuid() });
        assert_eq!(gid, unsafe { libc::getgid() });
    }

    #[test]
    fn round_trips_a_descriptor_through_scm_rights() {
        use std::io::{Read, Write};
        use std::os::unix::io::FromRawFd;

        let (sender, receiver) = UnixStream::pair().expect("create socket pair");
        let (read_fd, write_fd) = create_pipe().expect("create pipe");

        let payload = b"frame";
        let sent = sendmsg_with_fds(sender.as_raw_fd(), payload, &[read_fd])
            .expect("send descriptor with payload");
        assert_eq!(sent, payload.len());
        close_fd(read_fd);

        let mut buffer = [0_u8; 32];
        let (received, fds) =
            recvmsg_with_fds(receiver.as_raw_fd(), &mut buffer, 4).expect("receive descriptor");
        assert_eq!(&buffer[..received], payload);
        assert_eq!(fds.len(), 1, "exactly one descriptor should arrive");

        // The received descriptor must be the live read end of the same pipe.
        let mut producer = unsafe { std::fs::File::from_raw_fd(write_fd) };
        producer.write_all(b"payload").expect("write to pipe");
        drop(producer);

        let mut consumer = unsafe { std::fs::File::from_raw_fd(fds[0]) };
        let mut content = String::new();
        consumer.read_to_string(&mut content).expect("read pipe");
        assert_eq!(content, "payload");
    }

    #[test]
    fn refuses_more_descriptors_than_one_frame_may_carry() {
        let (sender, _receiver) = UnixStream::pair().expect("create socket pair");
        let fds = vec![sender.as_raw_fd(); MAX_FDS_PER_MESSAGE + 1];

        let error = sendmsg_with_fds(sender.as_raw_fd(), b"x", &fds)
            .expect_err("too many descriptors must be rejected");
        assert_eq!(error.kind(), io::ErrorKind::InvalidInput);
    }

    #[test]
    fn eventfd_signals_then_clears() {
        let fd = create_eventfd().expect("create eventfd");
        let idle = std::time::Duration::from_millis(0);

        // Nothing signaled yet.
        let readiness = poll_readable(fd, fd, idle).expect("poll idle eventfd");
        assert!(
            !readiness.wake,
            "an unsignaled eventfd must not be readable"
        );

        signal_eventfd(fd);
        let readiness = poll_readable(fd, fd, idle).expect("poll signaled eventfd");
        assert!(readiness.wake, "a signaled eventfd must be readable");

        drain_eventfd(fd);
        let readiness = poll_readable(fd, fd, idle).expect("poll drained eventfd");
        assert!(
            !readiness.wake,
            "draining must make the next wait block again"
        );

        close_fd(fd);
    }

    #[test]
    fn poll_reports_the_socket_and_wake_source_independently() {
        use std::io::Write;

        let (mut sender, receiver) = UnixStream::pair().expect("create socket pair");
        let wake = create_eventfd().expect("create eventfd");
        let idle = std::time::Duration::from_millis(0);

        let readiness = poll_readable(receiver.as_raw_fd(), wake, idle).expect("poll idle");
        assert!(!readiness.primary && !readiness.wake);

        sender.write_all(b"request").expect("write to socket");
        let readiness = poll_readable(receiver.as_raw_fd(), wake, idle).expect("poll socket");
        assert!(readiness.primary, "socket data must be reported");
        assert!(!readiness.wake, "the wake source is independent");

        signal_eventfd(wake);
        let readiness = poll_readable(receiver.as_raw_fd(), wake, idle).expect("poll both");
        assert!(readiness.primary && readiness.wake, "both can be ready");

        close_fd(wake);
    }

    #[test]
    fn poll_reports_a_hangup_when_the_peer_closes() {
        let (sender, receiver) = UnixStream::pair().expect("create socket pair");
        let wake = create_eventfd().expect("create eventfd");
        drop(sender);

        let readiness = poll_readable(
            receiver.as_raw_fd(),
            wake,
            std::time::Duration::from_millis(50),
        )
        .expect("poll closed peer");
        assert!(
            readiness.hangup || readiness.primary,
            "a closed peer must be observable"
        );

        close_fd(wake);
    }
}
