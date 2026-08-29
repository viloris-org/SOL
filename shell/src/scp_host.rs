//! Native SCP layer-surface host for the SOL desktop.
//!
//! Every Shell surface — wallpaper, top bar, Dock, Launcher — is projected by
//! its own renderer-neutral module into a [`LayerPlacement`] plus a block of
//! RGBA pixels. This module is the one place that turns those two things into
//! SCP: a capability request, a layer surface, a sealed shared-memory buffer,
//! and a commit.
//!
//! Keeping the boundary at [`DesktopHost`] rather than at the protocol is what
//! lets the surface modules stay testable. A test drives a
//! [`RecordingDesktopHost`] and asserts on placement and pixels; the desktop
//! session drives [`ScpDesktopHost`] and the same code reaches a compositor.
//!
//! ## Surfaces are named, not numbered
//!
//! A caller addresses its surface by namespace (`sol.desktop`, `sol.topbar`, …)
//! and never sees a `SurfaceId` or a `LayerSurfaceId`. The host owns that
//! mapping, which is what allows a surface to be created lazily on its first
//! present, re-placed when its geometry changes, and withdrawn by name.

use std::{
    collections::BTreeMap,
    fmt, io,
    os::unix::net::UnixStream,
    time::Duration,
};

use sol_compositor::scp::{
    memfd,
    protocol::{
        BufferFormat, ClientMessage, CompositorMessage, LayerKeyboardInteractivity,
        LayerShellLayer as ScpLayer, LayerSurfaceId, SurfaceId,
    },
    resolve_socket_path,
    transport::{read_frame, write_frame, write_frame_with_fd},
    unix_socket,
};

use crate::overlay::LayerShellLayer;

/// The capability a trusted Shell surface needs from the compositor.
const LAYER_SHELL_CAPABILITY: &str = "layer-shell";

/// Why the Shell is asking for it, recorded in the compositor's audit log.
const LAYER_SHELL_JUSTIFICATION: &str = "Render trusted SOL system UI";

/// How long a blocking protocol exchange waits before giving up.
///
/// Generous: a compositor under load may take a while, and a Shell that gives
/// up early turns a slow frame into a lost desktop. Short enough that a wedged
/// compositor does not hang the session forever.
const EXCHANGE_TIMEOUT: Duration = Duration::from_secs(10);

/// How long an event poll blocks before returning to the desktop's own loop.
const POLL_TIMEOUT: Duration = Duration::from_millis(50);

/// Which output edges a Shell surface is pinned to.
///
/// An axis with neither edge set is centered by the compositor, which is how
/// the Dock reaches the bottom center without the Shell computing a margin from
/// an output width it would have to recompute on every mode change.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct LayerAnchor {
    pub top: bool,
    pub bottom: bool,
    pub left: bool,
    pub right: bool,
}

impl LayerAnchor {
    /// Pinned to all four edges: the surface fills its output.
    pub const FULL: Self = Self {
        top: true,
        bottom: true,
        left: true,
        right: true,
    };

    /// Spanning the top edge, as the top bar does.
    pub const TOP_BAR: Self = Self {
        top: true,
        bottom: false,
        left: true,
        right: true,
    };

    /// Pinned to the bottom edge and centered horizontally, as the Dock does.
    pub const BOTTOM_CENTER: Self = Self {
        top: false,
        bottom: true,
        left: false,
        right: false,
    };
}

/// Logical margins between a surface and the edges it is anchored to.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct LayerMargin {
    pub top: i32,
    pub right: i32,
    pub bottom: i32,
    pub left: i32,
}

impl LayerMargin {
    /// A margin on the bottom edge only.
    pub const fn bottom(bottom: i32) -> Self {
        Self {
            top: 0,
            right: 0,
            bottom,
            left: 0,
        }
    }
}

/// Keyboard ownership a Shell surface asks the compositor for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LayerKeyboard {
    /// Never takes keyboard focus: wallpaper, Dock, top bar.
    None,
    /// Takes keyboard focus while mapped: the Launcher.
    Exclusive,
    /// Takes keyboard focus only when the user interacts with it.
    OnDemand,
}

