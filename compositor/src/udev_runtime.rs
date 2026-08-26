//! Real TTY backend: libseat session ownership, libinput, DRM/GBM/EGL and KMS.
//!
//! Device files are deliberately never opened with `std::fs`: libseat owns
//! every DRM and input fd so logind/seatd can revoke and re-enable them during
//! VT switches.  `DrmOutput` owns the KMS swapchain and page-flip lifecycle.

use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

use smithay::{
    backend::{
        allocator::{
            Fourcc,
            format::FormatSet,
            gbm::{GbmAllocator, GbmBufferFlags, GbmDevice},
        },
        drm::{
            DrmDevice, DrmDeviceFd, DrmEvent, DrmNode,
            compositor::FrameFlags,
            exporter::gbm::GbmFramebufferExporter,
            output::{DrmOutput, DrmOutputManager, DrmOutputRenderElements},
        },
        egl::{EGLDevice, EGLDisplay, context::ContextPriority},
        input::{
            AbsolutePositionEvent, Axis, AxisSource, Event, InputBackend, InputEvent, KeyState,
            KeyboardKeyEvent, PointerAxisEvent, PointerButtonEvent, PointerMotionEvent,
        },
        libinput::{LibinputInputBackend, LibinputSessionInterface},
        renderer::{
            ImportAll, ImportMem, ImportMemWl,
            element::{
                Kind, render_elements,
                solid::{SolidColorBuffer, SolidColorRenderElement},
                surface::{WaylandSurfaceRenderElement, render_elements_from_surface_tree},
            },
            gles::GlesRenderer,
            multigpu::{GpuManager, MultiRenderer, gbm::GbmGlesBackend},
        },
        session::{
            Event as SessionEvent, Session,
            libseat::{LibSeatSession, LibSeatSessionNotifier},
        },
        udev::{UdevBackend, UdevEvent},
    },
    input::{
        keyboard::{FilterResult, Keysym, keysyms},
        pointer::{AxisFrame, ButtonEvent, CursorImageAttributes, CursorImageStatus, MotionEvent},
    },
    output::Output,
    reexports::{
        calloop::{EventLoop, LoopHandle, RegistrationToken},
        drm::control::{ModeTypeFlags, connector, crtc},
        input::Libinput,
        rustix::fs::OFlags,
        wayland_server::{Display, DisplayHandle, ListeningSocket},
    },
    utils::{DeviceFd, Logical, Point, Serial, Size},
    wayland::shell::xdg::ToplevelSurface,
};
use smithay_drm_extras::drm_scanner::{DrmScanEvent, DrmScanner};
use sol_scheduler::FrameWatchdog;

use crate::{
    CLEAR_BACKGROUND, accept_clients,
    state::SolState,
    udev_output::{OutputTopology, SysfsDrmConnectorProbe},
};

const COLOR_FORMATS: &[Fourcc] = &[Fourcc::Abgr8888, Fourcc::Argb8888];

fn output_name(card_name: &str, connector_name: &str) -> String {
    format!("{card_name}-{connector_name}")
}

type RendererBackend = GbmGlesBackend<GlesRenderer, DrmDeviceFd>;
type UdevRenderer<'a> = MultiRenderer<'a, 'a, RendererBackend, RendererBackend>;
render_elements! {
    UdevRenderElement<R> where R: ImportAll + ImportMem;
    Surface=WaylandSurfaceRenderElement<R>,
    Cursor=SolidColorRenderElement,
}
type SolDrmOutput =
    DrmOutput<GbmAllocator<DrmDeviceFd>, GbmFramebufferExporter<DrmDeviceFd>, (), DrmDeviceFd>;
type SolDrmOutputManager = DrmOutputManager<
    GbmAllocator<DrmDeviceFd>,
    GbmFramebufferExporter<DrmDeviceFd>,
    (),
    DrmDeviceFd,
>;

struct OutputSurface {
    output: Output,
    drm_output: SolDrmOutput,
    pending_page_flip: bool,
}

