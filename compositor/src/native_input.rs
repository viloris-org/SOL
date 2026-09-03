//! Native Linux input backend.
//!
//! The DRM backend owns the display directly, so its input path must likewise
//! consume kernel input events directly rather than depending on Wayland or X11.
//! This module reads evdev devices from `/dev/input/event*`, translates their
//! batches into SCP's XKB-keycode, pointer, wheel, and multitouch entry points,
//! and periodically scans for hot-plugged devices.

use crate::scp::{
    ScpState,
    protocol::{AxisSource, ButtonState, KeyState, Orientation},
};
use std::{
    collections::HashSet,
    fs::{File, OpenOptions},
    io,
    mem::{self, MaybeUninit},
    os::{fd::AsRawFd, unix::fs::OpenOptionsExt},
    path::PathBuf,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};

const INPUT_DIRECTORY: &str = "/dev/input";
const RESCAN_INTERVAL: Duration = Duration::from_secs(2);
const POLL_TIMEOUT_MS: i32 = 100;

const EV_SYN: u16 = 0;
const EV_KEY: u16 = 1;
const EV_REL: u16 = 2;
const EV_ABS: u16 = 3;
const SYN_REPORT: u16 = 0;
const SYN_DROPPED: u16 = 3;

const REL_X: u16 = 0;
const REL_Y: u16 = 1;
const REL_HWHEEL: u16 = 6;
const REL_WHEEL: u16 = 8;
const REL_WHEEL_HI_RES: u16 = 11;
const REL_HWHEEL_HI_RES: u16 = 12;

const ABS_X: u16 = 0;
const ABS_Y: u16 = 1;
const ABS_MT_SLOT: u16 = 0x2f;
const ABS_MT_POSITION_X: u16 = 0x35;
const ABS_MT_POSITION_Y: u16 = 0x36;
const ABS_MT_TRACKING_ID: u16 = 0x39;

const BTN_MISC: u16 = 0x100;
const BTN_MOUSE: u16 = 0x110;
const BTN_TASK: u16 = 0x117;
const BTN_TOUCH: u16 = 0x14a;

const INPUT_PROP_POINTER: usize = 0;
const INPUT_PROP_DIRECT: usize = 1;
const INPUT_PROP_BUTTONPAD: usize = 2;

const IOC_WRITE: libc::c_ulong = 1;
const IOC_READ: libc::c_ulong = 2;
const IOC_SIZE_SHIFT: libc::c_ulong = 16;
const IOC_DIR_SHIFT: libc::c_ulong = 30;
const EVDEV_IOCTL_TYPE: libc::c_ulong = b'E' as libc::c_ulong;

/// Events after evdev batching and device-specific coordinate translation.
#[derive(Debug, Clone, PartialEq)]
enum NativeEvent {
    PointerDelta {
        dx: f64,
        dy: f64,
        time_ms: u32,
    },
    PointerButton {
        code: u32,
        pressed: bool,
        time_ms: u32,
    },
    PointerAxis {
        orientation: Orientation,
        value: f64,
        discrete: i32,
        time_ms: u32,
    },
    PointerFrame,
    Key {
        code: u32,
        pressed: bool,
        time_ms: u32,
    },
    TouchDown {
        id: i32,
        x: f64,
        y: f64,
        time_ms: u32,
    },
    TouchMotion {
        id: i32,
        x: f64,
        y: f64,
        time_ms: u32,
    },
    TouchUp {
        id: i32,
        time_ms: u32,
    },
    TouchFrame,
    CancelTouches,
}

#[repr(C)]
#[derive(Clone, Copy, Debug)]
struct LinuxInputEvent {
    time: libc::timeval,
    event_type: u16,
    code: u16,
    value: i32,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
struct LinuxAbsInfo {
    value: i32,
    minimum: i32,
    maximum: i32,
    fuzz: i32,
    flat: i32,
    resolution: i32,
}

#[derive(Clone, Copy, Debug)]
struct AxisRange {
    minimum: i32,
    maximum: i32,
}

impl AxisRange {
    fn normalize(self, value: i32, extent: u32) -> f64 {
        let span = i64::from(self.maximum) - i64::from(self.minimum);
        if span <= 0 || extent <= 1 {
            return 0.0;
        }
        let offset = (i64::from(value) - i64::from(self.minimum)).clamp(0, span);
        offset as f64 * f64::from(extent - 1) / span as f64
    }

