//! Protobuf encoding for the public SCP domain types.
//!
//! The rest of the compositor intentionally works with the ergonomic enums in
//! [`super::protocol`]. This module is the sole boundary to the generated,
//! language-neutral contract in `protocols/scp/v2/scp.proto`.

use super::protocol as p;
use prost::Message as _;
use std::{collections::HashMap, io};

#[allow(clippy::all, clippy::pedantic, clippy::nursery)]
pub mod generated {
    include!(concat!(env!("OUT_DIR"), "/sol.scp.v2.rs"));
}

pub trait WireMessage: Sized {
    fn encode_wire(&self) -> io::Result<Vec<u8>>;
    fn decode_wire(bytes: &[u8]) -> io::Result<Self>;
}

fn invalid(message: impl Into<String>) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message.into())
}

fn required<T>(value: Option<T>, name: &str) -> io::Result<T> {
    value.ok_or_else(|| invalid(format!("missing protobuf field {name}")))
}

fn empty() -> generated::Empty {
    generated::Empty {}
}
fn id(id: u32) -> generated::Id {
    generated::Id { id }
}
fn sid(surface_id: u32) -> generated::Surface {
    generated::Surface { surface_id }
}

fn rect(value: p::Rect) -> generated::Rect {
    generated::Rect {
        x: value.x,
        y: value.y,
        width: value.width,
        height: value.height,
    }
}

fn from_rect(value: generated::Rect) -> p::Rect {
    p::Rect {
        x: value.x,
        y: value.y,
        width: value.width,
        height: value.height,
    }
}

fn buffer_format(value: p::BufferFormat) -> i32 {
    match value {
        p::BufferFormat::Argb8888 => 1,
        p::BufferFormat::Xrgb8888 => 2,
        p::BufferFormat::Rgba8888 => 3,
        p::BufferFormat::Rgb565 => 4,
    }
}
fn from_buffer_format(value: i32) -> io::Result<p::BufferFormat> {
    match value {
        1 => Ok(p::BufferFormat::Argb8888),
        2 => Ok(p::BufferFormat::Xrgb8888),
        3 => Ok(p::BufferFormat::Rgba8888),
        4 => Ok(p::BufferFormat::Rgb565),
        _ => Err(invalid("invalid buffer format")),
    }
}
fn shm_format(value: p::ShmFormat) -> i32 {
    match value {
        p::ShmFormat::Argb8888 => 1,
        p::ShmFormat::Xrgb8888 => 2,
        p::ShmFormat::Rgb565 => 3,
    }
}
fn from_shm_format(value: i32) -> io::Result<p::ShmFormat> {
    match value {
        1 => Ok(p::ShmFormat::Argb8888),
        2 => Ok(p::ShmFormat::Xrgb8888),
        3 => Ok(p::ShmFormat::Rgb565),
        _ => Err(invalid("invalid shared-memory format")),
    }
}
fn dmabuf_format(value: p::DmabufFormat) -> i32 {
    match value {
        p::DmabufFormat::Argb8888 => 1,
        p::DmabufFormat::Xrgb8888 => 2,
        p::DmabufFormat::Abgr8888 => 3,
        p::DmabufFormat::Xbgr8888 => 4,
        p::DmabufFormat::Rgb565 => 5,
        p::DmabufFormat::Nv12 => 6,
    }
}
fn from_dmabuf_format(value: i32) -> io::Result<p::DmabufFormat> {
    match value {
        1 => Ok(p::DmabufFormat::Argb8888),
        2 => Ok(p::DmabufFormat::Xrgb8888),
        3 => Ok(p::DmabufFormat::Abgr8888),
        4 => Ok(p::DmabufFormat::Xbgr8888),
        5 => Ok(p::DmabufFormat::Rgb565),
        6 => Ok(p::DmabufFormat::Nv12),
        _ => Err(invalid("invalid DMA-BUF format")),
    }
}

fn drag_action(value: p::DragAction) -> i32 {
    match value {
        p::DragAction::Copy => 1,
        p::DragAction::Move => 2,
        p::DragAction::Ask => 3,
    }
}
fn from_drag_action(value: i32) -> io::Result<p::DragAction> {
    match value {
        1 => Ok(p::DragAction::Copy),
        2 => Ok(p::DragAction::Move),
        3 => Ok(p::DragAction::Ask),
        _ => Err(invalid("invalid drag action")),
    }
}
fn cursor_mode(value: p::CursorMode) -> i32 {
    match value {
        p::CursorMode::Include => 1,
        p::CursorMode::Exclude => 2,
    }
}
fn from_cursor_mode(value: i32) -> io::Result<p::CursorMode> {
    match value {
        1 => Ok(p::CursorMode::Include),
        2 => Ok(p::CursorMode::Exclude),
        _ => Err(invalid("invalid cursor mode")),
    }
}
fn capture_format(value: p::CaptureFormat) -> i32 {
    match value {
        p::CaptureFormat::Rgba8888 => 1,
    }
}
fn from_capture_format(value: i32) -> io::Result<p::CaptureFormat> {
    match value {
        1 => Ok(p::CaptureFormat::Rgba8888),
        _ => Err(invalid("invalid capture format")),
    }
}
fn shortcut_priority(value: p::ShortcutPriority) -> i32 {
    match value {
        p::ShortcutPriority::App => 1,
        p::ShortcutPriority::Shell => 2,
        p::ShortcutPriority::System => 3,
    }
}
fn from_shortcut_priority(value: i32) -> io::Result<p::ShortcutPriority> {
    match value {
        1 => Ok(p::ShortcutPriority::App),
        2 => Ok(p::ShortcutPriority::Shell),
        3 => Ok(p::ShortcutPriority::System),
        _ => Err(invalid("invalid shortcut priority")),
    }
}

fn popup_positioner(value: &p::PopupPositioner) -> generated::PopupPositioner {
    generated::PopupPositioner {
        anchor_rect: Some(rect(value.anchor_rect)),
        anchor_edge: match value.anchor_edge {
            p::Edge::Top => 1,
            p::Edge::Bottom => 2,
            p::Edge::Left => 3,
            p::Edge::Right => 4,
        },
        gravity: match value.gravity {
            p::Gravity::None => 1,
            p::Gravity::Top => 2,
            p::Gravity::Bottom => 3,
            p::Gravity::Left => 4,
            p::Gravity::Right => 5,
            p::Gravity::TopLeft => 6,
            p::Gravity::TopRight => 7,
            p::Gravity::BottomLeft => 8,
            p::Gravity::BottomRight => 9,
        },
        constraint: Some(generated::ConstraintAdjustment {
            flip_x: value.constraint.flip_x,
            flip_y: value.constraint.flip_y,
            slide_x: value.constraint.slide_x,
            slide_y: value.constraint.slide_y,
            resize_x: value.constraint.resize_x,
            resize_y: value.constraint.resize_y,
        }),
        offset: Some(generated::Point {
            x: value.offset.0,
            y: value.offset.1,
        }),
        size: Some(generated::Size {
            width: value.size.0,
            height: value.size.1,
        }),
    }
}

fn from_popup_positioner(value: generated::PopupPositioner) -> io::Result<p::PopupPositioner> {
    let anchor_rect = from_rect(required(value.anchor_rect, "positioner.anchor_rect")?);
    let anchor_edge = match value.anchor_edge {
        1 => p::Edge::Top,
        2 => p::Edge::Bottom,
        3 => p::Edge::Left,
        4 => p::Edge::Right,
        _ => return Err(invalid("invalid popup anchor edge")),
    };
    let gravity = match value.gravity {
        1 => p::Gravity::None,
        2 => p::Gravity::Top,
        3 => p::Gravity::Bottom,
        4 => p::Gravity::Left,
        5 => p::Gravity::Right,
        6 => p::Gravity::TopLeft,
        7 => p::Gravity::TopRight,
        8 => p::Gravity::BottomLeft,
        9 => p::Gravity::BottomRight,
        _ => return Err(invalid("invalid popup gravity")),
    };
    let c = required(value.constraint, "positioner.constraint")?;
    let offset = required(value.offset, "positioner.offset")?;
    let size = required(value.size, "positioner.size")?;
    Ok(p::PopupPositioner {
        anchor_rect,
        anchor_edge,
        gravity,
        constraint: p::ConstraintAdjustment {
            flip_x: c.flip_x,
            flip_y: c.flip_y,
            slide_x: c.slide_x,
            slide_y: c.slide_y,
            resize_x: c.resize_x,
            resize_y: c.resize_y,
        },
        offset: (offset.x, offset.y),
        size: (size.width, size.height),
    })
}