struct DeviceBackend {
    card_name: String,
    render_node: DrmNode,
    manager: SolDrmOutputManager,
    scanner: DrmScanner,
    surfaces: HashMap<crtc::Handle, OutputSurface>,
    notifier_token: RegistrationToken,
}

struct UdevRuntime {
    handle: LoopHandle<'static, UdevRuntime>,
    display: Display<SolState>,
    display_handle: DisplayHandle,
    listener: ListeningSocket,
    state: SolState,
    session: LibSeatSession,
    gpus: GpuManager<RendererBackend>,
    devices: HashMap<DrmNode, DeviceBackend>,
    known_drm_paths: Vec<PathBuf>,
    topology: OutputTopology,
    probe: SysfsDrmConnectorProbe,
    session_active: bool,
    libinput: Option<Libinput>,
    suppressed_keys: Vec<Keysym>,
    fallback_cursor: SolidColorBuffer,
    serial: u32,
    started_at: Instant,
    frame_watchdog: FrameWatchdog,
    pending_input_at: Option<Instant>,
}

pub fn run(spawn: Option<String>) -> Result<(), Box<dyn std::error::Error>> {
    let mut event_loop: EventLoop<UdevRuntime> = EventLoop::try_new()?;
    let handle = event_loop.handle();
    let display: Display<SolState> = Display::new()?;
    let display_handle = display.handle();

    let (session, notifier) = LibSeatSession::new()?;
    let seat_name = session.seat();
    let udev = UdevBackend::new(&seat_name)?;

    let probe = SysfsDrmConnectorProbe::new();
    let mut topology = OutputTopology::default();
    topology.reconcile(probe.connected()?);
    let initial_outputs = topology.configurations();
    if initial_outputs.is_empty() {
        return Err("no connected DRM connector with a usable mode".into());
    }

    let state = SolState::with_output_configurations(&display_handle, Some(&initial_outputs));
    let socket_name = std::env::var("SOL_WAYLAND_SOCKET").unwrap_or_else(|_| "wayland-sol".into());
    let listener = ListeningSocket::bind(&socket_name)?;
    let gpus = GpuManager::new(GbmGlesBackend::with_context_priority(ContextPriority::High))?;

    let session_active = session.is_active();
    let mut frame_watchdog = FrameWatchdog::for_refresh_millihz(60_000);
    if let Err(error) = frame_watchdog.enable_realtime(2) {
        tracing::warn!(%error, "SCHED_FIFO priority 2 unavailable; compositor remains on CFS");
    } else {
        tracing::info!("compositor render/present event loop elevated to SCHED_FIFO priority 2");
    }

    let mut runtime = UdevRuntime {
        handle,
        display,
        display_handle,
        listener,
        state,
        session,
        gpus,
        devices: HashMap::new(),
        known_drm_paths: Vec::new(),
        topology,
        probe,
        session_active,
        libinput: None,
        suppressed_keys: Vec::new(),
        fallback_cursor: SolidColorBuffer::new((12, 18), [0.95, 0.95, 0.97, 1.0]),
        serial: 1,
        started_at: Instant::now(),
        frame_watchdog,
        pending_input_at: None,
    };

    let drm_devices = udev
        .device_list()
        .map(|(_, path)| path.to_path_buf())
        .collect::<Vec<_>>();
    runtime.known_drm_paths = drm_devices.clone();
    if runtime.session_active {
        for path in &drm_devices {
            if let Err(error) = runtime.add_device(path) {
                tracing::error!(device = ?path, %error, "failed to initialize DRM device");
            }
        }
    }
    if runtime.devices.is_empty() && runtime.session_active {
        return Err("libseat did not yield a usable DRM/GBM device".into());
    } else if !runtime.session_active {
        tracing::info!("libseat session is initially inactive; waiting for activation");
    }
    runtime.update_shm_formats();

    install_session_source(&event_loop, notifier)?;
    runtime.libinput = Some(install_input_source(
        &event_loop,
        &seat_name,
        runtime.session.clone(),
    )?);
    install_udev_source(&event_loop, udev)?;

    tracing::info!(
        socket = %socket_name,
        seat = %seat_name,
        devices = ?drm_devices,
        outputs = ?initial_outputs,
        "SOL DRM/GBM TTY backend listening"
    );
    super::spawn_client(&spawn);

    loop {
        event_loop.dispatch(Some(Duration::from_millis(8)), &mut runtime)?;
        runtime.tick()?;
    }
}