    fn delta_scale(self, extent: u32) -> f64 {
        let span = i64::from(self.maximum) - i64::from(self.minimum);
        if span <= 0 {
            1.0
        } else {
            f64::from(extent) / span as f64
        }
    }
}

#[derive(Debug, Default)]
struct TouchSlot {
    active: bool,
    transition: Option<bool>,
    x: Option<i32>,
    y: Option<i32>,
    moved: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AbsoluteKind {
    None,
    Direct,
    Touchpad,
}

/// Stateful decoder for one evdev event node.
struct DeviceDecoder {
    relevant: bool,
    relative_pointer: bool,
    absolute_kind: AbsoluteKind,
    x_range: Option<AxisRange>,
    y_range: Option<AxisRange>,
    extent: (u32, u32),
    rel_x: i32,
    rel_y: i32,
    wheel: i32,
    hwheel: i32,
    wheel_hi_res: i32,
    hwheel_hi_res: i32,
    pointer_batch: bool,
    current_slot: usize,
    touch_slots: Vec<TouchSlot>,
    single_touch_contact: bool,
    touchpad_x: Option<i32>,
    touchpad_y: Option<i32>,
    touchpad_previous: Option<(i32, i32)>,
    last_time_ms: u32,
}

impl DeviceDecoder {
    fn from_fd(fd: libc::c_int, extent: (u32, u32)) -> Self {
        let event_types = ioctl_bytes(fd, eviocgbit(0, 64), 64).unwrap_or_default();
        let relative_axes = ioctl_bytes(fd, eviocgbit(EV_REL, 8), 8).unwrap_or_default();
        let relative_pointer = bit_is_set(&event_types, EV_REL as usize)
            && (bit_is_set(&relative_axes, REL_X as usize)
                || bit_is_set(&relative_axes, REL_Y as usize));
        let has_absolute = bit_is_set(&event_types, EV_ABS as usize);
        let keys = ioctl_bytes(fd, eviocgbit(EV_KEY, 96), 96).unwrap_or_default();
        // A full keyboard event node exposes ordinary typing keys. This keeps
        // us from grabbing separate power-button/lid-switch event nodes.
        let keyboard = bit_is_set(&keys, 30) // KEY_A
            || bit_is_set(&keys, 28) // KEY_ENTER
            || bit_is_set(&keys, 57); // KEY_SPACE
        let pointer_buttons = (BTN_MOUSE..=BTN_TASK).any(|code| bit_is_set(&keys, code as usize));
        let properties = ioctl_bytes(fd, eviocgprop(8), 8).unwrap_or_default();
        let direct = bit_is_set(&properties, INPUT_PROP_DIRECT);
        let pointer = bit_is_set(&properties, INPUT_PROP_POINTER)
            || bit_is_set(&properties, INPUT_PROP_BUTTONPAD);

        let mt_x = abs_info(fd, ABS_MT_POSITION_X);
        let mt_y = abs_info(fd, ABS_MT_POSITION_Y);
        let plain_x = abs_info(fd, ABS_X);
        let plain_y = abs_info(fd, ABS_Y);
        let has_mt = mt_x.is_some() && mt_y.is_some();
        let absolute_kind = if has_absolute
            && (direct || (has_mt && !pointer))
            && (has_mt || (plain_x.is_some() && plain_y.is_some()))
        {
            AbsoluteKind::Direct
        } else if has_absolute && pointer && plain_x.is_some() && plain_y.is_some() {
            AbsoluteKind::Touchpad
        } else {
            AbsoluteKind::None
        };
        let (x_range, y_range) = if has_mt {
            (mt_x, mt_y)
        } else {
            (plain_x, plain_y)
        };
        let slot_count = abs_info(fd, ABS_MT_SLOT)
            .map(|range| range.maximum.saturating_add(1).clamp(1, 32) as usize)
            .unwrap_or(1);

        Self::new(
            keyboard || pointer_buttons || relative_pointer || absolute_kind != AbsoluteKind::None,
            relative_pointer,
            absolute_kind,
            x_range,
            y_range,
            extent,
            slot_count,
        )
    }