impl WireMessage for p::ClientMessage {
    fn encode_wire(&self) -> io::Result<Vec<u8>> {
        use generated::client_message::Message;
        use p::ClientMessage as C;
        let message = match self {
            C::Connect { app_id, pid } => Message::Connect(generated::Connect {
                app_id: app_id.clone(),
                pid: *pid,
            }),
            C::ConnectVersioned {
                app_id,
                pid,
                min_version,
                max_version,
            } => Message::ConnectVersioned(generated::ConnectVersioned {
                app_id: app_id.clone(),
                pid: *pid,
                min_version: *min_version,
                max_version: *max_version,
            }),
            C::CreateSurface { surface_id } => Message::CreateSurface(sid(*surface_id)),
            C::DestroySurface { surface_id } => Message::DestroySurface(sid(*surface_id)),
            C::AttachBuffer {
                surface_id,
                width,
                height,
                stride,
                format,
                ..
            } => Message::AttachBuffer(generated::AttachBuffer {
                surface_id: *surface_id,
                width: *width,
                height: *height,
                stride: *stride,
                format: buffer_format(*format),
            }),
            C::Commit {
                surface_id,
                frame_callback,
            } => Message::Commit(generated::Commit {
                surface_id: *surface_id,
                frame_callback: *frame_callback,
            }),
            C::RequestCapability {
                capability,
                justification,
            } => Message::RequestCapability(generated::RequestCapability {
                capability: capability.clone(),
                justification: justification.clone(),
            }),
            C::CreateToplevel {
                surface_id,
                capability_token,
                title,
            } => Message::CreateToplevel(generated::CreateToplevel {
                surface_id: *surface_id,
                capability_token: capability_token.clone(),
                title: title.clone(),
            }),
            C::SetToplevelTitle { toplevel_id, title } => {
                Message::SetToplevelTitle(generated::SetToplevelTitle {
                    toplevel_id: *toplevel_id,
                    title: title.clone(),
                })
            }
            C::SetFullscreen {
                toplevel_id,
                capability_token,
            } => Message::SetFullscreen(generated::SetFullscreen {
                toplevel_id: *toplevel_id,
                capability_token: capability_token.clone(),
            }),
            C::AckConfigure {
                toplevel_id,
                serial,
            } => Message::AckConfigure(generated::AckConfigure {
                toplevel_id: *toplevel_id,
                serial: *serial,
            }),
            C::CreateShmPool { pool_id, size, .. } => {
                Message::CreateShmPool(generated::CreateShmPool {
                    pool_id: *pool_id,
                    size: u64::try_from(*size)
                        .map_err(|_| invalid("shared-memory size exceeds u64"))?,
                })
            }
            C::CreateBuffer {
                buffer_id,
                pool_id,
                offset,
                width,
                height,
                stride,
                format,
            } => Message::CreateBuffer(generated::CreateBuffer {
                buffer_id: *buffer_id,
                pool_id: *pool_id,
                offset: u64::try_from(*offset).map_err(|_| invalid("buffer offset exceeds u64"))?,
                width: *width,
                height: *height,
                stride: *stride,
                format: shm_format(*format),
            }),
            C::CreateDmabufBuffer {
                buffer_id,
                width,
                height,
                format,
                modifier,
                planes,
                ..
            } => Message::CreateDmabufBuffer(generated::CreateDmabufBuffer {
                buffer_id: *buffer_id,
                width: *width,
                height: *height,
                format: dmabuf_format(*format),
                modifier: *modifier,
                planes: planes
                    .iter()
                    .map(|plane| generated::DmabufPlane {
                        fd_index: plane.fd_index,
                        offset: plane.offset,
                        stride: plane.stride,
                    })
                    .collect(),
            }),
            C::AttachShmBuffer {
                surface_id,
                buffer_id,
            } => Message::AttachShmBuffer(generated::SurfaceBuffer {
                surface_id: *surface_id,
                buffer_id: *buffer_id,
            }),
            C::AttachDmabufBuffer {
                surface_id,
                buffer_id,
            } => Message::AttachDmabufBuffer(generated::SurfaceBuffer {
                surface_id: *surface_id,
                buffer_id: *buffer_id,
            }),
            C::DetachBuffer { surface_id } => Message::DetachBuffer(sid(*surface_id)),
            C::DestroyBuffer { buffer_id } => Message::DestroyBuffer(id(*buffer_id)),
            C::DestroyShmPool { pool_id } => Message::DestroyShmPool(id(*pool_id)),
            C::Damage {
                surface_id,
                x,
                y,
                width,
                height,
            } => Message::Damage(generated::Damage {
                surface_id: *surface_id,
                x: *x,
                y: *y,
                width: *width,
                height: *height,
            }),
            C::SetInputRegion { surface_id, rects } => Message::SetInputRegion(generated::Region {
                surface_id: *surface_id,
                rects: rects.iter().copied().map(rect).collect(),
            }),
            C::SetOpaqueRegion { surface_id, rects } => {
                Message::SetOpaqueRegion(generated::Region {
                    surface_id: *surface_id,
                    rects: rects.iter().copied().map(rect).collect(),
                })
            }
            C::CreatePopup {
                surface_id,
                parent_id,
                positioner,
                grab,
            } => Message::CreatePopup(generated::CreatePopup {
                surface_id: *surface_id,
                parent_id: *parent_id,
                positioner: Some(popup_positioner(positioner)),
                grab: *grab,
            }),
            C::DestroyPopup { popup_id } => Message::DestroyPopup(id(*popup_id)),
            C::SetToplevelState { toplevel_id, state } => {
                use generated::toplevel_state::State;
                let state = match state {
                    p::ToplevelStateRequest::Maximize => State::Maximize(empty()),
                    p::ToplevelStateRequest::Minimize => State::Minimize(empty()),
                    p::ToplevelStateRequest::Fullscreen { output_id } => {
                        State::Fullscreen(generated::OptionalOutput {
                            output_id: *output_id,
                        })
                    }
                    p::ToplevelStateRequest::UnsetMaximize => State::UnsetMaximize(empty()),
                    p::ToplevelStateRequest::UnsetFullscreen => State::UnsetFullscreen(empty()),
                };
                Message::SetToplevelState(generated::SetToplevelState {
                    toplevel_id: *toplevel_id,
                    state: Some(generated::ToplevelState { state: Some(state) }),
                })
            }
            C::SetToplevelAppId {
                toplevel_id,
                app_id,
            } => Message::SetToplevelAppId(generated::SetToplevelAppId {
                toplevel_id: *toplevel_id,
                app_id: app_id.clone(),
            }),
            C::CloseToplevel { toplevel_id } => Message::CloseToplevel(id(*toplevel_id)),
            C::SetCursor {
                serial,
                surface_id,
                hotspot_x,
                hotspot_y,
            } => Message::SetCursor(generated::SetCursor {
                serial: *serial,
                surface_id: *surface_id,
                hotspot_x: *hotspot_x,
                hotspot_y: *hotspot_y,
            }),
            C::CreateLayerSurface {
                surface_id,
                capability_token,
                layer,
                namespace,
                output_id,
            } => Message::CreateLayerSurface(generated::CreateLayerSurface {
                surface_id: *surface_id,
                capability_token: capability_token.clone(),
                layer: match layer {
                    p::LayerShellLayer::Background => 1,
                    p::LayerShellLayer::Bottom => 2,
                    p::LayerShellLayer::Top => 3,
                    p::LayerShellLayer::Overlay => 4,
                },
                namespace: namespace.clone(),
                output_id: *output_id,
            }),
            C::SetLayerAnchor {
                layer_id,
                top,
                bottom,
                left,
                right,
            } => Message::SetLayerAnchor(generated::SetLayerAnchor {
                layer_id: *layer_id,
                top: *top,
                bottom: *bottom,
                left: *left,
                right: *right,
            }),
            C::SetLayerExclusiveZone { layer_id, zone } => {
                Message::SetLayerExclusiveZone(generated::SetLayerExclusiveZone {
                    layer_id: *layer_id,
                    zone: *zone,
                })
            }
            C::SetLayerMargin {
                layer_id,
                top,
                right,
                bottom,
                left,
            } => Message::SetLayerMargin(generated::SetLayerMargin {
                layer_id: *layer_id,
                top: *top,
                right: *right,
                bottom: *bottom,
                left: *left,
            }),
            C::SetLayerKeyboardInteractivity {
                layer_id,
                interactivity,
            } => Message::SetLayerKeyboardInteractivity(generated::SetLayerKeyboardInteractivity {
                layer_id: *layer_id,
                interactivity: match interactivity {
                    p::LayerKeyboardInteractivity::None => 1,
                    p::LayerKeyboardInteractivity::Exclusive => 2,
                    p::LayerKeyboardInteractivity::OnDemand => 3,
                },
            }),
            C::SetLayerSize {
                layer_id,
                width,
                height,
            } => Message::SetLayerSize(generated::SetLayerSize {
                layer_id: *layer_id,
                width: *width,
                height: *height,
            }),
            C::AckLayerConfigure { layer_id, serial } => {
                Message::AckLayerConfigure(generated::AckLayerConfigure {
                    layer_id: *layer_id,
                    serial: *serial,
                })
            }
            C::LockSession { capability_token } => Message::LockSession(generated::Token {
                capability_token: capability_token.clone(),
            }),
            C::CreateLockSurface {
                surface_id,
                lock_id,
                output_id,
            } => Message::CreateLockSurface(generated::CreateLockSurface {
                surface_id: *surface_id,
                lock_id: *lock_id,
                output_id: *output_id,
            }),
            C::AckLockConfigure {
                lock_surface_id,
                serial,
            } => Message::AckLockConfigure(generated::AckLockConfigure {
                lock_surface_id: *lock_surface_id,
                serial: *serial,
            }),
            C::UnlockSession { lock_id } => Message::UnlockSession(id(*lock_id)),
            C::AuthorizeSessionUser { lock_id, uid } => {
                Message::AuthorizeSessionUser(generated::AuthorizeSessionUser {
                    lock_id: *lock_id,
                    uid: *uid,
                })
            }
            C::RevokeSessionUser {
                uid,
                capability_token,
            } => Message::RevokeSessionUser(generated::RevokeSessionUser {
                uid: *uid,
                capability_token: capability_token.clone(),
            }),
            C::SetSelection { mime_types, serial } => {
                Message::SetSelection(generated::SetSelection {
                    mime_types: mime_types.clone(),
                    serial: *serial,
                })
            }
            C::RequestSelection { mime_type } => Message::RequestSelection(generated::MimeType {
                mime_type: mime_type.clone(),
            }),
            C::StartDrag {
                surface_id,
                origin_surface,
                icon_surface,
                mime_types,
                serial,
            } => Message::StartDrag(generated::StartDrag {
                surface_id: *surface_id,
                origin_surface: *origin_surface,
                icon_surface: *icon_surface,
                mime_types: mime_types.clone(),
                serial: *serial,
            }),
            C::ReceiveDragData { mime_type } => Message::ReceiveDragData(generated::MimeType {
                mime_type: mime_type.clone(),
            }),
            C::AcceptDrag { serial, mime_type } => Message::AcceptDrag(generated::AcceptDrag {
                serial: *serial,
                mime_type: mime_type.clone(),
            }),
            C::FinishDrag => Message::FinishDrag(empty()),
            C::CancelDrag => Message::CancelDrag(empty()),
            C::SetDragActions { actions, preferred } => {
                Message::SetDragActions(generated::SetDragActions {
                    actions: actions.iter().copied().map(drag_action).collect(),
                    preferred: preferred.map(drag_action),
                })
            }
            C::RequestCapture {
                target,
                cursor_mode: mode,
                capability_token,
            } => {
                use generated::capture_target::Target;
                let target = match target {
                    p::CaptureTarget::Window(id) => Target::Window(*id),
                    p::CaptureTarget::Output(id) => Target::Output(*id),
                    p::CaptureTarget::Workspace => Target::Workspace(empty()),
                };
                Message::RequestCapture(generated::RequestCapture {
                    target: Some(generated::CaptureTarget {
                        target: Some(target),
                    }),
                    cursor_mode: cursor_mode(*mode),
                    capability_token: capability_token.clone(),
                })
            }
            C::RegisterShortcut {
                binding,
                justification,
                capability_token,
            } => Message::RegisterShortcut(generated::RegisterShortcut {
                binding: Some(generated::KeyBinding {
                    keycode: binding.keycode,
                    modifiers: binding.modifiers,
                }),
                justification: justification.clone(),
                capability_token: capability_token.clone(),
            }),
            C::UnregisterShortcut { shortcut_id } => {
                Message::UnregisterShortcut(generated::U64Id { id: *shortcut_id })
            }
        };
        Ok(generated::ClientMessage {
            message: Some(message),
        }
        .encode_to_vec())
    }