fn install_session_source(
    event_loop: &EventLoop<UdevRuntime>,
    notifier: LibSeatSessionNotifier,
) -> Result<(), Box<dyn std::error::Error>> {
    event_loop
        .handle()
        .insert_source(notifier, |event, _, runtime| match event {
            SessionEvent::PauseSession => runtime.pause_session(),
            SessionEvent::ActivateSession => runtime.resume_session(),
        })?;
    Ok(())
}

fn install_input_source(
    event_loop: &EventLoop<UdevRuntime>,
    seat_name: &str,
    session: LibSeatSession,
) -> Result<Libinput, Box<dyn std::error::Error>> {
    let mut context =
        Libinput::new_with_udev::<LibinputSessionInterface<LibSeatSession>>(session.into());
    context
        .udev_assign_seat(seat_name)
        .map_err(|()| format!("libinput could not assign seat {seat_name}"))?;
    let backend = LibinputInputBackend::new(context.clone());
    event_loop
        .handle()
        .insert_source(backend, |event, _, runtime| runtime.process_input(event))?;

    Ok(context)
}

fn install_udev_source(
    event_loop: &EventLoop<UdevRuntime>,
    udev: UdevBackend,
) -> Result<(), Box<dyn std::error::Error>> {
    event_loop
        .handle()
        .insert_source(udev, |event, _, runtime| match event {
            UdevEvent::Added { path, .. } => {
                runtime.refresh_topology();
                if !runtime.known_drm_paths.contains(&path) {
                    runtime.known_drm_paths.push(path.clone());
                }
                if runtime.session_active
                    && let Err(error) = runtime.add_device(&path)
                {
                    tracing::error!(device = ?path, %error, "failed to add hotplugged DRM device");
                }
            }
            UdevEvent::Changed { device_id } => {
                runtime.refresh_topology();
                if let Ok(node) = DrmNode::from_dev_id(device_id) {
                    runtime.scan_device(node);
                }
            }
            UdevEvent::Removed { device_id } => {
                if let Ok(node) = DrmNode::from_dev_id(device_id) {
                    runtime.remove_device(node);
                }
                runtime.known_drm_paths.retain(|path| {
                    DrmNode::from_path(path)
                        .map(|node| node.dev_id() != device_id)
                        .unwrap_or(false)
                });
                runtime.refresh_topology();
            }
        })?;
    Ok(())
}

impl UdevRuntime {
    fn add_device(&mut self, path: &Path) -> Result<(), Box<dyn std::error::Error>> {
        let node = DrmNode::from_path(path)?;
        if self.devices.contains_key(&node) {
            return Ok(());
        }
        let card_name = path
            .file_name()
            .and_then(|name| name.to_str())
            .filter(|name| name.starts_with("card"))
            .ok_or_else(|| format!("DRM primary node has no card name: {path:?}"))?
            .to_owned();

        let fd = self.session.open(
            path,
            OFlags::RDWR | OFlags::CLOEXEC | OFlags::NOCTTY | OFlags::NONBLOCK,
        )?;
        let fd = DrmDeviceFd::new(DeviceFd::from(fd));
        let (drm, notifier) = DrmDevice::new(fd.clone(), self.session_active)?;
        let gbm = GbmDevice::new(fd)?;

        let egl_display = unsafe { EGLDisplay::new(gbm.clone())? };
        let egl_device = EGLDevice::device_for_display(&egl_display)?;
        if egl_device.is_software() {
            return Err(format!("{path:?} only exposes a software EGL device").into());
        }
        let render_node = egl_device.try_get_render_node()?.unwrap_or(node);
        self.gpus.as_mut().add_node(render_node, gbm.clone())?;

        let mut renderer = self.gpus.single_renderer(&render_node)?;
        let render_formats = renderer
            .as_mut()
            .egl_context()
            .dmabuf_render_formats()
            .iter()
            .copied()
            .collect::<FormatSet>();
        drop(renderer);

        let allocator = GbmAllocator::new(
            gbm.clone(),
            GbmBufferFlags::RENDERING | GbmBufferFlags::SCANOUT,
        );
        let exporter = GbmFramebufferExporter::new(gbm.clone(), Some(render_node));
        let manager = DrmOutputManager::new(
            drm,
            allocator,
            exporter,
            Some(gbm),
            COLOR_FORMATS.iter().copied(),
            render_formats,
        );
        let notifier_token = self
            .handle
            .insert_source(notifier, move |event, _, runtime| match event {
                DrmEvent::VBlank(crtc) => runtime.page_flip(node, crtc),
                DrmEvent::Error(error) => runtime.drm_error(node, error),
            })?;

        self.devices.insert(
            node,
            DeviceBackend {
                card_name,
                render_node,
                manager,
                scanner: DrmScanner::new(),
                surfaces: HashMap::new(),
                notifier_token,
            },
        );
        tracing::info!(
            ?node,
            ?render_node,
            ?path,
            "acquired DRM master-capable device through libseat"
        );
        if self.session_active {
            self.scan_device(node);
        }
        Ok(())
    }