/// Everything the compositor needs to place one Shell surface.
///
/// Sizes are **physical** pixels, matching the buffer that accompanies them: a
/// placement and its pixels are presented together and must agree, so carrying
/// two coordinate systems across this boundary would only invite them to drift.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LayerPlacement {
    /// Stable Shell-chosen surface name, used to address it later.
    pub namespace: String,
    /// Stacking layer.
    pub layer: LayerShellLayer,
    /// Output edges the surface is pinned to.
    pub anchor: LayerAnchor,
    /// Distance from each anchored edge.
    pub margin: LayerMargin,
    /// Physical surface extent. A zero on an axis defers to the anchor: a
    /// surface stretched across an axis is sized by the compositor.
    pub size: (i32, i32),
    /// Work area reserved from other surfaces, in physical pixels.
    pub exclusive_zone: i32,
    /// Keyboard ownership.
    pub keyboard: LayerKeyboard,
}

impl LayerPlacement {
    /// Number of bytes a buffer for this placement must contain.
    #[must_use]
    pub fn buffer_len(&self) -> Option<usize> {
        let width = usize::try_from(self.size.0).ok()?;
        let height = usize::try_from(self.size.1).ok()?;
        width.checked_mul(height)?.checked_mul(4)
    }
}

/// The native boundary every SOL Shell surface presents through.
pub trait DesktopHost {
    /// Present one surface's frame, creating or re-placing it as needed.
    ///
    /// `pixels` is premultiplied RGBA8, row-major, exactly
    /// `placement.size.0 * placement.size.1 * 4` bytes.
    fn present(
        &mut self,
        placement: &LayerPlacement,
        pixels: &[u8],
    ) -> Result<(), DesktopHostError>;

    /// Withdraw a surface by namespace. Withdrawing one that was never
    /// presented is not an error: a surface the user never opened is already in
    /// the state `dismiss` asks for.
    fn dismiss(&mut self, namespace: &str) -> Result<(), DesktopHostError>;
}

/// A failure at the native Shell surface boundary.
#[derive(Debug)]
pub enum DesktopHostError {
    /// The connection to the compositor failed.
    Transport(io::Error),
    /// The compositor refused the connection or a capability.
    Refused(String),
    /// The compositor answered something the Shell cannot act on.
    Protocol(String),
    /// The Shell built a frame whose pixels do not match its placement.
    MalformedFrame(String),
}

impl fmt::Display for DesktopHostError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Transport(error) => write!(formatter, "SCP transport failed: {error}"),
            Self::Refused(reason) => write!(formatter, "compositor refused the Shell: {reason}"),
            Self::Protocol(reason) => write!(formatter, "unexpected SCP response: {reason}"),
            Self::MalformedFrame(reason) => write!(formatter, "malformed Shell frame: {reason}"),
        }
    }
}

impl std::error::Error for DesktopHostError {}

impl From<io::Error> for DesktopHostError {
    fn from(error: io::Error) -> Self {
        Self::Transport(error)
    }
}

/// The output geometry the compositor configured the Shell's surfaces against.
///
/// This is the one output contract the desktop's surfaces lay themselves out
/// against. It is kept next to the host rather than restated per surface
/// because it is the host that learns it — from the compositor's first
/// configure — and a second copy is a second thing to fall out of date on a
/// mode change.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct HostOutput {
    /// Physical output extent.
    pub size: (i32, i32),
    /// Fractional output scale.
    pub scale: f32,
}

impl HostOutput {
    /// Describe an output directly, for tests and for a fixed session config.
    #[must_use]
    pub fn new(width: i32, height: i32, scale: f32) -> Self {
        Self {
            size: (width, height),
            scale: if scale > 0.0 { scale } else { 1.0 },
        }
    }

    /// Whether the compositor has reported a usable extent yet.
    #[must_use]
    pub const fn is_configured(&self) -> bool {
        self.size.0 > 0 && self.size.1 > 0
    }