    fn new(
        relevant: bool,
        relative_pointer: bool,
        absolute_kind: AbsoluteKind,
        x_range: Option<AxisRange>,
        y_range: Option<AxisRange>,
        extent: (u32, u32),
        slot_count: usize,
    ) -> Self {
        Self {
            relevant,
            relative_pointer,
            absolute_kind,
            x_range,
            y_range,
            extent,
            rel_x: 0,
            rel_y: 0,
            wheel: 0,
            hwheel: 0,
            wheel_hi_res: 0,
            hwheel_hi_res: 0,
            pointer_batch: false,
            current_slot: 0,
            touch_slots: (0..slot_count.max(1))
                .map(|_| TouchSlot::default())
                .collect(),
            single_touch_contact: false,
            touchpad_x: None,
            touchpad_y: None,
            touchpad_previous: None,
            last_time_ms: 0,
        }
    }

    fn push(&mut self, event: LinuxInputEvent, output: &mut Vec<NativeEvent>) {
        self.last_time_ms = event_time_ms(&event);
        match event.event_type {
            EV_SYN if event.code == SYN_REPORT => self.finish_batch(output),
            EV_SYN if event.code == SYN_DROPPED => {
                self.clear_batch();
                self.touchpad_previous = None;
                for slot in &mut self.touch_slots {
                    *slot = TouchSlot::default();
                }
                output.push(NativeEvent::CancelTouches);
            }
            EV_KEY => self.handle_key(event, output),
            EV_REL => self.handle_relative(event),
            EV_ABS => self.handle_absolute(event),
            _ => {}
        }
    }

    fn handle_key(&mut self, event: LinuxInputEvent, output: &mut Vec<NativeEvent>) {
        // Kernel repeat notifications are intentionally ignored. SCP publishes
        // repeat settings and clients repeat from that contract.
        if event.value != 0 && event.value != 1 {
            return;
        }
        let pressed = event.value == 1;
        if event.code == BTN_TOUCH {
            self.single_touch_contact = pressed;
            if self.absolute_kind == AbsoluteKind::Direct && self.touch_slots.len() == 1 {
                self.touch_slots[0].transition = Some(pressed);
                self.touch_slots[0].active = pressed;
            }
        } else if (BTN_MOUSE..=BTN_TASK).contains(&event.code) {
            self.pointer_batch = true;
            output.push(NativeEvent::PointerButton {
                code: u32::from(event.code),
                pressed,
                time_ms: self.last_time_ms,
            });
        } else if event.code < BTN_MISC {
            output.push(NativeEvent::Key {
                // SCP uses XKB's evdev keycode space.
                code: u32::from(event.code) + 8,
                pressed,
                time_ms: self.last_time_ms,
            });
        }
    }

    fn handle_relative(&mut self, event: LinuxInputEvent) {
        if !self.relative_pointer {
            return;
        }
        match event.code {
            REL_X => self.rel_x = self.rel_x.saturating_add(event.value),
            REL_Y => self.rel_y = self.rel_y.saturating_add(event.value),
            REL_WHEEL => self.wheel = self.wheel.saturating_add(event.value),
            REL_HWHEEL => self.hwheel = self.hwheel.saturating_add(event.value),
            REL_WHEEL_HI_RES => self.wheel_hi_res = self.wheel_hi_res.saturating_add(event.value),
            REL_HWHEEL_HI_RES => {
                self.hwheel_hi_res = self.hwheel_hi_res.saturating_add(event.value)
            }
            _ => return,
        }
        self.pointer_batch = true;
    }