    fn remove_device(&mut self, node: DrmNode) {
        if let Some(device) = self.devices.remove(&node) {
            let render_node = device.render_node;
            self.handle.remove(device.notifier_token);
            drop(device);
            self.gpus.as_mut().remove_node(&render_node);
            tracing::info!(?node, "released removed DRM device");
        }
    }

    fn scan_device(&mut self, node: DrmNode) {
        let events = {
            let Some(device) = self.devices.get_mut(&node) else {
                return;
            };
            match device.scanner.scan_connectors(device.manager.device()) {
                Ok(events) => events.into_iter().collect::<Vec<_>>(),
                Err(error) => {
                    tracing::warn!(?node, %error, "DRM connector scan failed");
                    return;
                }
            }
        };
        for event in events {
            match event {
                DrmScanEvent::Connected {
                    connector,
                    crtc: Some(crtc),
                } => {
                    self.connect_output(node, connector, crtc);
                }
                DrmScanEvent::Disconnected {
                    crtc: Some(crtc), ..
                } => {
                    if let Some(device) = self.devices.get_mut(&node) {
                        device.surfaces.remove(&crtc);
                    }
                }
                _ => {}
            }
        }
    }

    fn connect_output(&mut self, node: DrmNode, connector: connector::Info, crtc: crtc::Handle) {
        let connector_name = format!(
            "{}-{}",
            connector.interface().as_str(),
            connector.interface_id()
        );
        let Some(card_name) = self
            .devices
            .get(&node)
            .map(|device| device.card_name.as_str())
        else {
            return;
        };
        let output_name = output_name(card_name, &connector_name);
        let Some(output) = self
            .state
            .outputs
            .outputs
            .iter()
            .find(|output| output.name() == output_name)
            .cloned()
        else {
            tracing::warn!(%output_name, "connector has no reconciled Wayland output");
            return;
        };
        let Some(mode) = connector
            .modes()
            .iter()
            .copied()
            .find(|mode| mode.mode_type().contains(ModeTypeFlags::PREFERRED))
            .or_else(|| connector.modes().first().copied())
        else {
            return;
        };
        let wayland_mode = smithay::output::Mode::from(mode);
        output.change_current_state(Some(wayland_mode), None, None, None);
        output.set_preferred(wayland_mode);

        let Some(device) = self.devices.get_mut(&node) else {
            return;
        };
        if device.surfaces.contains_key(&crtc) {
            return;
        }
        let mut renderer = match self.gpus.single_renderer(&device.render_node) {
            Ok(renderer) => renderer,
            Err(error) => {
                tracing::error!(?node, %error, "renderer unavailable for connector");
                return;
            }
        };
        let planes = match device.manager.device().planes(&crtc) {
            Ok(planes) => planes,
            Err(error) => {
                tracing::error!(?node, ?crtc, %error, "failed to enumerate DRM planes");
                return;
            }
        };
        let empty = DrmOutputRenderElements::<
            UdevRenderer<'_>,
            UdevRenderElement<UdevRenderer<'_>>,
        >::default();
        match device.manager.initialize_output(
            crtc,
            mode,
            &[connector.handle()],
            &output,
            Some(planes),
            &mut renderer,
            &empty,
        ) {
            Ok(drm_output) => {
                device.surfaces.insert(
                    crtc,
                    OutputSurface {
                        output,
                        drm_output,
                        pending_page_flip: false,
                    },
                );
                tracing::info!(?node, ?crtc, %connector_name, "KMS output initialized");
            }
            Err(error) => tracing::error!(?node, ?crtc, %error, "failed to initialize KMS output"),
        }
    }