    fn decode_wire(bytes: &[u8]) -> io::Result<Self> {
        use generated::client_message::Message;
        use p::ClientMessage as C;
        let frame =
            generated::ClientMessage::decode(bytes).map_err(|error| invalid(error.to_string()))?;
        Ok(match required(frame.message, "client_message.message")? {
            Message::Connect(v) => C::Connect {
                app_id: v.app_id,
                pid: v.pid,
            },
            Message::ConnectVersioned(v) => C::ConnectVersioned {
                app_id: v.app_id,
                pid: v.pid,
                min_version: v.min_version,
                max_version: v.max_version,
            },
            Message::CreateSurface(v) => C::CreateSurface {
                surface_id: v.surface_id,
            },
            Message::DestroySurface(v) => C::DestroySurface {
                surface_id: v.surface_id,
            },
            Message::AttachBuffer(v) => C::AttachBuffer {
                surface_id: v.surface_id,
                buffer_fd: -1,
                width: v.width,
                height: v.height,
                stride: v.stride,
                format: from_buffer_format(v.format)?,
            },
            Message::Commit(v) => C::Commit {
                surface_id: v.surface_id,
                frame_callback: v.frame_callback,
            },
            Message::RequestCapability(v) => C::RequestCapability {
                capability: v.capability,
                justification: v.justification,
            },
            Message::CreateToplevel(v) => C::CreateToplevel {
                surface_id: v.surface_id,
                capability_token: v.capability_token,
                title: v.title,
            },
            Message::SetToplevelTitle(v) => C::SetToplevelTitle {
                toplevel_id: v.toplevel_id,
                title: v.title,
            },
            Message::SetFullscreen(v) => C::SetFullscreen {
                toplevel_id: v.toplevel_id,
                capability_token: v.capability_token,
            },
            Message::AckConfigure(v) => C::AckConfigure {
                toplevel_id: v.toplevel_id,
                serial: v.serial,
            },
            Message::CreateShmPool(v) => C::CreateShmPool {
                pool_id: v.pool_id,
                fd: -1,
                size: usize::try_from(v.size)
                    .map_err(|_| invalid("shared-memory size exceeds usize"))?,
            },
            Message::CreateBuffer(v) => C::CreateBuffer {
                buffer_id: v.buffer_id,
                pool_id: v.pool_id,
                offset: usize::try_from(v.offset)
                    .map_err(|_| invalid("buffer offset exceeds usize"))?,
                width: v.width,
                height: v.height,
                stride: v.stride,
                format: from_shm_format(v.format)?,
            },
            Message::CreateDmabufBuffer(v) => C::CreateDmabufBuffer {
                buffer_id: v.buffer_id,
                width: v.width,
                height: v.height,
                format: from_dmabuf_format(v.format)?,
                modifier: v.modifier,
                planes: v
                    .planes
                    .into_iter()
                    .map(|plane| p::DmabufPlane {
                        fd_index: plane.fd_index,
                        offset: plane.offset,
                        stride: plane.stride,
                    })
                    .collect(),
                fds: Vec::new(),
            },
            Message::AttachShmBuffer(v) => C::AttachShmBuffer {
                surface_id: v.surface_id,
                buffer_id: v.buffer_id,
            },
            Message::DetachBuffer(v) => C::DetachBuffer {
                surface_id: v.surface_id,
            },
            Message::DestroyBuffer(v) => C::DestroyBuffer { buffer_id: v.id },
            Message::DestroyShmPool(v) => C::DestroyShmPool { pool_id: v.id },
            Message::AttachDmabufBuffer(v) => C::AttachDmabufBuffer {
                surface_id: v.surface_id,
                buffer_id: v.buffer_id,
            },
            Message::Damage(v) => C::Damage {
                surface_id: v.surface_id,
                x: v.x,
                y: v.y,
                width: v.width,
                height: v.height,
            },
            Message::SetInputRegion(v) => C::SetInputRegion {
                surface_id: v.surface_id,
                rects: v.rects.into_iter().map(from_rect).collect(),
            },
            Message::SetOpaqueRegion(v) => C::SetOpaqueRegion {
                surface_id: v.surface_id,
                rects: v.rects.into_iter().map(from_rect).collect(),
            },
            Message::CreatePopup(v) => C::CreatePopup {
                surface_id: v.surface_id,
                parent_id: v.parent_id,
                positioner: from_popup_positioner(required(
                    v.positioner,
                    "create_popup.positioner",
                )?)?,
                grab: v.grab,
            },
            Message::DestroyPopup(v) => C::DestroyPopup { popup_id: v.id },
            Message::SetToplevelState(v) => {
                use generated::toplevel_state::State;
                let s = required(
                    required(v.state, "set_toplevel_state.state")?.state,
                    "toplevel_state.state",
                )?;
                let state = match s {
                    State::Maximize(_) => p::ToplevelStateRequest::Maximize,
                    State::Minimize(_) => p::ToplevelStateRequest::Minimize,
                    State::Fullscreen(v) => p::ToplevelStateRequest::Fullscreen {
                        output_id: v.output_id,
                    },
                    State::UnsetMaximize(_) => p::ToplevelStateRequest::UnsetMaximize,
                    State::UnsetFullscreen(_) => p::ToplevelStateRequest::UnsetFullscreen,
                };
                C::SetToplevelState {
                    toplevel_id: v.toplevel_id,
                    state,
                }
            }
            Message::SetToplevelAppId(v) => C::SetToplevelAppId {
                toplevel_id: v.toplevel_id,
                app_id: v.app_id,
            },
            Message::CloseToplevel(v) => C::CloseToplevel { toplevel_id: v.id },
            Message::SetCursor(v) => C::SetCursor {
                serial: v.serial,
                surface_id: v.surface_id,
                hotspot_x: v.hotspot_x,
                hotspot_y: v.hotspot_y,
            },
            Message::CreateLayerSurface(v) => C::CreateLayerSurface {
                surface_id: v.surface_id,
                capability_token: v.capability_token,
                layer: match v.layer {
                    1 => p::LayerShellLayer::Background,
                    2 => p::LayerShellLayer::Bottom,
                    3 => p::LayerShellLayer::Top,
                    4 => p::LayerShellLayer::Overlay,
                    _ => return Err(invalid("invalid layer")),
                },
                namespace: v.namespace,
                output_id: v.output_id,
            },
            Message::SetLayerAnchor(v) => C::SetLayerAnchor {
                layer_id: v.layer_id,
                top: v.top,
                bottom: v.bottom,
                left: v.left,
                right: v.right,
            },
            Message::SetLayerExclusiveZone(v) => C::SetLayerExclusiveZone {
                layer_id: v.layer_id,
                zone: v.zone,
            },
            Message::SetLayerMargin(v) => C::SetLayerMargin {
                layer_id: v.layer_id,
                top: v.top,
                right: v.right,
                bottom: v.bottom,
                left: v.left,
            },
            Message::SetLayerKeyboardInteractivity(v) => C::SetLayerKeyboardInteractivity {
                layer_id: v.layer_id,
                interactivity: match v.interactivity {
                    1 => p::LayerKeyboardInteractivity::None,
                    2 => p::LayerKeyboardInteractivity::Exclusive,
                    3 => p::LayerKeyboardInteractivity::OnDemand,
                    _ => return Err(invalid("invalid keyboard interactivity")),
                },
            },
            Message::SetLayerSize(v) => C::SetLayerSize {
                layer_id: v.layer_id,
                width: v.width,
                height: v.height,
            },
            Message::AckLayerConfigure(v) => C::AckLayerConfigure {
                layer_id: v.layer_id,
                serial: v.serial,
            },
            Message::LockSession(v) => C::LockSession {
                capability_token: v.capability_token,
            },
            Message::CreateLockSurface(v) => C::CreateLockSurface {
                surface_id: v.surface_id,
                lock_id: v.lock_id,
                output_id: v.output_id,
            },
            Message::AckLockConfigure(v) => C::AckLockConfigure {
                lock_surface_id: v.lock_surface_id,
                serial: v.serial,
            },
            Message::UnlockSession(v) => C::UnlockSession { lock_id: v.id },
            Message::AuthorizeSessionUser(v) => C::AuthorizeSessionUser {
                lock_id: v.lock_id,
                uid: v.uid,
            },
            Message::RevokeSessionUser(v) => C::RevokeSessionUser {
                uid: v.uid,
                capability_token: v.capability_token,
            },
            Message::SetSelection(v) => C::SetSelection {
                mime_types: v.mime_types,
                serial: v.serial,
            },
            Message::RequestSelection(v) => C::RequestSelection {
                mime_type: v.mime_type,
            },
            Message::StartDrag(v) => C::StartDrag {
                surface_id: v.surface_id,
                origin_surface: v.origin_surface,
                icon_surface: v.icon_surface,
                mime_types: v.mime_types,
                serial: v.serial,
            },
            Message::ReceiveDragData(v) => C::ReceiveDragData {
                mime_type: v.mime_type,
            },
            Message::AcceptDrag(v) => C::AcceptDrag {
                serial: v.serial,
                mime_type: v.mime_type,
            },
            Message::FinishDrag(_) => C::FinishDrag,
            Message::CancelDrag(_) => C::CancelDrag,
            Message::SetDragActions(v) => C::SetDragActions {
                actions: v
                    .actions
                    .into_iter()
                    .map(from_drag_action)
                    .collect::<io::Result<_>>()?,
                preferred: v.preferred.map(from_drag_action).transpose()?,
            },
            Message::RequestCapture(v) => {
                use generated::capture_target::Target;
                let target = match required(
                    required(v.target, "request_capture.target")?.target,
                    "capture_target.target",
                )? {
                    Target::Window(id) => p::CaptureTarget::Window(id),
                    Target::Output(id) => p::CaptureTarget::Output(id),
                    Target::Workspace(_) => p::CaptureTarget::Workspace,
                };
                C::RequestCapture {
                    target,
                    cursor_mode: from_cursor_mode(v.cursor_mode)?,
                    capability_token: v.capability_token,
                }
            }
            Message::RegisterShortcut(v) => {
                let b = required(v.binding, "register_shortcut.binding")?;
                C::RegisterShortcut {
                    binding: p::KeyBinding {
                        keycode: b.keycode,
                        modifiers: b.modifiers,
                    },
                    justification: v.justification,
                    capability_token: v.capability_token,
                }
            }
            Message::UnregisterShortcut(v) => C::UnregisterShortcut { shortcut_id: v.id },
        })
    }
}

