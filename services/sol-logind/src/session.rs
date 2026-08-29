//! Launches the authenticated user's desktop session.
//!
//! sol-logind does not spawn the compositor/shell itself — `sol-session`
//! (crate `session/`) already owns that process ordering (compositor, wait
//! for the SCP socket, companion services, then shell). This module's job is
//! narrower: resolve the session environment for the authenticated account,
//! drop from root to that account, and exec `sol-session`.

use std::{
    env,
    ffi::{CString, OsString},
    fs, io,
    net::Shutdown,
    os::unix::{fs::PermissionsExt, process::CommandExt},
    path::{Path, PathBuf},
    process::{Child, Command, ExitStatus},
    thread,
    time::{Duration, Instant},
};

use anyhow::{Context, Result};
use std::os::unix::net::UnixListener;

use crate::users::UserAccount;

const DEFAULT_PATH: &str = "/usr/local/bin:/usr/bin:/bin";
const READY_POLL_INTERVAL: Duration = Duration::from_millis(16);

/// A user session that is starting behind the still-engaged login surface.
pub struct PendingUserSession {
    child: Child,
    ready_listener: UnixListener,
    ready_path: PathBuf,
}

impl PendingUserSession {
    /// Wait until the shell reports that its first SCP surface is committed.
    pub fn wait_until_ready(&mut self, timeout: Duration) -> Result<()> {
        let deadline = Instant::now() + timeout;
        loop {
            match self.ready_listener.accept() {
                Ok((stream, _)) => {
                    let _ = stream.shutdown(Shutdown::Both);
                    return Ok(());
                }
                Err(error) if error.kind() == io::ErrorKind::WouldBlock => {}
                Err(error) => return Err(error).context("desktop readiness channel failed"),
            }

            if let Some(status) = self
                .child
                .try_wait()
                .context("failed to inspect the starting user session")?
            {
                anyhow::bail!("user session exited before its desktop was ready: {status}");
            }
            if Instant::now() >= deadline {
                anyhow::bail!("desktop did not become ready within {}s", timeout.as_secs());
            }
            thread::sleep(READY_POLL_INTERVAL);
        }
    }

    /// Wait for the active desktop session to end.
    pub fn wait(mut self) -> Result<ExitStatus> {
        self.child.wait().context("failed to wait for sol-session")
    }

    /// Stop a session that failed before the display could be handed to it.
    pub fn abort(mut self) {
        if self.child.try_wait().ok().flatten().is_none() {
            let _ = self.child.kill();
            let _ = self.child.wait();
        }
    }
}

impl Drop for PendingUserSession {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.ready_path);
    }
}

/// Start `sol-session` for `user` while the login surface remains engaged.
///
/// In production this drops privileges to `user` before exec (requires
/// running as root; otherwise the drop is skipped with a warning so the
/// binary remains runnable in ad-hoc, non-rooted testing). In dev mode the
/// session runs as the current user, inheriting the current environment, so
/// the full login→session chain can be exercised locally.
pub fn start_user_session(user: &UserAccount, dev_mode: bool) -> Result<PendingUserSession> {
    let program = session_binary();
    let compositor_socket = sol_compositor::scp::resolve_socket_path()
        .context("could not resolve the compositor used by the login screen")?;
    let runtime_dir = resolve_runtime_dir(
        user.uid,
        dev_mode,
        env::var_os("XDG_RUNTIME_DIR").map(PathBuf::from).as_deref(),
    );
    ensure_runtime_dir(&runtime_dir, user, dev_mode)?;
    let (ready_listener, ready_path) = readiness_listener(&runtime_dir)?;

    let mut command = Command::new(&program);
    // The compositor owns the physical seat for the entire boot. A login starts
    // only the per-user half of the session and attaches it to that compositor;
    // starting a second compositor here would necessarily introduce a display
    // mode/input handoff and make a frame-perfect transition impossible.
    command
        .arg("--attach")
        .arg("--socket")
        .arg(&compositor_socket);

    if dev_mode {
        // Inherit the developer's real environment; only fill in what's
        // required for sol-session to find its socket directory.
        command.env("XDG_RUNTIME_DIR", &runtime_dir);
    } else {
        command.env_clear();
        command.envs(session_environment(user, &runtime_dir));
        command.current_dir(&user.home_dir);
        drop_privileges_before_exec(&mut command, user);
    }
    command.env("SOL_SCP_SOCKET", &compositor_socket);
    command.env("SOL_SESSION_READY_SOCKET", &ready_path);
    // The installed Shell lives in the compositor's trusted binary directory;
    // forward only this explicit launcher override across the cleared login
    // environment rather than inheriting arbitrary root process variables.
    if let Some(shell) = env::var_os("SOL_SHELL_BIN") {
        command.env("SOL_SHELL_BIN", shell);
    }
    if let Some(bus) = env::var_os("DBUS_SESSION_BUS_ADDRESS") {
        command.env("DBUS_SESSION_BUS_ADDRESS", bus);
    }

    tracing::info!(
        user = %user.username,
        program = %program.display(),
        dev_mode,
        "launching user session"
    );

    let child = command
        .spawn()
        .with_context(|| format!("failed to spawn {}", program.display()))?;
    Ok(PendingUserSession {
        child,
        ready_listener,
        ready_path,
    })
}