    fn page_flip(&mut self, node: DrmNode, crtc: crtc::Handle) {
        let Some(surface) = self
            .devices
            .get_mut(&node)
            .and_then(|d| d.surfaces.get_mut(&crtc))
        else {
            return;
        };
        surface.pending_page_flip = false;
        if let Err(error) = surface.drm_output.frame_submitted() {
            tracing::warn!(?node, ?crtc, %error, "failed to complete KMS page flip");
        }
    }

    fn drm_error(&mut self, node: DrmNode, error: smithay::backend::drm::DrmError) {
        if let Some(device) = self.devices.get_mut(&node) {
            for surface in device.surfaces.values_mut() {
                // A failed page flip has no completion event. Clear the fence
                // so the next tick can rebuild and submit a replacement frame.
                surface.pending_page_flip = false;
            }
        }
        tracing::error!(?node, %error, "DRM event error");
    }

    fn pause_session(&mut self) {
        self.session_active = false;
        self.release_all_keys();
        if let Some(context) = self.libinput.as_mut() {
            context.suspend();
        }
        for device in self.devices.values_mut() {
            device.manager.pause();
            for surface in device.surfaces.values_mut() {
                surface.pending_page_flip = false;
            }
        }
        tracing::info!("session paused; libseat revoked input and DRM access");
    }

    fn resume_session(&mut self) {
        self.session_active = true;
        if let Some(context) = self.libinput.as_mut()
            && let Err(error) = context.resume()
        {
            tracing::error!(?error, "failed to reacquire libinput devices");
        }
        for device in self.devices.values_mut() {
            if let Err(error) = device.manager.activate(false) {
                tracing::error!(%error, "failed to reacquire DRM device after session resume");
            }
        }
        let paths = self.known_drm_paths.clone();
        for path in paths {
            if let Err(error) = self.add_device(&path) {
                tracing::error!(device = ?path, %error, "failed to reacquire DRM device");
            }
        }
        self.refresh_topology();
        let nodes = self.devices.keys().copied().collect::<Vec<_>>();
        for node in nodes {
            self.scan_device(node);
        }
        tracing::info!("session resumed; DRM devices reactivated and outputs rescanned");
    }

    fn refresh_topology(&mut self) {
        match self.probe.connected() {
            Ok(connectors) => {
                let changes = self.topology.reconcile(connectors);
                let configurations = self.topology.configurations();
                if !configurations.is_empty() {
                    self.state
                        .reconcile_outputs(&configurations, &self.display_handle);
                }
                tracing::info!(?changes, "reconciled DRM hotplug topology");
            }
            Err(error) => tracing::warn!(%error, "failed to read DRM connector topology"),
        }
    }

    fn update_shm_formats(&mut self) {
        let Some(render_node) = self.devices.values().next().map(|d| d.render_node) else {
            return;
        };
        if let Ok(renderer) = self.gpus.single_renderer(&render_node) {
            self.state.shm_state.update_formats(renderer.shm_formats());
        }
    }

    fn tick(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        accept_clients(&mut self.display_handle, &self.listener)?;
        self.display.dispatch_clients(&mut self.state)?;
        self.display.flush_clients()?;

        if self.session_active {
            let surfaces = self
                .devices
                .iter()
                .flat_map(|(node, device)| device.surfaces.keys().map(move |crtc| (*node, *crtc)))
                .collect::<Vec<_>>();
            for (node, crtc) in surfaces {
                self.render_output(node, crtc);
            }
        }
        Ok(())
    }

