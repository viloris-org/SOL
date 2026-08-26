//! Cryptographically secure random number generation without external dependencies.
//!
//! Uses getrandom(2) syscall directly on Linux.

use std::io;

/// Fill a buffer with cryptographically secure random bytes.
///
/// Uses getrandom(2) syscall on Linux, which is available since kernel 3.17.
/// This replaces the `rand` crate dependency for token generation.
pub fn fill_bytes(dest: &mut [u8]) -> io::Result<()> {
    let mut offset = 0;
    while offset < dest.len() {
        // SAFETY: getrandom is a read-only syscall that fills the buffer
        let result = unsafe {
            libc::syscall(
                libc::SYS_getrandom,
                dest.as_mut_ptr().add(offset),
                dest.len() - offset,
                0,
            )
        };

        if result < 0 {
            let err = io::Error::last_os_error();
            if err.kind() == io::ErrorKind::Interrupted {
                continue;
            }
            return Err(err);
        }

        offset += result as usize;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generates_random_bytes() {
        let mut buf1 = [0u8; 32];
        let mut buf2 = [0u8; 32];

        fill_bytes(&mut buf1).expect("fill buf1");
        fill_bytes(&mut buf2).expect("fill buf2");

        // Extremely unlikely to be equal if truly random
        assert_ne!(buf1, buf2);

        // Should not be all zeros
        assert_ne!(buf1, [0u8; 32]);
    }

    #[test]
    fn handles_partial_reads() {
        let mut buf = [0u8; 256];
        fill_bytes(&mut buf).expect("fill large buffer");
        assert_ne!(buf, [0u8; 256]);
    }
}