    /// Logical extent: what layout reasons in, before output scale.
    #[must_use]
    pub fn logical_size(&self) -> sol_ui::LogicalSize {
        sol_ui::LogicalSize::new(
            self.size.0 as f32 / self.scale,
            self.size.1 as f32 / self.scale,
        )
    }

    /// Convert a logical length into physical pixels.
    #[must_use]
    pub fn physical(&self, logical: f32) -> i32 {
        (logical * self.scale).round() as i32
    }
}

/// One live layer surface owned by the host.
#[derive(Debug)]
struct HostedSurface {
    surface_id: SurfaceId,
    layer_id: LayerSurfaceId,
    /// The placement currently in effect, so an unchanged frame does not resend
    /// six configuration messages the compositor would only apply identically.
    placement: LayerPlacement,
}

/// A real SCP connection presenting the SOL desktop's Shell surfaces.
pub struct ScpDesktopHost {
    stream: UnixStream,
    layer_token: Vec<u8>,
    surfaces: BTreeMap<String, HostedSurface>,
    next_surface_id: SurfaceId,
    next_callback_id: u32,
    /// Events read while waiting for a reply, handed back by [`Self::poll`].
    ///
    /// The compositor multiplexes queued events onto the same stream as replies,
    /// so a blocking exchange will sometimes read an input event first. Dropping
    /// it would lose a keystroke; this is where it waits instead.
    deferred: Vec<CompositorMessage>,
    output: HostOutput,
}