    fn render_output(&mut self, node: DrmNode, crtc: crtc::Handle) {
        let Some(device) = self.devices.get(&node) else {
            return;
        };
        let Some(surface) = device.surfaces.get(&crtc) else {
            return;
        };
        if surface.pending_page_flip {
            return;
        }
        let frame_started = Instant::now();
        let output = surface.output.clone();
        let render_node = device.render_node;
        let output_location = output.current_location();
        let scale = output.current_scale().fractional_scale();
        let windows = self
            .state
            .window_manager
            .toplevel_surfaces()
            .cloned()
            .map(|toplevel| {
                let location = self
                    .state
                    .window_manager
                    .surface_geometry(toplevel.wl_surface())
                    .map(|geometry| geometry.loc - output_location)
                    .unwrap_or_default();
                (toplevel, location)
            })
            .collect::<Vec<(ToplevelSurface, Point<i32, Logical>)>>();

        let mut renderer = match self.gpus.single_renderer(&render_node) {
            Ok(renderer) => renderer,
            Err(error) => {
                tracing::warn!(?node, %error, "lost GBM/EGL renderer");
                return;
            }
        };
        let pointer_location = self.state.pointer.current_location() - output_location.to_f64();
        let pointer_position = (
            pointer_location.x.round() as i32,
            pointer_location.y.round() as i32,
        );
        let mut elements = Vec::<UdevRenderElement<UdevRenderer<'_>>>::new();
        let mut cursor_surface = None;
        match &self.state.cursor_image {
            CursorImageStatus::Surface(cursor) => {
                let hotspot = smithay::wayland::compositor::with_states(cursor, |states| {
                    states
                        .data_map
                        .get::<std::sync::Mutex<CursorImageAttributes>>()
                        .map(|attributes| attributes.lock().unwrap().hotspot)
                        .unwrap_or_default()
                });
                let cursor_position = (
                    pointer_position.0 - hotspot.x,
                    pointer_position.1 - hotspot.y,
                );
                elements.extend(
                    render_elements_from_surface_tree(
                        &mut renderer,
                        cursor,
                        cursor_position,
                        scale,
                        1.0,
                        Kind::Cursor,
                    )
                    .into_iter()
                    .map(UdevRenderElement::Surface),
                );
                cursor_surface = Some(cursor.clone());
            }
            CursorImageStatus::Named(_) => {
                elements.push(UdevRenderElement::Cursor(
                    SolidColorRenderElement::from_buffer(
                        &self.fallback_cursor,
                        pointer_position,
                        scale,
                        1.0,
                        Kind::Cursor,
                    ),
                ));
            }
            CursorImageStatus::Hidden => {}
        }
        elements.extend(windows.iter().flat_map(|(surface, location)| {
            render_elements_from_surface_tree(
                &mut renderer,
                surface.wl_surface(),
                (location.x, location.y),
                scale,
                1.0,
                Kind::Unspecified,
            )
            .into_iter()
            .map(UdevRenderElement::Surface)
        }));

        let Some(surface) = self
            .devices
            .get_mut(&node)
            .and_then(|d| d.surfaces.get_mut(&crtc))
        else {
            return;
        };
        let presented = match surface.drm_output.render_frame(
            &mut renderer,
            &elements,
            CLEAR_BACKGROUND,
            FrameFlags::empty(),
        ) {
            Ok(frame) if !frame.is_empty => {
                if let Err(error) = surface.drm_output.queue_frame(()) {
                    tracing::warn!(?node, ?crtc, %error, "KMS framebuffer submission failed");
                    false
                } else {
                    surface.pending_page_flip = true;
                    for (toplevel, _) in &windows {
                        super::send_frames_surface_tree(
                            toplevel.wl_surface(),
                            self.started_at.elapsed().as_millis() as u32,
                        );
                    }
                    if let Some(cursor) = cursor_surface.as_ref() {
                        super::send_frames_surface_tree(
                            cursor,
                            self.started_at.elapsed().as_millis() as u32,
                        );
                    }
                    true
                }
            }
            Ok(_) => false,
            Err(error) => {
                tracing::warn!(?node, ?crtc, %error, "GBM/EGL frame render failed");
                false
            }
        };
        if presented {
            if let Some(input_at) = self.pending_input_at.take() {
                self.frame_watchdog.note_input_age(input_at.elapsed());
            }
            match self.frame_watchdog.observe(frame_started.elapsed()) {
                Ok(observation) if observation.watchdog_downgraded => tracing::error!(
                    ?node,
                    ?crtc,
                    frame_time_us = frame_started.elapsed().as_micros(),
                    budget_us = self.frame_watchdog.frame_budget().as_micros(),
                    "compositor exceeded watchdog budget and was downgraded to SCHED_OTHER"
                ),
                Ok(_) => {}
                Err(error) => tracing::error!(%error, "failed to downgrade compositor scheduler"),
            }
        }
    }