    fn handle_absolute(&mut self, event: LinuxInputEvent) {
        match event.code {
            ABS_MT_SLOT => {
                self.current_slot = usize::try_from(event.value)
                    .unwrap_or(0)
                    .min(self.touch_slots.len() - 1);
            }
            ABS_MT_TRACKING_ID if self.absolute_kind == AbsoluteKind::Direct => {
                let slot = &mut self.touch_slots[self.current_slot];
                let active = event.value >= 0;
                slot.transition = Some(active);
                slot.active = active;
                slot.moved = false;
            }
            ABS_MT_POSITION_X if self.absolute_kind == AbsoluteKind::Direct => {
                let slot = &mut self.touch_slots[self.current_slot];
                slot.x = Some(event.value);
                slot.moved = true;
            }
            ABS_MT_POSITION_Y if self.absolute_kind == AbsoluteKind::Direct => {
                let slot = &mut self.touch_slots[self.current_slot];
                slot.y = Some(event.value);
                slot.moved = true;
            }
            ABS_X if self.absolute_kind == AbsoluteKind::Direct => {
                self.touch_slots[0].x = Some(event.value);
                self.touch_slots[0].moved = true;
            }
            ABS_Y if self.absolute_kind == AbsoluteKind::Direct => {
                self.touch_slots[0].y = Some(event.value);
                self.touch_slots[0].moved = true;
            }
            ABS_X if self.absolute_kind == AbsoluteKind::Touchpad => {
                self.touchpad_x = Some(event.value)
            }
            ABS_Y if self.absolute_kind == AbsoluteKind::Touchpad => {
                self.touchpad_y = Some(event.value)
            }
            _ => {}
        }
    }

    fn finish_batch(&mut self, output: &mut Vec<NativeEvent>) {
        let mut emitted_pointer = self.pointer_batch;
        if self.rel_x != 0 || self.rel_y != 0 {
            output.push(NativeEvent::PointerDelta {
                dx: f64::from(self.rel_x),
                dy: f64::from(self.rel_y),
                time_ms: self.last_time_ms,
            });
        }
        self.emit_wheel(Orientation::Vertical, self.wheel, self.wheel_hi_res, output);
        self.emit_wheel(
            Orientation::Horizontal,
            self.hwheel,
            self.hwheel_hi_res,
            output,
        );

        if self.absolute_kind == AbsoluteKind::Touchpad {
            if self.single_touch_contact {
                if let (Some(x), Some(y), Some((old_x, old_y))) =
                    (self.touchpad_x, self.touchpad_y, self.touchpad_previous)
                {
                    let dx = f64::from(x.saturating_sub(old_x))
                        * self
                            .x_range
                            .map_or(1.0, |range| range.delta_scale(self.extent.0));
                    let dy = f64::from(y.saturating_sub(old_y))
                        * self
                            .y_range
                            .map_or(1.0, |range| range.delta_scale(self.extent.1));
                    if dx != 0.0 || dy != 0.0 {
                        output.push(NativeEvent::PointerDelta {
                            dx,
                            dy,
                            time_ms: self.last_time_ms,
                        });
                        emitted_pointer = true;
                    }
                }
                self.touchpad_previous = self.touchpad_x.zip(self.touchpad_y);
            } else {
                self.touchpad_previous = None;
            }
        }

        if emitted_pointer {
            output.push(NativeEvent::PointerFrame);
        }
        self.finish_touch_batch(output);
        self.clear_batch();
    }

    fn emit_wheel(
        &self,
        orientation: Orientation,
        low_res: i32,
        high_res: i32,
        output: &mut Vec<NativeEvent>,
    ) {
        if low_res == 0 && high_res == 0 {
            return;
        }
        // One traditional wheel detent is 120 high-resolution units. Prefer
        // high-resolution data when a device reports both forms in a batch.
        let steps = if high_res != 0 {
            f64::from(high_res) / 120.0
        } else {
            f64::from(low_res)
        };
        output.push(NativeEvent::PointerAxis {
            orientation,
            value: -steps * 15.0,
            discrete: if low_res != 0 {
                -low_res
            } else {
                -(high_res / 120)
            },
            time_ms: self.last_time_ms,
        });
    }

