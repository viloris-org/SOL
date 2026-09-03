//! Immutable capture-frame export through SCM_RIGHTS.

use crate::scp::{compose::Framebuffer, memfd};
use std::{
    io::{Seek, Write},
    os::fd::{FromRawFd, IntoRawFd, OwnedFd},
};

/// Turn a composed frame into a sealed descriptor positioned at byte zero.
pub fn export_frame(frame: &Framebuffer) -> std::io::Result<OwnedFd> {
    let fd = memfd::create("sol-capture", true)?;
    // SAFETY: `create` returned a new descriptor and transfers its ownership.
    let mut file = unsafe { std::fs::File::from_raw_fd(fd) };
    file.write_all(frame.pixels())?;
    file.flush()?;
    file.rewind()?;
    let fd = file.into_raw_fd();
    if let Err(error) = memfd::seal_readonly(fd) {
        crate::scp::unix_socket::close_fd(fd);
        return Err(error);
    }
    // SAFETY: ownership was taken from `file` above and has not been shared.
    Ok(unsafe { OwnedFd::from_raw_fd(fd) })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scp::compose::Rgba8;
    use std::os::fd::AsRawFd;

    #[test]
    fn exported_frames_are_sized_and_immutable() {
        let frame = Framebuffer::filled(3, 2, Rgba8(1, 2, 3, 255)).unwrap();
        let fd = export_frame(&frame).unwrap();
        assert_eq!(
            crate::scp::unix_socket::fd_size(fd.as_raw_fd()).unwrap(),
            24
        );
        let seals = memfd::seals(fd.as_raw_fd()).unwrap();
        assert_ne!(seals & memfd::F_SEAL_WRITE, 0);
        assert_ne!(seals & memfd::F_SEAL_SHRINK, 0);
    }
}