fn readiness_listener(runtime_dir: &Path) -> Result<(UnixListener, PathBuf)> {
    for _ in 0..8 {
        let nonce = rand::random::<u64>();
        let path = runtime_dir.join(format!(
            ".sol-session-ready-{}-{nonce:016x}",
            std::process::id()
        ));
        match UnixListener::bind(&path) {
            Ok(listener) => {
                listener
                    .set_nonblocking(true)
                    .context("could not make desktop readiness channel non-blocking")?;
                return Ok((listener, path));
            }
            Err(error) if error.kind() == io::ErrorKind::AddrInUse => continue,
            Err(error) => return Err(error).context("could not create desktop readiness channel"),
        }
    }
    anyhow::bail!("could not allocate a unique desktop readiness channel")
}

fn session_binary() -> PathBuf {
    env::var_os("SOL_SESSION_BIN")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("sol-session"))
}

/// Determine the `XDG_RUNTIME_DIR` for the session.
///
/// Production always uses the standard `/run/user/{uid}` location. Dev mode
/// reuses whatever runtime dir the developer's own session already has, so
/// running `--dev` from an existing desktop doesn't require a second one.
fn resolve_runtime_dir(uid: u32, dev_mode: bool, existing: Option<&Path>) -> PathBuf {
    if dev_mode && let Some(existing) = existing {
        return existing.to_path_buf();
    }
    PathBuf::from(format!("/run/user/{uid}"))
}

/// Build the minimal environment the session process (and, by inheritance,
/// everything `sol-session` spawns under it) needs.
fn session_environment(user: &UserAccount, runtime_dir: &Path) -> Vec<(OsString, OsString)> {
    vec![
        (
            OsString::from("HOME"),
            user.home_dir.clone().into_os_string(),
        ),
        (OsString::from("USER"), OsString::from(&user.username)),
        (OsString::from("LOGNAME"), OsString::from(&user.username)),
        (OsString::from("SHELL"), user.shell.clone().into_os_string()),
        (OsString::from("PATH"), OsString::from(DEFAULT_PATH)),
        (
            OsString::from("XDG_RUNTIME_DIR"),
            runtime_dir.as_os_str().to_os_string(),
        ),
    ]
}

/// Ensure the runtime directory exists with `0700` permissions owned by the
/// target user. In dev mode we don't own privilege escalation, so the
/// directory (already the developer's own) is left untouched.
fn ensure_runtime_dir(runtime_dir: &Path, user: &UserAccount, dev_mode: bool) -> Result<()> {
    if dev_mode {
        fs::create_dir_all(runtime_dir)
            .with_context(|| format!("failed to create {}", runtime_dir.display()))?;
        return Ok(());
    }

    if runtime_dir.exists() {
        // PAM (pam_systemd/elogind), or a previous login, likely already
        // created this with the right ownership.
        return Ok(());
    }

    fs::create_dir_all(runtime_dir)
        .with_context(|| format!("failed to create {}", runtime_dir.display()))?;
    fs::set_permissions(runtime_dir, fs::Permissions::from_mode(0o700))
        .with_context(|| format!("failed to set permissions on {}", runtime_dir.display()))?;

    if unsafe { libc::geteuid() } == 0 {
        chown(runtime_dir, user.uid, user.gid)
            .with_context(|| format!("failed to chown {}", runtime_dir.display()))?;
    }
    Ok(())
}