    fn finish_touch_batch(&mut self, output: &mut Vec<NativeEvent>) {
        if self.absolute_kind != AbsoluteKind::Direct {
            return;
        }
        let Some(x_range) = self.x_range else { return };
        let Some(y_range) = self.y_range else { return };
        let mut emitted = false;
        for (index, slot) in self.touch_slots.iter_mut().enumerate() {
            match slot.transition {
                Some(false) => {
                    output.push(NativeEvent::TouchUp {
                        id: index as i32,
                        time_ms: self.last_time_ms,
                    });
                    emitted = true;
                }
                Some(true) if slot.x.is_some() && slot.y.is_some() => {
                    output.push(NativeEvent::TouchDown {
                        id: index as i32,
                        x: x_range.normalize(slot.x.unwrap_or_default(), self.extent.0),
                        y: y_range.normalize(slot.y.unwrap_or_default(), self.extent.1),
                        time_ms: self.last_time_ms,
                    });
                    emitted = true;
                }
                None if slot.active && slot.moved && slot.x.is_some() && slot.y.is_some() => {
                    output.push(NativeEvent::TouchMotion {
                        id: index as i32,
                        x: x_range.normalize(slot.x.unwrap_or_default(), self.extent.0),
                        y: y_range.normalize(slot.y.unwrap_or_default(), self.extent.1),
                        time_ms: self.last_time_ms,
                    });
                    emitted = true;
                }
                _ => {}
            }
            slot.transition = None;
            slot.moved = false;
        }
        if emitted {
            output.push(NativeEvent::TouchFrame);
        }
    }

    fn clear_batch(&mut self) {
        self.rel_x = 0;
        self.rel_y = 0;
        self.wheel = 0;
        self.hwheel = 0;
        self.wheel_hi_res = 0;
        self.hwheel_hi_res = 0;
        self.pointer_batch = false;
    }
}

struct InputDevice {
    path: PathBuf,
    name: String,
    file: File,
    decoder: DeviceDecoder,
}

impl InputDevice {
    fn open(path: PathBuf, extent: (u32, u32), grab: bool) -> io::Result<Option<Self>> {
        let file = OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_NONBLOCK | libc::O_CLOEXEC)
            .open(&path)?;
        let name = device_name(file.as_raw_fd()).unwrap_or_else(|| {
            path.file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .into_owned()
        });
        let decoder = DeviceDecoder::from_fd(file.as_raw_fd(), extent);
        if !decoder.relevant {
            return Ok(None);
        }

        if grab {
            // SAFETY: `file` is an open evdev descriptor. EVIOCGRAB interprets
            // its integer argument as grab (non-zero) or release (zero).
            let result = unsafe { libc::ioctl(file.as_raw_fd(), eviocgrab(), 1 as libc::c_int) };
            if result < 0 {
                tracing::warn!(device = %path.display(), error = %io::Error::last_os_error(),
                    "could not exclusively grab input device");
            }
        }

        Ok(Some(Self {
            path,
            name,
            file,
            decoder,
        }))
    }

    fn read_ready(&mut self, output: &mut Vec<NativeEvent>) -> io::Result<()> {
        let mut events = [MaybeUninit::<LinuxInputEvent>::uninit(); 64];
        loop {
            // SAFETY: the buffer is writable for its full byte length. A
            // successful evdev read initializes a whole number of input_event
            // records, checked below before any record is read.
            let bytes = unsafe {
                libc::read(
                    self.file.as_raw_fd(),
                    events.as_mut_ptr().cast(),
                    mem::size_of_val(&events),
                )
            };
            if bytes < 0 {
                let error = io::Error::last_os_error();
                if error.kind() == io::ErrorKind::WouldBlock {
                    return Ok(());
                }
                if error.kind() == io::ErrorKind::Interrupted {
                    continue;
                }
                return Err(error);
            }
            if bytes == 0 {
                return Err(io::Error::new(
                    io::ErrorKind::UnexpectedEof,
                    "input device closed",
                ));
            }
            let bytes = bytes as usize;
            if !bytes.is_multiple_of(mem::size_of::<LinuxInputEvent>()) {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "partial evdev event",
                ));
            }
            let count = bytes / mem::size_of::<LinuxInputEvent>();
            for event in events.iter().take(count) {
                // SAFETY: the successful read initialized these `count` slots.
                self.decoder.push(unsafe { event.assume_init() }, output);
            }
        }
    }
}

/// Real hardware input source for the native DRM compositor.
pub struct NativeInputBackend {
    devices: Vec<InputDevice>,
    known_paths: HashSet<PathBuf>,
    extent: (u32, u32),
    pointer: (f64, f64),
    grab: bool,
    last_scan: Instant,
}