fn output_mode(value: &p::OutputMode) -> generated::OutputMode {
    generated::OutputMode {
        width: value.width,
        height: value.height,
        refresh_rate: value.refresh_rate,
        preferred: value.preferred,
    }
}
fn from_output_mode(value: generated::OutputMode) -> p::OutputMode {
    p::OutputMode {
        width: value.width,
        height: value.height,
        refresh_rate: value.refresh_rate,
        preferred: value.preferred,
    }
}
fn dismiss_reason(value: p::DismissReason) -> i32 {
    match value {
        p::DismissReason::OutsideClick => 1,
        p::DismissReason::ParentClosed => 2,
        p::DismissReason::EscapeKey => 3,
    }
}
fn from_dismiss_reason(value: i32) -> io::Result<p::DismissReason> {
    match value {
        1 => Ok(p::DismissReason::OutsideClick),
        2 => Ok(p::DismissReason::ParentClosed),
        3 => Ok(p::DismissReason::EscapeKey),
        _ => Err(invalid("invalid popup dismissal reason")),
    }
}
fn subpixel(value: p::SubpixelLayout) -> i32 {
    match value {
        p::SubpixelLayout::Unknown => 0,
        p::SubpixelLayout::None => 1,
        p::SubpixelLayout::HorizontalRgb => 2,
        p::SubpixelLayout::HorizontalBgr => 3,
        p::SubpixelLayout::VerticalRgb => 4,
        p::SubpixelLayout::VerticalBgr => 5,
    }
}
fn from_subpixel(value: i32) -> io::Result<p::SubpixelLayout> {
    match value {
        0 => Ok(p::SubpixelLayout::Unknown),
        1 => Ok(p::SubpixelLayout::None),
        2 => Ok(p::SubpixelLayout::HorizontalRgb),
        3 => Ok(p::SubpixelLayout::HorizontalBgr),
        4 => Ok(p::SubpixelLayout::VerticalRgb),
        5 => Ok(p::SubpixelLayout::VerticalBgr),
        _ => Err(invalid("invalid subpixel layout")),
    }
}
fn transform(value: p::Transform) -> i32 {
    match value {
        p::Transform::Normal => 1,
        p::Transform::Rotate90 => 2,
        p::Transform::Rotate180 => 3,
        p::Transform::Rotate270 => 4,
        p::Transform::Flipped => 5,
        p::Transform::Flipped90 => 6,
        p::Transform::Flipped180 => 7,
        p::Transform::Flipped270 => 8,
    }
}
fn from_transform(value: i32) -> io::Result<p::Transform> {
    match value {
        1 => Ok(p::Transform::Normal),
        2 => Ok(p::Transform::Rotate90),
        3 => Ok(p::Transform::Rotate180),
        4 => Ok(p::Transform::Rotate270),
        5 => Ok(p::Transform::Flipped),
        6 => Ok(p::Transform::Flipped90),
        7 => Ok(p::Transform::Flipped180),
        8 => Ok(p::Transform::Flipped270),
        _ => Err(invalid("invalid output transform")),
    }
}