impl ScpDesktopHost {
    /// Connect to the compositor and acquire the layer-shell capability.
    ///
    /// The output geometry is not queried separately: the compositor's first
    /// configure for a full-output surface *is* the output size, so the host
    /// creates the desktop background first and learns the geometry from it.
    pub fn connect() -> Result<Self, DesktopHostError> {
        let mut stream = UnixStream::connect(resolve_socket_path()?)?;
        stream.set_read_timeout(Some(EXCHANGE_TIMEOUT))?;
        stream.set_write_timeout(Some(EXCHANGE_TIMEOUT))?;

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
            } => match capability_tokens.get(LAYER_SHELL_CAPABILITY) {
                Some(token) => token.clone(),
                None => request_layer_capability(&mut stream)?,
            },
            CompositorMessage::Rejected { reason } => {
                return Err(DesktopHostError::Refused(reason));
            }
            other => return Err(unexpected("connection", &other)),
        };

        Ok(Self {
            stream,
            layer_token,
            surfaces: BTreeMap::new(),
            next_surface_id: 1,
            next_callback_id: 1,
            deferred: Vec::new(),
            output: HostOutput {
                size: (0, 0),
                scale: 1.0,
            },
        })
    }

    /// The output geometry learned from the compositor's configures.
    ///
    /// `(0, 0)` until the first surface has been created; the desktop session
    /// creates its background surface before laying anything else out.
    #[must_use]
    pub const fn output(&self) -> HostOutput {
        self.output
    }

    /// Drain events the compositor has queued, without blocking indefinitely.
    ///
    /// Returns everything buffered during earlier blocking exchanges first, so
    /// events stay in the order the compositor sent them.
    pub fn poll(&mut self) -> Result<Vec<CompositorMessage>, DesktopHostError> {
        let mut events = std::mem::take(&mut self.deferred);
        self.stream.set_read_timeout(Some(POLL_TIMEOUT))?;
        loop {
            match read_frame::<CompositorMessage>(&mut self.stream) {
                Ok(event) => events.push(event),
                Err(error) if would_block(&error) => break,
                Err(error) => {
                    self.stream.set_read_timeout(Some(EXCHANGE_TIMEOUT))?;
                    return Err(error.into());
                }
            }
        }
        self.stream.set_read_timeout(Some(EXCHANGE_TIMEOUT))?;
        Ok(events)
    }

    /// Whether a surface with this namespace is currently mapped.
    #[must_use]
    pub fn is_mapped(&self, namespace: &str) -> bool {
        self.surfaces.contains_key(namespace)
    }

    /// Forget a surface the compositor has closed on its own.
    ///
    /// A closed surface is gone at the protocol level; destroying it again would
    /// be an error. The next present recreates it.
    pub fn forget_closed(&mut self, layer_id: LayerSurfaceId) -> Option<String> {
        let namespace = self
            .surfaces
            .iter()
            .find(|(_, surface)| surface.layer_id == layer_id)
            .map(|(namespace, _)| namespace.clone())?;
        self.surfaces.remove(&namespace);
        Some(namespace)
    }

    /// The namespace of a surface the compositor named in an event.
    ///
    /// Input arrives addressed by `SurfaceId`, and the Shell reasons in
    /// namespaces, so this is the one translation between the two.
    #[must_use]
    pub fn namespace_of(&self, surface_id: SurfaceId) -> Option<&str> {
        self.surfaces
            .iter()
            .find(|(_, surface)| surface.surface_id == surface_id)
            .map(|(namespace, _)| namespace.as_str())
    }

    /// Acknowledge a configure the compositor sent after surface creation.
    ///
    /// Creation acks its own first configure inline. A later one — a mode
    /// change, a scale change — arrives as an event, and must be acknowledged
    /// for the compositor to treat the new geometry as agreed.
    pub fn ack_layer_configure(
        &mut self,
        layer_id: LayerSurfaceId,
        serial: u32,
    ) -> Result<(), DesktopHostError> {
        write_frame(
            &mut self.stream,
            &ClientMessage::AckLayerConfigure { layer_id, serial },
        )?;
        Ok(())
    }

    /// Record a new output extent reported by the compositor.
    pub fn set_output_size(&mut self, width: i32, height: i32, scale: f32) {
        if width > 0 && height > 0 {
            self.output = HostOutput {
                size: (width, height),
                scale: if scale > 0.0 { scale } else { 1.0 },
            };
        }
    }

    fn create_surface(
        &mut self,
        placement: &LayerPlacement,
    ) -> Result<HostedSurface, DesktopHostError> {
        let surface_id = self.next_surface_id;
        self.next_surface_id = self.next_surface_id.saturating_add(1);

        write_frame(&mut self.stream, &ClientMessage::CreateSurface { surface_id })?;
        write_frame(
            &mut self.stream,
            &ClientMessage::CreateLayerSurface {
                surface_id,
                capability_token: self.layer_token.clone(),
                layer: scp_layer(placement.layer),
                namespace: placement.namespace.clone(),
                output_id: None,
            },
        )?;

        let (layer_id, serial, width, height) = self.await_layer_configure()?;
        // The compositor configures a new layer surface to its output's full
        // extent before any anchor or size is set, which is how the Shell learns
        // the output geometry it must lay itself out against.
        self.set_output_size(width, height, self.output.scale);

        let mut surface = HostedSurface {
            surface_id,
            layer_id,
            // Deliberately not `placement`: nothing has been applied yet, and
            // recording it as applied would make `apply_placement` skip the
            // messages that actually configure the surface.
            placement: LayerPlacement {
                namespace: placement.namespace.clone(),
                layer: placement.layer,
                anchor: LayerAnchor::default(),
                margin: LayerMargin::default(),
                size: (0, 0),
                exclusive_zone: 0,
                keyboard: LayerKeyboard::None,
            },
        };
        self.apply_placement(&mut surface, placement)?;
        write_frame(
            &mut self.stream,
            &ClientMessage::AckLayerConfigure { layer_id, serial },
        )?;
        Ok(surface)
    }

    /// Send only the configuration messages whose value actually changed.
    fn apply_placement(
        &mut self,
        surface: &mut HostedSurface,
        placement: &LayerPlacement,
    ) -> Result<(), DesktopHostError> {
        let layer_id = surface.layer_id;
        let current = &surface.placement;

        if current.anchor != placement.anchor {
            write_frame(
                &mut self.stream,
                &ClientMessage::SetLayerAnchor {
                    layer_id,
                    top: placement.anchor.top,
                    bottom: placement.anchor.bottom,
                    left: placement.anchor.left,
                    right: placement.anchor.right,
                },
            )?;
        }
        if current.margin != placement.margin {
            write_frame(
                &mut self.stream,
                &ClientMessage::SetLayerMargin {
                    layer_id,
                    top: placement.margin.top,
                    right: placement.margin.right,
                    bottom: placement.margin.bottom,
                    left: placement.margin.left,
                },
            )?;
        }
        if current.size != placement.size {
            write_frame(
                &mut self.stream,
                &ClientMessage::SetLayerSize {
                    layer_id,
                    width: placement.size.0,
                    height: placement.size.1,
                },
            )?;
        }
        if current.exclusive_zone != placement.exclusive_zone {
            write_frame(
                &mut self.stream,
                &ClientMessage::SetLayerExclusiveZone {
                    layer_id,
                    zone: placement.exclusive_zone,
                },
            )?;
        }
        if current.keyboard != placement.keyboard {
            write_frame(
                &mut self.stream,
                &ClientMessage::SetLayerKeyboardInteractivity {
                    layer_id,
                    interactivity: scp_keyboard(placement.keyboard),
                },
            )?;
        }

        surface.placement = placement.clone();
        Ok(())
    }

    /// Attach a sealed buffer holding `pixels` and commit the surface.
    fn commit_frame(
        &mut self,
        surface: &HostedSurface,
        placement: &LayerPlacement,
        pixels: &[u8],
    ) -> Result<(), DesktopHostError> {
        let (width, height) = placement.size;
        let stride = width
            .checked_mul(4)
            .ok_or_else(|| DesktopHostError::MalformedFrame("surface stride overflows".into()))?;

        // A buffer is allocated per frame and sealed read-only rather than being
        // reused. That costs an allocation, and it buys two things worth more:
        // the compositor can require `F_SEAL_SHRINK` on everything it maps, and
        // the Shell can never scribble into a buffer the compositor is still
        // compositing from. Shell surfaces repaint on change, not per vblank, so
        // the cost lands only when the desktop actually changed.
        let mut file = memfd::create_file("sol-shell-surface")?;
        std::io::Write::write_all(&mut file, pixels)?;
        std::io::Write::flush(&mut file)?;
        let fd = memfd::into_raw_fd(file);
        if let Err(error) = memfd::seal_readonly(fd) {
            unix_socket::close_fd(fd);
            return Err(error.into());
        }

        let attach = write_frame_with_fd(
            &mut self.stream,
            &ClientMessage::AttachBuffer {
                surface_id: surface.surface_id,
                buffer_fd: fd,
                width,
                height,
                stride,
                format: BufferFormat::Rgba8888,
            },
            fd,
        );
        // The compositor duplicates the descriptor out of the ancillary data, so
        // the Shell's own copy is finished either way.
        unix_socket::close_fd(fd);
        attach?;

        write_frame(
            &mut self.stream,
            &ClientMessage::Damage {
                surface_id: surface.surface_id,
                x: 0,
                y: 0,
                width,
                height,
            },
        )?;

        let callback_id = self.next_callback_id;
        self.next_callback_id = self.next_callback_id.saturating_add(1);
        write_frame(
            &mut self.stream,
            &ClientMessage::Commit {
                surface_id: surface.surface_id,
                frame_callback: Some(callback_id),
            },
        )?;
        Ok(())
    }

    /// Block until the compositor configures a layer surface, deferring
    /// anything else it sends in the meantime.
    fn await_layer_configure(&mut self) -> Result<(LayerSurfaceId, u32, i32, i32), DesktopHostError> {
        loop {
            match read_frame::<CompositorMessage>(&mut self.stream)? {
                CompositorMessage::ConfigureLayerSurface {
                    layer_id,
                    serial,
                    width,
                    height,
                } => return Ok((layer_id, serial, width, height)),
                CompositorMessage::ProtocolError {
                    code,
                    message,
                    fatal,
                } if fatal => {
                    return Err(DesktopHostError::Protocol(format!(
                        "fatal protocol error {code}: {message}"
                    )));
                }
                other => self.deferred.push(other),
            }
        }
    }
}

