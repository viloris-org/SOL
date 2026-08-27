//! Native SOL Compositor Protocol client used by the desktop shell.

use sol_compositor::scp::{
    protocol::{
        ClientMessage, CompositorMessage, LayerKeyboardInteractivity, LayerShellLayer,
        LayerSurfaceId, SurfaceId,
    },
    resolve_socket_path,
    transport::{read_frame, write_frame},
};
use std::{io, os::unix::net::UnixStream, time::Duration};

const SHELL_SURFACE_ID: SurfaceId = 1;
const BAR_HEIGHT: i32 = 40;

pub struct ShellClient {
    stream: UnixStream,
    layer_id: LayerSurfaceId,
}

impl ShellClient {
    pub fn connect() -> Result<Self, Box<dyn std::error::Error>> {
        let mut stream = UnixStream::connect(resolve_socket_path()?)?;
        stream.set_read_timeout(Some(Duration::from_secs(10)))?;
        stream.set_write_timeout(Some(Duration::from_secs(10)))?;

        write_frame(
            &mut stream,
            &ClientMessage::Connect {
                app_id: process_app_id()?,
                pid: std::process::id(),
            },
        )?;
        let layer_token = match read_frame::<CompositorMessage>(&mut stream)? {
            CompositorMessage::Connected {
                capability_tokens, ..
            } => match capability_tokens.get("layer-shell") {
                Some(token) => token.clone(),
                None => request_layer_capability(&mut stream)?,
            },
            CompositorMessage::Rejected { reason } => {
                return Err(format!("SCP connection rejected: {reason}").into());
            }
            response => return Err(unexpected("connection", response).into()),
        };

        write_frame(
            &mut stream,
            &ClientMessage::CreateSurface {
                surface_id: SHELL_SURFACE_ID,
            },
        )?;
        write_frame(
            &mut stream,
            &ClientMessage::CreateLayerSurface {
                surface_id: SHELL_SURFACE_ID,
                capability_token: layer_token,
                layer: LayerShellLayer::Top,
                namespace: "sol-shell".to_string(),
                output_id: None,
            },
        )?;

        let (layer_id, serial, width, height) = match read_frame(&mut stream)? {
            CompositorMessage::ConfigureLayerSurface {
                layer_id,
                serial,
                width,
                height,
            } => (layer_id, serial, width, height),
            response => return Err(unexpected("layer configure", response).into()),
        };

        for message in [
            ClientMessage::SetLayerAnchor {
                layer_id,
                top: true,
                bottom: false,
                left: true,
                right: true,
            },
            ClientMessage::SetLayerExclusiveZone {
                layer_id,
                zone: BAR_HEIGHT,
            },
            ClientMessage::SetLayerKeyboardInteractivity {
                layer_id,
                interactivity: LayerKeyboardInteractivity::None,
            },
            ClientMessage::SetLayerSize {
                layer_id,
                width,
                height: BAR_HEIGHT.min(height),
            },
            ClientMessage::AckLayerConfigure { layer_id, serial },
            ClientMessage::Commit {
                surface_id: SHELL_SURFACE_ID,
                frame_callback: None,
            },
        ] {
            write_frame(&mut stream, &message)?;
        }

        tracing::info!(
            layer_id,
            width,
            height = BAR_HEIGHT.min(height),
            "SCP shell surface committed"
        );
        Ok(Self { stream, layer_id })
    }

    pub fn run(mut self, running: &std::sync::atomic::AtomicBool) -> io::Result<()> {
        self.stream
            .set_read_timeout(Some(Duration::from_millis(250)))?;
        while running.load(std::sync::atomic::Ordering::Acquire) {
            match read_frame::<CompositorMessage>(&mut self.stream) {
                Ok(CompositorMessage::LayerSurfaceClosed { layer_id })
                    if layer_id == self.layer_id =>
                {
                    return Ok(());
                }
                Ok(CompositorMessage::ProtocolError {
                    code,
                    message,
                    fatal,
                }) => {
                    tracing::warn!(%code, %message, fatal, "SCP protocol error");
                    if fatal {
                        return Ok(());
                    }
                }
                Ok(_) => {}
                Err(error)
                    if matches!(
                        error.kind(),
                        io::ErrorKind::WouldBlock | io::ErrorKind::TimedOut
                    ) => {}
                Err(error) => return Err(error),
            }
        }
        Ok(())
    }
}

fn request_layer_capability(
    stream: &mut UnixStream,
) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    write_frame(
        stream,
        &ClientMessage::RequestCapability {
            capability: "layer-shell".to_string(),
            justification: "Render trusted SOL system UI".to_string(),
        },
    )?;
    match read_frame::<CompositorMessage>(stream)? {
        CompositorMessage::CapabilityDecision {
            granted: true,
            token: Some(token),
            ..
        } => Ok(token),
        CompositorMessage::CapabilityDecision { reason, .. } => Err(reason
            .unwrap_or_else(|| "layer-shell capability denied".to_string())
            .into()),
        response => Err(unexpected("layer capability", response).into()),
    }
}

fn process_app_id() -> io::Result<String> {
    Ok(
        std::fs::read_to_string(format!("/proc/{}/comm", std::process::id()))?
            .trim()
            .to_string(),
    )
}

fn unexpected(context: &str, response: CompositorMessage) -> String {
    format!("unexpected SCP response during {context}: {response:?}")
}