fn input_event(value: &p::InputEvent) -> generated::InputEvent {
    use generated::input_event::Event;
    let event = match value {
        p::InputEvent::PointerEnter { serial, x, y } => {
            Event::PointerEnter(generated::PointerEnter {
                serial: *serial,
                x: *x,
                y: *y,
            })
        }
        p::InputEvent::PointerLeave { serial } => {
            Event::PointerLeave(generated::Serial { serial: *serial })
        }
        p::InputEvent::PointerMotion { x, y, time_ms } => {
            Event::PointerMotion(generated::PointerMotion {
                x: *x,
                y: *y,
                time_ms: *time_ms,
            })
        }
        p::InputEvent::PointerButton {
            serial,
            button,
            state,
            time_ms,
        } => Event::PointerButton(generated::PointerButton {
            serial: *serial,
            button: *button,
            state: match state {
                p::ButtonState::Pressed => 1,
                p::ButtonState::Released => 2,
            },
            time_ms: *time_ms,
        }),
        p::InputEvent::PointerAxis {
            time_ms,
            axis_source,
            orientation,
            value,
            discrete,
        } => Event::PointerAxis(generated::PointerAxis {
            time_ms: *time_ms,
            axis_source: match axis_source {
                p::AxisSource::Wheel => 1,
                p::AxisSource::Finger => 2,
                p::AxisSource::Continuous => 3,
                p::AxisSource::WheelTilt => 4,
            },
            orientation: match orientation {
                p::Orientation::Vertical => 1,
                p::Orientation::Horizontal => 2,
            },
            value: *value,
            discrete: *discrete,
        }),
        p::InputEvent::PointerFrame => Event::PointerFrame(empty()),
        p::InputEvent::KeyboardEnter { serial, keys } => {
            Event::KeyboardEnter(generated::KeyboardEnter {
                serial: *serial,
                keys: keys.clone(),
            })
        }
        p::InputEvent::KeyboardLeave { serial } => {
            Event::KeyboardLeave(generated::Serial { serial: *serial })
        }
        p::InputEvent::KeyboardKey {
            serial,
            key,
            state,
            time_ms,
        } => Event::KeyboardKey(generated::KeyboardKey {
            serial: *serial,
            key: *key,
            state: match state {
                p::KeyState::Pressed => 1,
                p::KeyState::Released => 2,
            },
            time_ms: *time_ms,
        }),
        p::InputEvent::TouchDown {
            serial,
            touch_id,
            x,
            y,
            time_ms,
        } => Event::TouchDown(generated::TouchDown {
            serial: *serial,
            touch_id: *touch_id,
            x: *x,
            y: *y,
            time_ms: *time_ms,
        }),
        p::InputEvent::TouchUp {
            serial,
            touch_id,
            time_ms,
        } => Event::TouchUp(generated::TouchUp {
            serial: *serial,
            touch_id: *touch_id,
            time_ms: *time_ms,
        }),
        p::InputEvent::TouchMotion {
            touch_id,
            x,
            y,
            time_ms,
        } => Event::TouchMotion(generated::TouchMotion {
            touch_id: *touch_id,
            x: *x,
            y: *y,
            time_ms: *time_ms,
        }),
        p::InputEvent::TouchCancel => Event::TouchCancel(empty()),
        p::InputEvent::TouchFrame => Event::TouchFrame(empty()),
        p::InputEvent::TouchShape {
            touch_id,
            major,
            minor,
        } => Event::TouchShape(generated::TouchShape {
            touch_id: *touch_id,
            major: *major,
            minor: *minor,
        }),
        p::InputEvent::TouchOrientation {
            touch_id,
            orientation,
        } => Event::TouchOrientation(generated::TouchOrientation {
            touch_id: *touch_id,
            orientation: *orientation,
        }),
        p::InputEvent::Modifiers {
            serial,
            mods_depressed,
            mods_latched,
            mods_locked,
            group,
        } => Event::Modifiers(generated::InputModifiers {
            serial: *serial,
            mods_depressed: *mods_depressed,
            mods_latched: *mods_latched,
            mods_locked: *mods_locked,
            group: *group,
        }),
    };
    generated::InputEvent { event: Some(event) }
}