fn chown(path: &Path, uid: u32, gid: u32) -> io::Result<()> {
    let path = CString::new(path.as_os_str().as_encoded_bytes())
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidInput, error))?;
    let result = unsafe { libc::chown(path.as_ptr(), uid, gid) };
    if result != 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}

/// Drop from root to `user` right before `exec`, via `pre_exec`.
///
/// Everything the closure needs is precomputed here — allocating after
/// `fork()` but before `exec()` is not async-signal-safe.
fn drop_privileges_before_exec(command: &mut Command, user: &UserAccount) {
    if unsafe { libc::geteuid() } != 0 {
        tracing::warn!(
            user = %user.username,
            "not running as root; launching session without dropping privileges"
        );
        return;
    }

    let Ok(username) = CString::new(user.username.as_str()) else {
        tracing::error!(user = %user.username, "username contains a NUL byte; cannot drop privileges");
        return;
    };
    let uid = user.uid;
    let gid = user.gid;

    // Safety: the closure only calls raw, async-signal-safe libc functions
    // and touches no heap-allocated state beyond what's captured here.
    unsafe {
        command.pre_exec(move || {
            if libc::initgroups(username.as_ptr(), gid) != 0 {
                return Err(io::Error::last_os_error());
            }
            if libc::setgid(gid) != 0 {
                return Err(io::Error::last_os_error());
            }
            if libc::setuid(uid) != 0 {
                return Err(io::Error::last_os_error());
            }
            Ok(())
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::net::UnixStream;

    fn user() -> UserAccount {
        let mut user = UserAccount::new("jdoe".into(), "John Doe".into(), 1000);
        user.home_dir = PathBuf::from("/home/jdoe");
        user.shell = PathBuf::from("/bin/zsh");
        user
    }

    #[test]
    fn production_runtime_dir_is_always_run_user_uid() {
        assert_eq!(
            resolve_runtime_dir(1000, false, Some(Path::new("/run/user/42"))),
            PathBuf::from("/run/user/1000")
        );
        assert_eq!(
            resolve_runtime_dir(1000, false, None),
            PathBuf::from("/run/user/1000")
        );
    }

    #[test]
    fn dev_runtime_dir_reuses_existing_when_present() {
        assert_eq!(
            resolve_runtime_dir(1000, true, Some(Path::new("/run/user/42"))),
            PathBuf::from("/run/user/42")
        );
    }

    #[test]
    fn dev_runtime_dir_falls_back_when_unset() {
        assert_eq!(
            resolve_runtime_dir(1000, true, None),
            PathBuf::from("/run/user/1000")
        );
    }

    #[test]
    fn session_environment_carries_user_identity() {
        let user = user();
        let env = session_environment(&user, Path::new("/run/user/1000"));
        let get = |key: &str| {
            env.iter()
                .find(|(k, _)| k == key)
                .map(|(_, v)| v.to_string_lossy().into_owned())
        };
        assert_eq!(get("HOME").as_deref(), Some("/home/jdoe"));
        assert_eq!(get("USER").as_deref(), Some("jdoe"));
        assert_eq!(get("LOGNAME").as_deref(), Some("jdoe"));
        assert_eq!(get("SHELL").as_deref(), Some("/bin/zsh"));
        assert_eq!(get("XDG_RUNTIME_DIR").as_deref(), Some("/run/user/1000"));
    }

    #[test]
    fn pending_session_waits_for_the_shell_ready_connection() {
        let runtime_dir = std::env::temp_dir().join(format!(
            "sol-logind-ready-test-{}-{:016x}",
            std::process::id(),
            rand::random::<u64>()
        ));
        fs::create_dir(&runtime_dir).expect("create isolated runtime directory");
        let (ready_listener, ready_path) =
            readiness_listener(&runtime_dir).expect("create readiness listener");
        let child = Command::new("sh")
            .args(["-c", "sleep 5"])
            .spawn()
            .expect("start fake session");
        let connector_path = ready_path.clone();
        let connector = thread::spawn(move || {
            UnixStream::connect(connector_path).expect("signal desktop ready");
        });

        let mut pending = PendingUserSession {
            child,
            ready_listener,
            ready_path: ready_path.clone(),
        };
        pending
            .wait_until_ready(Duration::from_secs(1))
            .expect("observe desktop readiness");
        connector.join().expect("join ready signal");
        pending.abort();

        assert!(!ready_path.exists(), "the one-shot socket must be removed");
        fs::remove_dir(runtime_dir).expect("remove isolated runtime directory");
    }
}