    fn process_input<B: InputBackend>(&mut self, event: InputEvent<B>) {
        self.pending_input_at = Some(Instant::now());
        match event {
            InputEvent::Keyboard { event } => self.keyboard::<B>(event),
            InputEvent::PointerMotion { event } => {
                let current = self.state.pointer.current_location();
                self.pointer_motion(current + event.delta(), event.time_msec());
            }
            InputEvent::PointerMotionAbsolute { event } => {
                let size = self.desktop_size();
                self.pointer_motion(event.position_transformed(size), event.time_msec());
            }
            InputEvent::PointerButton { event } => {
                let serial = self.next_serial();
                if event.state() == smithay::backend::input::ButtonState::Pressed {
                    let focus = self
                        .state
                        .window_manager
                        .surface_under(self.state.pointer.current_location());
                    if let Some(ref surface) = focus {
                        self.state.window_manager.set_focus(surface);
                    }
                    self.state
                        .keyboard
                        .clone()
                        .set_focus(&mut self.state, focus, serial);
                }
                let button = ButtonEvent {
                    serial,
                    time: event.time_msec(),
                    button: event.button_code(),
                    state: event.state(),
                };
                self.state.pointer.clone().button(&mut self.state, &button);
                self.state.pointer.clone().frame(&mut self.state);
            }
            InputEvent::PointerAxis { event } => self.pointer_axis::<B>(event),
            _ => {}
        }
    }

    fn keyboard<B: InputBackend>(&mut self, event: B::KeyboardKeyEvent) {
        enum Action {
            Vt(i32),
            Cycle,
            None,
        }
        let serial = self.next_serial();
        let key_state = event.state();
        let keyboard = self.state.keyboard.clone();
        let mut suppressed = std::mem::take(&mut self.suppressed_keys);
        let action = keyboard
            .input::<Action, _>(
                &mut self.state,
                event.key_code(),
                key_state,
                serial,
                event.time_msec(),
                |_, modifiers, handle| {
                    let sym = handle.modified_sym();
                    if key_state == KeyState::Pressed
                        && (keysyms::KEY_XF86Switch_VT_1..=keysyms::KEY_XF86Switch_VT_12)
                            .contains(&sym.raw())
                    {
                        let vt = (sym.raw() - keysyms::KEY_XF86Switch_VT_1 + 1) as i32;
                        suppressed.push(sym);
                        FilterResult::Intercept(Action::Vt(vt))
                    } else if key_state == KeyState::Pressed
                        && modifiers.alt
                        && (sym == Keysym::Tab || sym == Keysym::ISO_Left_Tab)
                    {
                        suppressed.push(sym);
                        FilterResult::Intercept(Action::Cycle)
                    } else if key_state == KeyState::Released && suppressed.contains(&sym) {
                        suppressed.retain(|key| *key != sym);
                        FilterResult::Intercept(Action::None)
                    } else {
                        FilterResult::Forward
                    }
                },
            )
            .unwrap_or(Action::None);
        self.suppressed_keys = suppressed;
        match action {
            Action::Vt(vt) => {
                if let Err(error) = self.session.change_vt(vt) {
                    tracing::error!(vt, %error, "VT switch failed");
                }
            }
            Action::Cycle => {
                let focus = self.state.window_manager.cycle_focus();
                keyboard.set_focus(&mut self.state, focus, serial);
            }
            Action::None => {}
        }
    }