fn from_input_event(value: generated::InputEvent) -> io::Result<p::InputEvent> {
    use generated::input_event::Event;
    Ok(match required(value.event, "input_event.event")? {
        Event::PointerEnter(v) => p::InputEvent::PointerEnter {
            serial: v.serial,
            x: v.x,
            y: v.y,
        },
        Event::PointerLeave(v) => p::InputEvent::PointerLeave { serial: v.serial },
        Event::PointerMotion(v) => p::InputEvent::PointerMotion {
            x: v.x,
            y: v.y,
            time_ms: v.time_ms,
        },
        Event::PointerButton(v) => p::InputEvent::PointerButton {
            serial: v.serial,
            button: v.button,
            state: match v.state {
                1 => p::ButtonState::Pressed,
                2 => p::ButtonState::Released,
                _ => return Err(invalid("invalid button state")),
            },
            time_ms: v.time_ms,
        },
        Event::PointerAxis(v) => p::InputEvent::PointerAxis {
            time_ms: v.time_ms,
            axis_source: match v.axis_source {
                1 => p::AxisSource::Wheel,
                2 => p::AxisSource::Finger,
                3 => p::AxisSource::Continuous,
                4 => p::AxisSource::WheelTilt,
                _ => return Err(invalid("invalid axis source")),
            },
            orientation: match v.orientation {
                1 => p::Orientation::Vertical,
                2 => p::Orientation::Horizontal,
                _ => return Err(invalid("invalid orientation")),
            },
            value: v.value,
            discrete: v.discrete,
        },
        Event::PointerFrame(_) => p::InputEvent::PointerFrame,
        Event::KeyboardEnter(v) => p::InputEvent::KeyboardEnter {
            serial: v.serial,
            keys: v.keys,
        },
        Event::KeyboardLeave(v) => p::InputEvent::KeyboardLeave { serial: v.serial },
        Event::KeyboardKey(v) => p::InputEvent::KeyboardKey {
            serial: v.serial,
            key: v.key,
            state: match v.state {
                1 => p::KeyState::Pressed,
                2 => p::KeyState::Released,
                _ => return Err(invalid("invalid key state")),
            },
            time_ms: v.time_ms,
        },
        Event::TouchDown(v) => p::InputEvent::TouchDown {
            serial: v.serial,
            touch_id: v.touch_id,
            x: v.x,
            y: v.y,
            time_ms: v.time_ms,
        },
        Event::TouchUp(v) => p::InputEvent::TouchUp {
            serial: v.serial,
            touch_id: v.touch_id,
            time_ms: v.time_ms,
        },
        Event::TouchMotion(v) => p::InputEvent::TouchMotion {
            touch_id: v.touch_id,
            x: v.x,
            y: v.y,
            time_ms: v.time_ms,
        },
        Event::TouchCancel(_) => p::InputEvent::TouchCancel,
        Event::TouchFrame(_) => p::InputEvent::TouchFrame,
        Event::TouchShape(v) => p::InputEvent::TouchShape {
            touch_id: v.touch_id,
            major: v.major,
            minor: v.minor,
        },
        Event::TouchOrientation(v) => p::InputEvent::TouchOrientation {
            touch_id: v.touch_id,
            orientation: v.orientation,
        },
        Event::Modifiers(v) => p::InputEvent::Modifiers {
            serial: v.serial,
            mods_depressed: v.mods_depressed,
            mods_latched: v.mods_latched,
            mods_locked: v.mods_locked,
            group: v.group,
        },
    })
}