impl NativeInputBackend {
    /// Discover currently present evdev devices. An empty set is valid: the
    /// backend keeps scanning so USB/Bluetooth input can be attached later.
    pub fn discover(extent: (u32, u32)) -> Self {
        let extent = (extent.0.max(1), extent.1.max(1));
        let grab = std::env::var_os("SOL_INPUT_NO_GRAB").is_none();
        let mut backend = Self {
            devices: Vec::new(),
            known_paths: HashSet::new(),
            extent,
            pointer: (f64::from(extent.0) / 2.0, f64::from(extent.1) / 2.0),
            grab,
            last_scan: Instant::now() - RESCAN_INTERVAL,
        };
        backend.scan_devices();
        backend
    }

    pub fn device_count(&self) -> usize {
        self.devices.len()
    }

    /// Poll devices until compositor shutdown. Device errors are isolated to
    /// that node; other devices and hot-plug discovery continue working.
    pub fn run(mut self, state: Arc<Mutex<ScpState>>, running: Arc<AtomicBool>) {
        while running.load(Ordering::Acquire) {
            if self.last_scan.elapsed() >= RESCAN_INTERVAL {
                self.scan_devices();
            }
            let mut descriptors: Vec<_> = self
                .devices
                .iter()
                .map(|device| libc::pollfd {
                    fd: device.file.as_raw_fd(),
                    events: libc::POLLIN,
                    revents: 0,
                })
                .collect();
            // SAFETY: `descriptors` owns `len` initialized pollfd records. With
            // length zero poll does not dereference the (possibly dangling)
            // empty-vector pointer.
            let ready = unsafe {
                libc::poll(
                    descriptors.as_mut_ptr(),
                    descriptors.len() as libc::nfds_t,
                    POLL_TIMEOUT_MS,
                )
            };
            if ready < 0 {
                let error = io::Error::last_os_error();
                if error.kind() != io::ErrorKind::Interrupted {
                    tracing::warn!(%error, "native input poll failed");
                }
                continue;
            }

            let mut native_events = Vec::new();
            let mut failed = Vec::new();
            for (index, descriptor) in descriptors.iter().enumerate() {
                if descriptor.revents & libc::POLLIN != 0
                    && let Err(error) = self.devices[index].read_ready(&mut native_events)
                {
                    tracing::warn!(device = %self.devices[index].path.display(), %error,
                        "input device stopped producing events");
                    failed.push(index);
                }
                if descriptor.revents & (libc::POLLERR | libc::POLLHUP | libc::POLLNVAL) != 0 {
                    failed.push(index);
                }
            }
            failed.sort_unstable();
            failed.dedup();
            for index in failed.into_iter().rev() {
                let removed = self.devices.remove(index);
                self.known_paths.remove(&removed.path);
                tracing::info!(device = %removed.name, "native input device removed");
                native_events.push(NativeEvent::CancelTouches);
            }
            self.dispatch(&state, native_events);
        }
    }

    fn scan_devices(&mut self) {
        self.last_scan = Instant::now();
        self.known_paths.retain(|path| path.exists());
        let Ok(entries) = std::fs::read_dir(INPUT_DIRECTORY) else {
            return;
        };
        let mut paths: Vec<_> = entries
            .flatten()
            .map(|entry| entry.path())
            .filter(|path| {
                path.file_name()
                    .and_then(|name| name.to_str())
                    .is_some_and(|name| name.starts_with("event"))
            })
            .collect();
        paths.sort();
        for path in paths {
            if !self.known_paths.insert(path.clone()) {
                continue;
            }
            match InputDevice::open(path.clone(), self.extent, self.grab) {
                Ok(Some(device)) => {
                    tracing::info!(device = %device.name, path = %path.display(),
                        "native input device attached");
                    self.devices.push(device);
                }
                Ok(None) => {}
                Err(error) => tracing::warn!(path = %path.display(), %error,
                    "cannot open native input device"),
            }
        }
    }