    fn pointer_motion(&mut self, mut location: Point<f64, Logical>, time: u32) {
        let size = self.desktop_size();
        location.x = location.x.clamp(0.0, f64::from(size.w.saturating_sub(1)));
        location.y = location.y.clamp(0.0, f64::from(size.h.saturating_sub(1)));
        let serial = self.next_serial();
        let focus = self.state.window_manager.surface_under(location);
        let under = focus.as_ref().map(|surface| {
            let origin = self
                .state
                .window_manager
                .surface_geometry(surface)
                .map(|geometry| geometry.loc)
                .unwrap_or_default();
            (surface.clone(), origin.to_f64())
        });
        let motion = MotionEvent {
            location,
            serial,
            time,
        };
        self.state
            .pointer
            .clone()
            .motion(&mut self.state, under, &motion);
        self.state.pointer.clone().frame(&mut self.state);
    }

    fn pointer_axis<B: InputBackend>(&mut self, event: B::PointerAxisEvent) {
        let mut frame = AxisFrame::new(event.time_msec()).source(event.source());
        for axis in [Axis::Horizontal, Axis::Vertical] {
            let amount = event
                .amount(axis)
                .unwrap_or_else(|| event.amount_v120(axis).unwrap_or(0.0) * 15.0 / 120.0);
            if amount != 0.0 {
                frame = frame
                    .relative_direction(axis, event.relative_direction(axis))
                    .value(axis, amount);
                if let Some(v120) = event.amount_v120(axis) {
                    frame = frame.v120(axis, v120 as i32);
                }
            } else if event.source() == AxisSource::Finger {
                frame = frame.stop(axis);
            }
        }
        self.state.pointer.clone().axis(&mut self.state, frame);
        self.state.pointer.clone().frame(&mut self.state);
    }

    fn desktop_size(&self) -> Size<i32, Logical> {
        let (width, height) =
            self.state
                .outputs
                .outputs
                .iter()
                .fold((1, 1), |(width, height), output| {
                    let location = output.current_location();
                    let size = output
                        .current_mode()
                        .map(|mode| mode.size)
                        .unwrap_or_default();
                    (
                        width.max(location.x + size.w),
                        height.max(location.y + size.h),
                    )
                });
        Size::new(width.max(1), height.max(1))
    }

    fn release_all_keys(&mut self) {
        let keyboard = self.state.keyboard.clone();
        let pressed = keyboard.pressed_keys();
        let mut suppressed = std::mem::take(&mut self.suppressed_keys);
        for keycode in pressed {
            let serial = self.next_serial();
            let time = self.started_at.elapsed().as_millis() as u32;
            keyboard.input::<(), _>(
                &mut self.state,
                keycode,
                KeyState::Released,
                serial,
                time,
                |_, _, handle| {
                    let sym = handle.modified_sym();
                    if suppressed.contains(&sym) {
                        suppressed.retain(|key| *key != sym);
                        FilterResult::Intercept(())
                    } else {
                        FilterResult::Forward
                    }
                },
            );
        }
    }

    fn next_serial(&mut self) -> Serial {
        let serial = Serial::from(self.serial);
        self.serial = self.serial.wrapping_add(1);
        serial
    }
}

#[cfg(test)]
mod tests {
    use super::output_name;

    #[test]
    fn connector_names_remain_unique_across_drm_cards() {
        assert_eq!(output_name("card0", "HDMI-A-1"), "card0-HDMI-A-1");
        assert_eq!(output_name("card1", "HDMI-A-1"), "card1-HDMI-A-1");
        assert_ne!(
            output_name("card0", "HDMI-A-1"),
            output_name("card1", "HDMI-A-1")
        );
    }
}
