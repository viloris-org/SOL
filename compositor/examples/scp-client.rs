//! Native SCP connect → surface → toplevel round-trip.

use sol_compositor::scp::{
    protocol::{ClientMessage, CompositorMessage},
    transport::{DEFAULT_SOCKET_NAME, read_frame, write_frame},
};
use std::{io, os::unix::net::UnixStream, path::PathBuf, time::Duration};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let socket_path = socket_path()?;
    let mut stream = UnixStream::connect(&socket_path)?;
    stream.set_read_timeout(Some(Duration::from_secs(5)))?;
    stream.set_write_timeout(Some(Duration::from_secs(5)))?;

    let app_id = std::fs::read_to_string(format!("/proc/{}/comm", std::process::id()))?
        .trim()
        .to_string();
    write_frame(
        &mut stream,
        &ClientMessage::Connect {
            app_id,
            pid: std::process::id(),
        },
    )?;

    let connected: CompositorMessage = read_frame(&mut stream)?;
    let token = match connected {
        CompositorMessage::Connected {
            capability_tokens, ..
        } => capability_tokens
            .get("window-toplevel")
            .cloned()
            .ok_or("compositor did not grant window-toplevel")?,
        other => return Err(format!("connection failed: {other:?}").into()),
    };

    write_frame(&mut stream, &ClientMessage::CreateSurface { surface_id: 1 })?;
    write_frame(
        &mut stream,
        &ClientMessage::CreateToplevel {
            surface_id: 1,
            capability_token: token,
            title: "SCP Example Window".to_string(),
        },
    )?;

    let configured: CompositorMessage = read_frame(&mut stream)?;
    let (toplevel_id, serial) = match configured {
        CompositorMessage::ConfigureToplevel {
            toplevel_id,
            serial,
            width,
            height,
            decoration_height,
            ..
        } => {
            println!(
                "SCP toplevel {toplevel_id} configured at {width}×{height} ({decoration_height}px server decoration)"
            );
            (toplevel_id, serial)
        }
        other => return Err(format!("toplevel creation failed: {other:?}").into()),
    };

    write_frame(
        &mut stream,
        &ClientMessage::AckConfigure {
            toplevel_id,
            serial,
        },
    )?;
    write_frame(
        &mut stream,
        &ClientMessage::Commit {
            surface_id: 1,
            frame_callback: Some(1),
        },
    )?;
    println!("SCP native round-trip complete");
    Ok(())
}

fn socket_path() -> io::Result<PathBuf> {
    let runtime_dir = std::env::var_os("XDG_RUNTIME_DIR")
        .map(PathBuf::from)
        .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "XDG_RUNTIME_DIR is not set"))?;
    let configured = std::env::var_os("SOL_SCP_SOCKET")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(DEFAULT_SOCKET_NAME));
    if configured.is_absolute() {
        Ok(configured)
    } else {
        Ok(runtime_dir.join(configured))
    }
}
