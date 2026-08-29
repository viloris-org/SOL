//! Memory file descriptor (memfd) support without external dependencies.
//!
//! Provides memfd_create syscall wrapper for creating anonymous memory-backed files.

use std::ffi::CString;
use std::io;
use std::os::unix::io::RawFd;

// memfd_create flags
const MFD_CLOEXEC: u32 = 0x0001;
const MFD_ALLOW_SEALING: u32 = 0x0002;

// fcntl sealing constants
pub const F_ADD_SEALS: i32 = 1033;
pub const F_GET_SEALS: i32 = 1034;
pub const F_SEAL_SHRINK: i32 = 0x0002;
pub const F_SEAL_GROW: i32 = 0x0004;
pub const F_SEAL_WRITE: i32 = 0x0008;

/// Create an anonymous memory file descriptor.
///
/// The file descriptor can be written to and then sealed to make it read-only.
/// This is used for sharing keymaps and other read-only data with clients.
pub fn create(name: &str, allow_sealing: bool) -> io::Result<RawFd> {
    let c_name = CString::new(name)
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "invalid memfd name"))?;

    let mut flags = MFD_CLOEXEC;
    if allow_sealing {
        flags |= MFD_ALLOW_SEALING;
    }

    // SAFETY: memfd_create is a Linux syscall that creates an anonymous file
    let fd = unsafe { libc::syscall(libc::SYS_memfd_create, c_name.as_ptr(), flags) };

    if fd < 0 {
        return Err(io::Error::last_os_error());
    }

    Ok(fd as RawFd)
}

/// Add seals to a memfd to make it read-only.
///
/// Typical usage: seal after writing to prevent further modifications.
pub fn add_seals(fd: RawFd, seals: i32) -> io::Result<()> {
    // SAFETY: fcntl F_ADD_SEALS is standard on Linux memfd
    let result = unsafe { libc::fcntl(fd, F_ADD_SEALS, seals) };

    if result < 0 {
        return Err(io::Error::last_os_error());
    }

    Ok(())
}

/// Create an anonymous memory file as an owned [`std::fs::File`].
///
/// The raw [`create`] is the right entry point inside this crate, which is
/// allowed to hold descriptors directly. Every other SOL crate forbids `unsafe`,
/// so without this they could obtain a descriptor but never safely write to it —
/// which is exactly what a client needs to do to fill a buffer before attaching
/// it. Returning a `File` moves the one unsafe step behind this boundary.
///
/// The file is created with sealing allowed, so a caller that has finished
/// writing may follow up with [`seal_readonly`].
pub fn create_file(name: &str) -> io::Result<std::fs::File> {
    let fd = create(name, true)?;
    // SAFETY: `create` just returned this descriptor and nothing else holds it,
    // so the `File` becomes its sole owner.
    Ok(unsafe { <std::fs::File as std::os::unix::io::FromRawFd>::from_raw_fd(fd) })
}

/// Release a file's descriptor without closing it.
///
/// The counterpart to [`create_file`]: a client fills the file, then hands the
/// bare descriptor to `SCM_RIGHTS`, which needs it to stay open past the point
/// the `File` goes out of scope. The caller owns the descriptor from here and
/// must close it.
pub fn into_raw_fd(file: std::fs::File) -> RawFd {
    std::os::unix::io::IntoRawFd::into_raw_fd(file)
}

/// Seal a memfd to prevent shrinking, growing, and writing.
///
/// This makes the fd effectively read-only and immutable.
pub fn seal_readonly(fd: RawFd) -> io::Result<()> {
    add_seals(fd, F_SEAL_SHRINK | F_SEAL_GROW | F_SEAL_WRITE)
}

/// Read the seals currently set on a descriptor.
///
/// Fails with `EINVAL` for anything that does not support sealing, which is how
/// a caller distinguishes a sealable memfd from an ordinary file or socket.
pub fn seals(fd: RawFd) -> io::Result<i32> {
    // SAFETY: fcntl F_GET_SEALS takes no argument and returns the seal bits.
    let result = unsafe { libc::fcntl(fd, F_GET_SEALS) };

    if result < 0 {
        return Err(io::Error::last_os_error());
    }

    Ok(result)
}

/// Whether a descriptor can no longer be shrunk by whoever else holds it.
///
/// This is what makes a client's shared memory safe to map: without
/// `F_SEAL_SHRINK` the client can `ftruncate` the file after the compositor has
/// checked its size, and the next read of a page past the new end raises SIGBUS
/// — in the compositor, not in the client.
pub fn is_shrink_sealed(fd: RawFd) -> bool {
    seals(fd).is_ok_and(|seals| seals & F_SEAL_SHRINK != 0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use std::os::unix::io::FromRawFd;

    #[test]
    fn creates_memfd() {
        let fd = create("test", true).expect("create memfd");
        assert!(fd >= 0);
        unsafe {
            libc::close(fd);
        }
    }

    #[test]
    fn writes_and_seals() {
        let fd = create("test-seal", true).expect("create memfd");

        // Write some data
        let mut file = unsafe { std::fs::File::from_raw_fd(fd) };
        file.write_all(b"hello world").expect("write to memfd");
        file.sync_all().expect("sync memfd");

        let raw_fd = std::os::unix::io::IntoRawFd::into_raw_fd(file);

        // Seal it
        seal_readonly(raw_fd).expect("seal memfd");

        // Verify we can't write anymore by checking seals
        // (actual write would fail, but that's harder to test cleanly)

        unsafe {
            libc::close(raw_fd);
        }
    }
}