    fn dispatch(&mut self, state: &Arc<Mutex<ScpState>>, events: Vec<NativeEvent>) {
        if events.is_empty() {
            return;
        }
        let Ok(mut state) = state.lock() else {
            tracing::error!("compositor state lock is poisoned; native input stopped");
            return;
        };
        for event in events {
            match event {
                NativeEvent::PointerDelta { dx, dy, time_ms } => {
                    self.pointer.0 = (self.pointer.0 + dx).clamp(0.0, f64::from(self.extent.0 - 1));
                    self.pointer.1 = (self.pointer.1 + dy).clamp(0.0, f64::from(self.extent.1 - 1));
                    state.handle_pointer_motion(self.pointer.0, self.pointer.1, time_ms);
                }
                NativeEvent::PointerButton {
                    code,
                    pressed,
                    time_ms,
                } => state.handle_pointer_button(
                    code,
                    if pressed {
                        ButtonState::Pressed
                    } else {
                        ButtonState::Released
                    },
                    time_ms,
                ),
                NativeEvent::PointerAxis {
                    orientation,
                    value,
                    discrete,
                    time_ms,
                } => state.handle_pointer_axis(
                    AxisSource::Wheel,
                    orientation,
                    value,
                    discrete,
                    time_ms,
                ),
                NativeEvent::PointerFrame => state.handle_pointer_frame(),
                NativeEvent::Key {
                    code,
                    pressed,
                    time_ms,
                } => state.handle_key(
                    code,
                    if pressed {
                        KeyState::Pressed
                    } else {
                        KeyState::Released
                    },
                    time_ms,
                ),
                NativeEvent::TouchDown { id, x, y, time_ms } => {
                    state.handle_touch_down(id, x, y, time_ms)
                }
                NativeEvent::TouchMotion { id, x, y, time_ms } => {
                    state.handle_touch_motion(id, x, y, time_ms)
                }
                NativeEvent::TouchUp { id, time_ms } => state.handle_touch_up(id, time_ms),
                NativeEvent::TouchFrame => state.handle_touch_frame(),
                NativeEvent::CancelTouches => {
                    state.handle_touch_cancel();
                    state.reset_keyboard_state();
                }
            }
        }
    }
}

fn event_time_ms(event: &LinuxInputEvent) -> u32 {
    let seconds = u64::try_from(event.time.tv_sec).unwrap_or_default();
    let micros = u64::try_from(event.time.tv_usec).unwrap_or_default();
    (seconds.saturating_mul(1_000).saturating_add(micros / 1_000)) as u32
}

fn bit_is_set(bytes: &[u8], bit: usize) -> bool {
    bytes
        .get(bit / 8)
        .is_some_and(|byte| byte & (1 << (bit % 8)) != 0)
}

fn ioctl_bytes(fd: libc::c_int, request: libc::c_ulong, length: usize) -> io::Result<Vec<u8>> {
    let mut bytes = vec![0_u8; length];
    // SAFETY: `bytes` is writable for `length`, which is encoded in `request`.
    let result = unsafe { libc::ioctl(fd, request, bytes.as_mut_ptr()) };
    if result < 0 {
        Err(io::Error::last_os_error())
    } else {
        Ok(bytes)
    }
}

fn abs_info(fd: libc::c_int, axis: u16) -> Option<AxisRange> {
    let mut info = LinuxAbsInfo::default();
    // SAFETY: `info` is a correctly sized writable input_absinfo record.
    let result = unsafe { libc::ioctl(fd, eviocgabs(axis), &mut info) };
    (result >= 0 && info.maximum > info.minimum).then_some(AxisRange {
        minimum: info.minimum,
        maximum: info.maximum,
    })
}

fn device_name(fd: libc::c_int) -> Option<String> {
    let bytes = ioctl_bytes(fd, eviocgname(256), 256).ok()?;
    let end = bytes
        .iter()
        .position(|byte| *byte == 0)
        .unwrap_or(bytes.len());
    (!bytes[..end].is_empty()).then(|| String::from_utf8_lossy(&bytes[..end]).into_owned())
}

const fn ioctl_request(
    direction: libc::c_ulong,
    number: libc::c_ulong,
    size: usize,
) -> libc::c_ulong {
    (direction << IOC_DIR_SHIFT)
        | ((size as libc::c_ulong) << IOC_SIZE_SHIFT)
        | (EVDEV_IOCTL_TYPE << 8)
        | number
}

const fn eviocgbit(event_type: u16, length: usize) -> libc::c_ulong {
    ioctl_request(IOC_READ, 0x20 + event_type as libc::c_ulong, length)
}

const fn eviocgprop(length: usize) -> libc::c_ulong {
    ioctl_request(IOC_READ, 0x09, length)
}

const fn eviocgname(length: usize) -> libc::c_ulong {
    ioctl_request(IOC_READ, 0x06, length)
}