impl DesktopHost for ScpDesktopHost {
    fn present(
        &mut self,
        placement: &LayerPlacement,
        pixels: &[u8],
    ) -> Result<(), DesktopHostError> {
        let expected = placement.buffer_len().ok_or_else(|| {
            DesktopHostError::MalformedFrame(format!(
                "surface '{}' has an unrepresentable size {:?}",
                placement.namespace, placement.size
            ))
        })?;
        if expected == 0 || pixels.len() != expected {
            return Err(DesktopHostError::MalformedFrame(format!(
                "surface '{}' supplied {} bytes for a {}x{} frame needing {expected}",
                placement.namespace,
                pixels.len(),
                placement.size.0,
                placement.size.1
            )));
        }

        let mut surface = match self.surfaces.remove(&placement.namespace) {
            Some(mut surface) => {
                self.apply_placement(&mut surface, placement)?;
                surface
            }
            None => self.create_surface(placement)?,
        };

        let result = self.commit_frame(&surface, placement, pixels);
        surface.placement = placement.clone();
        // Retain the surface even when the commit failed: it exists at the
        // protocol level, and forgetting it here would leak it and then create a
        // second surface under the same namespace on the next present.
        self.surfaces.insert(placement.namespace.clone(), surface);
        result
    }

    fn dismiss(&mut self, namespace: &str) -> Result<(), DesktopHostError> {
        let Some(surface) = self.surfaces.remove(namespace) else {
            return Ok(());
        };
        write_frame(
            &mut self.stream,
            &ClientMessage::DestroySurface {
                surface_id: surface.surface_id,
            },
        )?;
        Ok(())
    }
}