impl WireMessage for p::CompositorMessage {
    fn encode_wire(&self) -> io::Result<Vec<u8>> {
        use generated::compositor_message::Message;
        use p::CompositorMessage as C;
        let message = match self {
            C::Connected {
                session_id,
                granted_capabilities,
                capability_tokens,
            } => Message::Connected(generated::Connected {
                session_id: *session_id,
                granted_capabilities: granted_capabilities.clone(),
                capability_tokens: capability_tokens
                    .iter()
                    .map(|(capability, token)| generated::CapabilityToken {
                        capability: capability.clone(),
                        token: token.clone(),
                    })
                    .collect(),
            }),
            C::ProtocolVersion { version, features } => {
                Message::ProtocolVersion(generated::ProtocolVersion {
                    version: *version,
                    features: features.clone(),
                })
            }
            C::Rejected { reason } => Message::Rejected(generated::Reason {
                reason: reason.clone(),
            }),
            C::CapabilityDecision {
                capability,
                granted,
                token,
                reason,
                needs_user_consent,
            } => Message::CapabilityDecision(generated::CapabilityDecision {
                capability: capability.clone(),
                granted: *granted,
                token: token.clone(),
                reason: reason.clone(),
                needs_user_consent: *needs_user_consent,
            }),
            C::CapabilityRevoked { capability, reason } => {
                Message::CapabilityRevoked(generated::CapabilityRevoked {
                    capability: capability.clone(),
                    reason: reason.clone(),
                })
            }
            C::ProtocolError {
                code,
                message,
                fatal,
            } => Message::ProtocolError(generated::ProtocolError {
                code: code.clone(),
                message: message.clone(),
                fatal: *fatal,
            }),
            C::ConfigureToplevel {
                toplevel_id,
                serial,
                width,
                height,
                decoration_height,
                states,
            } => Message::ConfigureToplevel(generated::ConfigureToplevel {
                toplevel_id: *toplevel_id,
                serial: *serial,
                width: *width,
                height: *height,
                decoration_height: *decoration_height,
                states: Some(generated::ToplevelStates {
                    activated: states.activated,
                    maximized: states.maximized,
                    fullscreen: states.fullscreen,
                    resizing: states.resizing,
                }),
            }),
            C::ToplevelClosed { toplevel_id } => Message::ToplevelClosed(id(*toplevel_id)),
            C::FrameCallback {
                surface_id,
                callback_id,
                timestamp_ms,
            } => Message::FrameCallback(generated::FrameCallback {
                surface_id: *surface_id,
                callback_id: *callback_id,
                timestamp_ms: *timestamp_ms,
            }),
            C::InputEvent { surface_id, event } => {
                Message::InputEvent(generated::SurfaceInputEvent {
                    surface_id: *surface_id,
                    event: Some(input_event(event)),
                })
            }
            C::OutputChanged {
                width,
                height,
                scale,
            } => Message::OutputChanged(generated::OutputChanged {
                width: *width,
                height: *height,
                scale: *scale,
            }),
            C::BufferRelease { buffer_id } => Message::BufferRelease(id(*buffer_id)),
            C::ConfigurePopup {
                popup_id,
                x,
                y,
                width,
                height,
            } => Message::ConfigurePopup(generated::ConfigurePopup {
                popup_id: *popup_id,
                x: *x,
                y: *y,
                width: *width,
                height: *height,
            }),
            C::PopupDismissed { popup_id, reason } => {
                Message::PopupDismissed(generated::PopupDismissed {
                    popup_id: *popup_id,
                    reason: dismiss_reason(*reason),
                })
            }
            C::OutputAdded {
                output_id,
                name,
                description,
                geometry,
                physical_size,
                subpixel: sp,
                transform: tr,
                scale,
                modes,
                current_mode,
            } => Message::OutputAdded(generated::OutputAdded {
                output_id: *output_id,
                name: name.clone(),
                description: description.clone(),
                geometry: Some(rect(*geometry)),
                physical_size: Some(generated::PhysicalSize {
                    width_mm: physical_size.0,
                    height_mm: physical_size.1,
                }),
                subpixel: subpixel(*sp),
                transform: transform(*tr),
                scale: *scale,
                modes: modes.iter().map(output_mode).collect(),
                current_mode: u64::try_from(*current_mode)
                    .map_err(|_| invalid("current output mode exceeds u64"))?,
            }),
            C::OutputRemoved { output_id } => Message::OutputRemoved(id(*output_id)),
            C::OutputGeometryChanged {
                output_id,
                geometry,
            } => Message::OutputGeometryChanged(generated::OutputGeometryChanged {
                output_id: *output_id,
                geometry: Some(rect(*geometry)),
            }),
            C::OutputScaleChanged { output_id, scale } => {
                Message::OutputScaleChanged(generated::OutputScaleChanged {
                    output_id: *output_id,
                    scale: *scale,
                })
            }
            C::OutputModeChanged { output_id, mode } => {
                Message::OutputModeChanged(generated::OutputModeChanged {
                    output_id: *output_id,
                    mode: Some(output_mode(mode)),
                })
            }
            C::SurfaceEnterOutput {
                surface_id,
                output_id,
            } => Message::SurfaceEnterOutput(generated::SurfaceOutput {
                surface_id: *surface_id,
                output_id: *output_id,
            }),
            C::SurfaceLeaveOutput {
                surface_id,
                output_id,
            } => Message::SurfaceLeaveOutput(generated::SurfaceOutput {
                surface_id: *surface_id,
                output_id: *output_id,
            }),
            C::KeymapFormat { format, size, .. } => Message::KeymapFormat(generated::Keymap {
                format: match format {
                    p::KeymapFormat::NoKeymap => 1,
                    p::KeymapFormat::XkbV1 => 2,
                },
                size: *size,
            }),
            C::RepeatInfo { rate, delay } => Message::RepeatInfo(generated::RepeatInfo {
                rate: *rate,
                delay: *delay,
            }),
            C::Modifiers {
                surface_id,
                serial,
                mods_depressed,
                mods_latched,
                mods_locked,
                group,
            } => Message::Modifiers(generated::Modifiers {
                surface_id: *surface_id,
                serial: *serial,
                mods_depressed: *mods_depressed,
                mods_latched: *mods_latched,
                mods_locked: *mods_locked,
                group: *group,
            }),
            C::ConfigureLayerSurface {
                layer_id,
                serial,
                width,
                height,
            } => Message::ConfigureLayerSurface(generated::ConfigureLayerSurface {
                layer_id: *layer_id,
                serial: *serial,
                width: *width,
                height: *height,
            }),
            C::LayerSurfaceClosed { layer_id } => Message::LayerSurfaceClosed(id(*layer_id)),
            C::SessionLockEngaged { lock_id } => Message::SessionLockEngaged(id(*lock_id)),
            C::SessionLocked { lock_id } => Message::SessionLocked(id(*lock_id)),
            C::SessionLockFinished { reason } => Message::SessionLockFinished(generated::Reason {
                reason: reason.clone(),
            }),
            C::ConfigureLockSurface {
                lock_surface_id,
                serial,
                width,
                height,
            } => Message::ConfigureLockSurface(generated::ConfigureLockSurface {
                lock_surface_id: *lock_surface_id,
                serial: *serial,
                width: *width,
                height: *height,
            }),
            C::SessionLockStateChanged { locked } => {
                Message::SessionLockStateChanged(generated::Locked { locked: *locked })
            }
            C::SessionUserAuthorized { uid } => {
                Message::SessionUserAuthorized(generated::User { uid: *uid })
            }
            C::SessionUserRevoked { uid } => {
                Message::SessionUserRevoked(generated::User { uid: *uid })
            }
            C::SelectionOffer { mime_types } => {
                Message::SelectionOffer(generated::SelectionOffer {
                    mime_types: mime_types.clone(),
                })
            }
            C::RequestSelectionData { mime_type, .. } => {
                Message::RequestSelectionData(generated::MimeType {
                    mime_type: mime_type.clone(),
                })
            }
            C::SelectionData { mime_type, .. } => Message::SelectionData(generated::MimeType {
                mime_type: mime_type.clone(),
            }),
            C::SelectionCleared => Message::SelectionCleared(empty()),
            C::DragEnter {
                serial,
                surface_id,
                x,
                y,
                mime_types,
            } => Message::DragEnter(generated::DragEnter {
                serial: *serial,
                surface_id: *surface_id,
                x: *x,
                y: *y,
                mime_types: mime_types.clone(),
            }),
            C::DragMotion { x, y, time_ms } => Message::DragMotion(generated::DragMotion {
                x: *x,
                y: *y,
                time_ms: *time_ms,
            }),
            C::DragLeave => Message::DragLeave(empty()),
            C::Drop => Message::Drop(empty()),
            C::RequestDragData { mime_type, .. } => Message::RequestDragData(generated::MimeType {
                mime_type: mime_type.clone(),
            }),
            C::DragData { mime_type, .. } => Message::DragData(generated::MimeType {
                mime_type: mime_type.clone(),
            }),
            C::DragFinished => Message::DragFinished(empty()),
            C::DragCancelled => Message::DragCancelled(empty()),
            C::DragActionSelected { action } => {
                Message::DragActionSelected(generated::DragActionSelected {
                    action: drag_action(*action),
                })
            }
            C::CaptureGranted {
                capture_id,
                width,
                height,
                stride,
                format,
                cursor_mode: mode,
                ..
            } => Message::CaptureGranted(generated::CaptureGranted {
                capture_id: *capture_id,
                width: *width,
                height: *height,
                stride: *stride,
                format: capture_format(*format),
                cursor_mode: cursor_mode(*mode),
            }),
            C::ShortcutGranted {
                shortcut_id,
                binding,
                priority,
            } => Message::ShortcutGranted(generated::ShortcutGranted {
                shortcut_id: *shortcut_id,
                binding: Some(generated::KeyBinding {
                    keycode: binding.keycode,
                    modifiers: binding.modifiers,
                }),
                priority: shortcut_priority(*priority),
            }),
            C::ShortcutRevoked {
                shortcut_id,
                reason,
            } => Message::ShortcutRevoked(generated::ShortcutRevoked {
                shortcut_id: *shortcut_id,
                reason: reason.clone(),
            }),
            C::ShortcutActivated {
                shortcut_id,
                timestamp_ms,
            } => Message::ShortcutActivated(generated::ShortcutActivated {
                shortcut_id: *shortcut_id,
                timestamp_ms: *timestamp_ms,
            }),
        };
        Ok(generated::CompositorMessage {
            message: Some(message),
        }
        .encode_to_vec())
    }