const fn eviocgabs(axis: u16) -> libc::c_ulong {
    ioctl_request(
        IOC_READ,
        0x40 + axis as libc::c_ulong,
        mem::size_of::<LinuxAbsInfo>(),
    )
}

const fn eviocgrab() -> libc::c_ulong {
    ioctl_request(IOC_WRITE, 0x90, mem::size_of::<libc::c_int>())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn raw(event_type: u16, code: u16, value: i32, milliseconds: i64) -> LinuxInputEvent {
        LinuxInputEvent {
            time: libc::timeval {
                tv_sec: milliseconds / 1_000,
                tv_usec: (milliseconds % 1_000) * 1_000,
            },
            event_type,
            code,
            value,
        }
    }

    #[test]
    fn keyboard_codes_are_converted_to_xkb_and_repeat_is_ignored() {
        let mut decoder =
            DeviceDecoder::new(true, false, AbsoluteKind::None, None, None, (100, 100), 1);
        let mut output = Vec::new();
        decoder.push(raw(EV_KEY, 1, 1, 10), &mut output);
        decoder.push(raw(EV_KEY, 1, 2, 20), &mut output);
        decoder.push(raw(EV_KEY, 1, 0, 30), &mut output);
        assert_eq!(
            output,
            vec![
                NativeEvent::Key {
                    code: 9,
                    pressed: true,
                    time_ms: 10
                },
                NativeEvent::Key {
                    code: 9,
                    pressed: false,
                    time_ms: 30
                },
            ]
        );
    }

    #[test]
    fn relative_motion_and_high_resolution_wheel_are_one_pointer_frame() {
        let mut decoder =
            DeviceDecoder::new(true, true, AbsoluteKind::None, None, None, (100, 100), 1);
        let mut output = Vec::new();
        decoder.push(raw(EV_REL, REL_X, 4, 10), &mut output);
        decoder.push(raw(EV_REL, REL_Y, -2, 10), &mut output);
        decoder.push(raw(EV_REL, REL_WHEEL, 1, 10), &mut output);
        decoder.push(raw(EV_REL, REL_WHEEL_HI_RES, 60, 10), &mut output);
        decoder.push(raw(EV_SYN, SYN_REPORT, 0, 10), &mut output);
        assert_eq!(
            output[0],
            NativeEvent::PointerDelta {
                dx: 4.0,
                dy: -2.0,
                time_ms: 10
            }
        );
        assert_eq!(
            output[1],
            NativeEvent::PointerAxis {
                orientation: Orientation::Vertical,
                value: -7.5,
                discrete: -1,
                time_ms: 10,
            }
        );
        assert_eq!(output[2], NativeEvent::PointerFrame);
    }

    #[test]
    fn multitouch_slots_become_normalized_touch_sequences() {
        let range = AxisRange {
            minimum: 0,
            maximum: 1_000,
        };
        let mut decoder = DeviceDecoder::new(
            true,
            false,
            AbsoluteKind::Direct,
            Some(range),
            Some(range),
            (101, 201),
            2,
        );
        let mut output = Vec::new();
        decoder.push(raw(EV_ABS, ABS_MT_SLOT, 1, 5), &mut output);
        decoder.push(raw(EV_ABS, ABS_MT_TRACKING_ID, 42, 5), &mut output);
        decoder.push(raw(EV_ABS, ABS_MT_POSITION_X, 500, 5), &mut output);
        decoder.push(raw(EV_ABS, ABS_MT_POSITION_Y, 250, 5), &mut output);
        decoder.push(raw(EV_SYN, SYN_REPORT, 0, 5), &mut output);
        assert_eq!(
            output,
            vec![
                NativeEvent::TouchDown {
                    id: 1,
                    x: 50.0,
                    y: 50.0,
                    time_ms: 5
                },
                NativeEvent::TouchFrame,
            ]
        );

        output.clear();
        decoder.push(raw(EV_ABS, ABS_MT_TRACKING_ID, -1, 8), &mut output);
        decoder.push(raw(EV_SYN, SYN_REPORT, 0, 8), &mut output);
        assert_eq!(
            output,
            vec![
                NativeEvent::TouchUp { id: 1, time_ms: 8 },
                NativeEvent::TouchFrame
            ]
        );
    }
}