/// A host that records frames instead of sending them.
///
/// Surface modules are tested against this: it makes placement and pixel
/// content assertable without a compositor, while the code under test is the
/// same code the native host drives.
#[derive(Debug, Default)]
pub struct RecordingDesktopHost {
    /// Every presented frame, in order, as `(placement, pixels)`.
    pub presented: Vec<(LayerPlacement, Vec<u8>)>,
    /// Namespaces withdrawn, in order.
    pub dismissed: Vec<String>,
}

impl RecordingDesktopHost {
    /// The most recent frame presented for one namespace.
    #[must_use]
    pub fn last_frame(&self, namespace: &str) -> Option<&(LayerPlacement, Vec<u8>)> {
        self.presented
            .iter()
            .rev()
            .find(|(placement, _)| placement.namespace == namespace)
    }
}

impl DesktopHost for RecordingDesktopHost {
    fn present(
        &mut self,
        placement: &LayerPlacement,
        pixels: &[u8],
    ) -> Result<(), DesktopHostError> {
        // Apply the same frame validation the native host does, so a surface
        // module cannot pass its tests with a frame the compositor would reject.
        let expected = placement.buffer_len().unwrap_or(0);
        if expected == 0 || pixels.len() != expected {
            return Err(DesktopHostError::MalformedFrame(format!(
                "surface '{}' supplied {} bytes for a {}x{} frame needing {expected}",
                placement.namespace,
                pixels.len(),
                placement.size.0,
                placement.size.1
            )));
        }
        self.presented.push((placement.clone(), pixels.to_vec()));
        Ok(())
    }

    fn dismiss(&mut self, namespace: &str) -> Result<(), DesktopHostError> {
        self.dismissed.push(namespace.to_owned());
        Ok(())
    }
}

const fn scp_layer(layer: LayerShellLayer) -> ScpLayer {
    match layer {
        LayerShellLayer::Background => ScpLayer::Background,
        LayerShellLayer::Bottom => ScpLayer::Bottom,
        LayerShellLayer::Top => ScpLayer::Top,
        LayerShellLayer::Overlay => ScpLayer::Overlay,
    }
}