    fn decode_wire(bytes: &[u8]) -> io::Result<Self> {
        use generated::compositor_message::Message;
        use p::CompositorMessage as C;
        let frame = generated::CompositorMessage::decode(bytes)
            .map_err(|error| invalid(error.to_string()))?;
        Ok(
            match required(frame.message, "compositor_message.message")? {
                Message::Connected(v) => C::Connected {
                    session_id: v.session_id,
                    granted_capabilities: v.granted_capabilities,
                    capability_tokens: v
                        .capability_tokens
                        .into_iter()
                        .map(|entry| (entry.capability, entry.token))
                        .collect::<HashMap<_, _>>(),
                },
                Message::ProtocolVersion(v) => C::ProtocolVersion {
                    version: v.version,
                    features: v.features,
                },
                Message::Rejected(v) => C::Rejected { reason: v.reason },
                Message::CapabilityDecision(v) => C::CapabilityDecision {
                    capability: v.capability,
                    granted: v.granted,
                    token: v.token,
                    reason: v.reason,
                    needs_user_consent: v.needs_user_consent,
                },
                Message::CapabilityRevoked(v) => C::CapabilityRevoked {
                    capability: v.capability,
                    reason: v.reason,
                },
                Message::ProtocolError(v) => C::ProtocolError {
                    code: v.code,
                    message: v.message,
                    fatal: v.fatal,
                },
                Message::ConfigureToplevel(v) => {
                    let s = required(v.states, "configure_toplevel.states")?;
                    C::ConfigureToplevel {
                        toplevel_id: v.toplevel_id,
                        serial: v.serial,
                        width: v.width,
                        height: v.height,
                        decoration_height: v.decoration_height,
                        states: p::ToplevelStates {
                            activated: s.activated,
                            maximized: s.maximized,
                            fullscreen: s.fullscreen,
                            resizing: s.resizing,
                        },
                    }
                }
                Message::ToplevelClosed(v) => C::ToplevelClosed { toplevel_id: v.id },
                Message::FrameCallback(v) => C::FrameCallback {
                    surface_id: v.surface_id,
                    callback_id: v.callback_id,
                    timestamp_ms: v.timestamp_ms,
                },
                Message::InputEvent(v) => C::InputEvent {
                    surface_id: v.surface_id,
                    event: from_input_event(required(v.event, "surface_input_event.event")?)?,
                },
                Message::OutputChanged(v) => C::OutputChanged {
                    width: v.width,
                    height: v.height,
                    scale: v.scale,
                },
                Message::BufferRelease(v) => C::BufferRelease { buffer_id: v.id },
                Message::ConfigurePopup(v) => C::ConfigurePopup {
                    popup_id: v.popup_id,
                    x: v.x,
                    y: v.y,
                    width: v.width,
                    height: v.height,
                },
                Message::PopupDismissed(v) => C::PopupDismissed {
                    popup_id: v.popup_id,
                    reason: from_dismiss_reason(v.reason)?,
                },
                Message::OutputAdded(v) => {
                    let physical = required(v.physical_size, "output_added.physical_size")?;
                    C::OutputAdded {
                        output_id: v.output_id,
                        name: v.name,
                        description: v.description,
                        geometry: from_rect(required(v.geometry, "output_added.geometry")?),
                        physical_size: (physical.width_mm, physical.height_mm),
                        subpixel: from_subpixel(v.subpixel)?,
                        transform: from_transform(v.transform)?,
                        scale: v.scale,
                        modes: v.modes.into_iter().map(from_output_mode).collect(),
                        current_mode: usize::try_from(v.current_mode)
                            .map_err(|_| invalid("current output mode exceeds usize"))?,
                    }
                }
                Message::OutputRemoved(v) => C::OutputRemoved { output_id: v.id },
                Message::OutputGeometryChanged(v) => C::OutputGeometryChanged {
                    output_id: v.output_id,
                    geometry: from_rect(required(v.geometry, "output_geometry_changed.geometry")?),
                },
                Message::OutputScaleChanged(v) => C::OutputScaleChanged {
                    output_id: v.output_id,
                    scale: v.scale,
                },
                Message::OutputModeChanged(v) => C::OutputModeChanged {
                    output_id: v.output_id,
                    mode: from_output_mode(required(v.mode, "output_mode_changed.mode")?),
                },
                Message::SurfaceEnterOutput(v) => C::SurfaceEnterOutput {
                    surface_id: v.surface_id,
                    output_id: v.output_id,
                },
                Message::SurfaceLeaveOutput(v) => C::SurfaceLeaveOutput {
                    surface_id: v.surface_id,
                    output_id: v.output_id,
                },
                Message::KeymapFormat(v) => C::KeymapFormat {
                    format: match v.format {
                        1 => p::KeymapFormat::NoKeymap,
                        2 => p::KeymapFormat::XkbV1,
                        _ => return Err(invalid("invalid keymap format")),
                    },
                    fd: -1,
                    size: v.size,
                },
                Message::RepeatInfo(v) => C::RepeatInfo {
                    rate: v.rate,
                    delay: v.delay,
                },
                Message::Modifiers(v) => C::Modifiers {
                    surface_id: v.surface_id,
                    serial: v.serial,
                    mods_depressed: v.mods_depressed,
                    mods_latched: v.mods_latched,
                    mods_locked: v.mods_locked,
                    group: v.group,
                },
                Message::ConfigureLayerSurface(v) => C::ConfigureLayerSurface {
                    layer_id: v.layer_id,
                    serial: v.serial,
                    width: v.width,
                    height: v.height,
                },
                Message::LayerSurfaceClosed(v) => C::LayerSurfaceClosed { layer_id: v.id },
                Message::SessionLockEngaged(v) => C::SessionLockEngaged { lock_id: v.id },
                Message::SessionLocked(v) => C::SessionLocked { lock_id: v.id },
                Message::SessionLockFinished(v) => C::SessionLockFinished { reason: v.reason },
                Message::ConfigureLockSurface(v) => C::ConfigureLockSurface {
                    lock_surface_id: v.lock_surface_id,
                    serial: v.serial,
                    width: v.width,
                    height: v.height,
                },
                Message::SessionLockStateChanged(v) => {
                    C::SessionLockStateChanged { locked: v.locked }
                }
                Message::SessionUserAuthorized(v) => C::SessionUserAuthorized { uid: v.uid },
                Message::SessionUserRevoked(v) => C::SessionUserRevoked { uid: v.uid },
                Message::SelectionOffer(v) => C::SelectionOffer {
                    mime_types: v.mime_types,
                },
                Message::RequestSelectionData(v) => C::RequestSelectionData {
                    mime_type: v.mime_type,
                    fd: -1,
                },
                Message::SelectionData(v) => C::SelectionData {
                    mime_type: v.mime_type,
                    fd: -1,
                },
                Message::SelectionCleared(_) => C::SelectionCleared,
                Message::DragEnter(v) => C::DragEnter {
                    serial: v.serial,
                    surface_id: v.surface_id,
                    x: v.x,
                    y: v.y,
                    mime_types: v.mime_types,
                },
                Message::DragMotion(v) => C::DragMotion {
                    x: v.x,
                    y: v.y,
                    time_ms: v.time_ms,
                },
                Message::DragLeave(_) => C::DragLeave,
                Message::Drop(_) => C::Drop,
                Message::RequestDragData(v) => C::RequestDragData {
                    mime_type: v.mime_type,
                    fd: -1,
                },
                Message::DragData(v) => C::DragData {
                    mime_type: v.mime_type,
                    fd: -1,
                },
                Message::DragFinished(_) => C::DragFinished,
                Message::DragCancelled(_) => C::DragCancelled,
                Message::DragActionSelected(v) => C::DragActionSelected {
                    action: from_drag_action(v.action)?,
                },
                Message::CaptureGranted(v) => C::CaptureGranted {
                    capture_id: v.capture_id,
                    width: v.width,
                    height: v.height,
                    stride: v.stride,
                    format: from_capture_format(v.format)?,
                    cursor_mode: from_cursor_mode(v.cursor_mode)?,
                    fd: -1,
                },
                Message::ShortcutGranted(v) => {
                    let b = required(v.binding, "shortcut_granted.binding")?;
                    C::ShortcutGranted {
                        shortcut_id: v.shortcut_id,
                        binding: p::KeyBinding {
                            keycode: b.keycode,
                            modifiers: b.modifiers,
                        },
                        priority: from_shortcut_priority(v.priority)?,
                    }
                }
                Message::ShortcutRevoked(v) => C::ShortcutRevoked {
                    shortcut_id: v.shortcut_id,
                    reason: v.reason,
                },
                Message::ShortcutActivated(v) => C::ShortcutActivated {
                    shortcut_id: v.shortcut_id,
                    timestamp_ms: v.timestamp_ms,
                },
            },
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn protobuf_is_binary_and_round_trips_handshake() {
        let message = p::ClientMessage::ConnectVersioned {
            app_id: "org.sol.test".into(),
            pid: 42,
            min_version: 1,
            max_version: 2,
        };
        let encoded = message.encode_wire().unwrap();
        assert_ne!(encoded.first(), Some(&b'{'));
        assert!(matches!(
            p::ClientMessage::decode_wire(&encoded).unwrap(),
            p::ClientMessage::ConnectVersioned { pid: 42, .. }
        ));
    }

    #[test]
    fn descriptor_numbers_never_enter_protobuf() {
        let message = p::CompositorMessage::CaptureGranted {
            capture_id: 7,
            width: 10,
            height: 8,
            stride: 40,
            format: p::CaptureFormat::Rgba8888,
            cursor_mode: p::CursorMode::Exclude,
            fd: 123_456,
        };
        let encoded = message.encode_wire().unwrap();
        let decoded = p::CompositorMessage::decode_wire(&encoded).unwrap();
        assert!(matches!(
            decoded,
            p::CompositorMessage::CaptureGranted { fd: -1, .. }
        ));
    }
}