const fn scp_keyboard(keyboard: LayerKeyboard) -> LayerKeyboardInteractivity {
    match keyboard {
        LayerKeyboard::None => LayerKeyboardInteractivity::None,
        LayerKeyboard::Exclusive => LayerKeyboardInteractivity::Exclusive,
        LayerKeyboard::OnDemand => LayerKeyboardInteractivity::OnDemand,
    }
}

fn would_block(error: &io::Error) -> bool {
    matches!(
        error.kind(),
        io::ErrorKind::WouldBlock | io::ErrorKind::TimedOut
    )
}

fn request_layer_capability(stream: &mut UnixStream) -> Result<Vec<u8>, DesktopHostError> {
    write_frame(
        stream,
        &ClientMessage::RequestCapability {
            capability: LAYER_SHELL_CAPABILITY.to_string(),
            justification: LAYER_SHELL_JUSTIFICATION.to_string(),
        },
    )?;
    match read_frame::<CompositorMessage>(stream)? {
        CompositorMessage::CapabilityDecision {
            granted: true,
            token: Some(token),
            ..
        } => Ok(token),
        CompositorMessage::CapabilityDecision { reason, .. } => Err(DesktopHostError::Refused(
            reason.unwrap_or_else(|| "layer-shell capability denied".to_string()),
        )),
        other => Err(unexpected("layer capability", &other)),
    }
}

/// The identity the compositor will independently verify from `/proc`.
fn process_app_id() -> io::Result<String> {
    Ok(
        std::fs::read_to_string(format!("/proc/{}/comm", std::process::id()))?
            .trim()
            .to_string(),
    )
}

fn unexpected(context: &str, response: &CompositorMessage) -> DesktopHostError {
    DesktopHostError::Protocol(format!("during {context}: {response:?}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn placement(namespace: &str, size: (i32, i32)) -> LayerPlacement {
        LayerPlacement {
            namespace: namespace.to_owned(),
            layer: LayerShellLayer::Top,
            anchor: LayerAnchor::TOP_BAR,
            margin: LayerMargin::default(),
            size,
            exclusive_zone: size.1,
            keyboard: LayerKeyboard::None,
        }
    }

    #[test]
    fn a_frame_whose_pixels_do_not_match_its_placement_is_refused() {
        let mut host = RecordingDesktopHost::default();
        let error = host
            .present(&placement("sol.topbar", (4, 2)), &[0; 16])
            .expect_err("a short buffer must not reach the compositor");

        assert!(matches!(error, DesktopHostError::MalformedFrame(_)));
        assert!(host.presented.is_empty());
    }

    #[test]
    fn a_well_formed_frame_is_recorded_with_its_placement() {
        let mut host = RecordingDesktopHost::default();
        host.present(&placement("sol.topbar", (4, 2)), &[7; 32])
            .expect("present a matching frame");

        let (recorded, pixels) = host.last_frame("sol.topbar").expect("recorded frame");
        assert_eq!(recorded.size, (4, 2));
        assert_eq!(recorded.exclusive_zone, 2);
        assert_eq!(pixels.len(), 32);
    }

    #[test]
    fn a_zero_sized_placement_is_never_presentable() {
        let mut host = RecordingDesktopHost::default();
        assert!(host.present(&placement("sol.dock", (0, 40)), &[]).is_err());
        assert_eq!(placement("sol.dock", (0, 40)).buffer_len(), Some(0));
        assert_eq!(placement("sol.dock", (-1, 40)).buffer_len(), None);
    }

    #[test]
    fn anchors_map_onto_the_compositors_edge_flags() {
        const {
            assert!(LayerAnchor::FULL.top && LayerAnchor::FULL.bottom);
            assert!(LayerAnchor::TOP_BAR.left && LayerAnchor::TOP_BAR.right);
            assert!(!LayerAnchor::TOP_BAR.bottom);
            // Neither horizontal edge: the compositor centers it.
            assert!(!LayerAnchor::BOTTOM_CENTER.left && !LayerAnchor::BOTTOM_CENTER.right);
            assert!(LayerAnchor::BOTTOM_CENTER.bottom);
        }
    }
}
