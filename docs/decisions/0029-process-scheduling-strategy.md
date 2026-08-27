# ADR-0029: Process Scheduling Strategy

**Status**: Accepted (Phase 1 core implemented)
**Date**: 2026-08-26  
**Authors**: rownix  

**Implementation**: Phase 1 core lives in `services/sol-scheduler` and is
enforced by `sol-session`/`sol-init`, `sol-compositor`, `sol-shell`, and the
PipeWire scheduling drop-in. Thread separation, priority inheritance, hardware
stress validation, and later phases remain incremental work as described below.

## Context

Traditional Linux schedulers (CFS, EEVDF) are optimized for server throughput, not desktop responsiveness. Modern desktop operating systems (Android, macOS, ChromeOS, Windows) all implement specialized scheduling for interactive workloads.

For SOL to deliver a premium desktop experience, the scheduler must provide:

### Hard Requirements
1. **Consistent frame timing** - 60/120/144Hz without drops
2. **Low input latency** - <10ms mouse/keyboard/touch response
3. **Smooth animations** - no jank during transitions
4. **Gaming performance** - competitive frame times for games
5. **Multi-display support** - Independent refresh rates per output
6. **Power efficiency** - Aggressive throttling when idle

### Modern Desktop Scheduler Capabilities

A production-ready desktop scheduler needs:

**Responsiveness Guarantees**
- Compositor/window server hard real-time scheduling
- Input event priority elevation
- Frame-time consistency (no jitter)

**Resource Isolation**
- Cgroup hierarchy for foreground/background separation
- CPU/IO/memory quota enforcement
- Background app suspension/throttling

**Workload Awareness**
- Application type detection (game, video editor, IDE, browser)
- Dynamic scheduling policy adaptation
- Thermal/battery state responsiveness

**Frame-Aligned Scheduling**
- Application render sync with compositor vsync
- Predictive wakeup to reduce pipeline stalls
- Per-output independent refresh rate scheduling

**Interaction Detection**
- User activity sensing (mouse/keyboard/touch events)
- Priority boost during active interaction
- Fast transition back to idle state

**Observability**
- Frame timing and latency metrics
- Scheduling event tracing (ftrace/eBPF)
- Per-app resource usage statistics

The question: How should SOL implement these capabilities while maintaining security and simplicity?

## Decision

SOL will use **hierarchical scheduling priorities** with real-time guarantees for the compositor and resource isolation for applications:

### 1. Compositor: Real-Time Scheduling

The compositor render thread uses **SCHED_FIFO priority 2** with watchdog protection:

```rust
// Compositor render/present thread only
SCHED_FIFO priority 2
Frame budget: 16.67ms (60Hz) / 8.33ms (120Hz)
Watchdog: Auto-downgrade if budget exceeded
```

**Rationale**:
- Frame drops are immediately visible to users
- Compositor must meet hard deadlines for smooth UI
- Input → render pipeline requires end-to-end low latency
- Android SurfaceFlinger and macOS WindowServer use similar approaches

**Safety**:
- Only render thread elevated, not entire process
- Watchdog detects runaway loops and downgrades to CFS
- Priority 2 is below kernel interrupt handlers
- Uses priority inheritance to avoid priority inversion

### 1a. Audio Pipeline: Real-Time Scheduling

The audio server (PipeWire/sol-audio) uses **SCHED_FIFO priority 10** with stricter guarantees than compositor:

```rust
// Audio server threads
SCHED_FIFO priority 10  // Higher than compositor
Buffer size: 256-512 samples (5-11ms @ 48kHz)
Deadline: Must not miss or audio cracks/pops occur
```

**Rationale**:
- **Audio is more latency-sensitive than video** - 1 missed frame (16ms) is visible jank; 1 missed audio buffer (5ms) is audible crackling
- **Human ear is less forgiving** - Frame drop = slight stutter; audio dropout = jarring artifact
- **Small buffer = tight deadline** - Pro audio workflows use 256 samples (5ms), leaving almost no slack
- **Background tasks can starve audio** - CPU-bound app at 100% can delay audio thread past buffer deadline
- Priority 10 ensures audio preempts everything except kernel interrupts and hardware IRQ handlers

**Safety**:
- Audio thread is small and predictable (mix samples → DMA, no complex logic)
- Watchdog: If audio thread exceeds 50% of buffer period, log warning
- Priority inheritance for shared locks with clients
- Cgroup CPU guarantee: audio cgroup always gets ≥10% CPU even under load

**Thread breakdown**:
```
PipeWire/sol-audio process:
  - DSP/mixing thread: SCHED_FIFO priority 10
  - Client communication: SCHED_OTHER nice -5
  - Control/management: SCHED_OTHER nice 0
```

**Interaction with compositor**:
- Audio priority 10 > Compositor priority 2
- If system is overloaded, audio glitches are worse than frame drops
- Compositor can recover from missed frame; audio cannot recover from buffer underrun

### 2. Games: Resource Isolation, Not Real-Time Priority

Games receive **maximum resources through cgroup isolation**, not SCHED_FIFO:

```
Foreground game:
  - High CFS nice value (-10)
  - Dedicated cgroup with full CPU allocation
  - CPU governor → performance
  - All background apps frozen or throttled

Compositor:
  - SCHED_FIFO guarantees frame presentation
  
Result: Game gets maximum throughput, compositor guarantees smoothness
```

**Why not SCHED_FIFO for games?**

1. **Priority conflicts**: Two SCHED_FIFO tasks competing is harder to schedule than one FIFO + one CFS
2. **Security risk**: Any app could claim to be a "game" and request priority
3. **Unnecessary**: Games need throughput and resources, not preemption
4. **Total work matters**: If game render + compositor present > 16.67ms, no scheduling policy fixes it

### 3. Cgroup Hierarchy

> Linux cgroup v2 does not expose a `cpu.min` bandwidth controller. The
> percentages below are reservation targets; Phase 1 implements their
> contention protection with `cpu.weight`. `cpu.max`, `io.weight`, and
> `memory.min` are enforced directly by their kernel controllers.

```
/sys/fs/cgroup/
├── sol-compositor/          # SCHED_FIFO threads run here
│   ├── cpu.weight: 1000
│   ├── cpu.min: 10%         # Guaranteed minimum CPU even under load
│   └── [compositor process]
│
├── sol-audio/               # Audio server (PipeWire/sol-audio)
│   ├── cpu.weight: 1000
│   ├── cpu.min: 10%         # Guaranteed minimum CPU even under load
│   └── [audio server process]
│
├── sol-shell/               # Shell UI
│   ├── cpu.weight: 500
│   ├── cpu.min: 5%          # Guaranteed minimum for UI responsiveness
│   └── [sol-shell process]
│
├── sol-network/             # Network services
│   ├── cpu.weight: 1000
│   ├── cpu.min: 5%          # Guaranteed minimum for packet processing
│   ├── io.weight: 200       # High IO priority for config/state files
│   ├── memory.min: 64M      # Protected from swap pressure
│   └── [sol-networkd, systemd-resolved]
│
├── sol-system/              # System services
│   ├── cpu.weight: 800
│   ├── cpu.min: 20%         # Guaranteed minimum (includes network)
│   ├── io.weight: 100
│   └── [sol-settingsd, sol-notificationd, sol-portal, etc.]
│
├── sol-foreground/          # Active app (game or regular)
│   ├── cpu.weight: 1000
│   ├── cpu.max: unlimited
│   └── [foreground app]
│
├── sol-background/          # Background apps
│   ├── cpu.weight: 100
│   ├── cpu.max: 20,100000   # 20% throttle
│   ├── io.weight: 10
│   └── [background apps]
│
└── sol-build/               # Build processes (auto-detected)
    ├── cpu.weight: 100      # Low priority
    ├── cpu.max: 80,100000   # Cap at 80% total CPU to leave headroom
    ├── io.weight: 10        # Lowest IO priority
    └── [make, cargo, gcc, clang, rustc, ninja, meson, etc.]
```

### 4. Thread Priority Breakdown

| Component | Thread | Scheduling | Priority |
|-----------|--------|-----------|----------|
| Audio server | DSP/mixing | SCHED_FIFO | 10 |
| Compositor | Render/present | SCHED_FIFO | 2 |
| Compositor | Input dispatch | SCHED_FIFO | 1 |
| Shell | UI event loop | SCHED_FIFO | 1 |
| Compositor | Protocol handlers | SCHED_OTHER | 0 |
| Audio server | Client I/O | SCHED_OTHER | nice -5 |
| Network service | Event loop | SCHED_OTHER | nice -10 |
| DNS resolver | Main thread | SCHED_OTHER | nice -10 |
| settingsd | Main thread | SCHED_OTHER | nice -10 |
| portal | Main thread | SCHED_OTHER | nice -10 |
| notificationd | Main thread | SCHED_OTHER | nice -5 |
| Shell | Background tasks | SCHED_OTHER | nice -5 |
| Game (foreground) | All threads | SCHED_OTHER | nice -10 |
| Regular app (foreground) | All threads | SCHED_OTHER | nice 0 |
| Background apps | All threads | SCHED_OTHER | nice 10 |
| Build processes | All threads | SCHED_OTHER | nice 10 |

**Priority rationale:**
- Audio (10) > Compositor (2): Audio glitches are worse than frame drops
- Compositor render (2) > Input (1): Presenting frames is the hard deadline
- Shell UI (1) = Input (1): Top bar/dock must stay responsive under load
- Input (1) > Protocol handlers (0): Low-latency input dispatch critical
- Network/DNS/settingsd/portal (nice -10): System services must respond even under CPU saturation
- All RT threads < kernel IRQ handlers (typically priority 50-99)

### 5. Frame-Aligned Scheduling

Apps sync their render loop with the compositor's vsync signal to eliminate wasted frames and reduce latency:

```
Per-output frame timeline:
  - Compositor announces next vsync deadline via SCP
  - Apps schedule render to complete before deadline
  - Compositor presents on vsync with final composited frame
  
Benefits:
  - No missed vsyncs from late app renders
  - Lower power (apps sleep until next frame needed)
  - Predictable frame pacing for animations
```

**Multi-display handling:**
- Each output has independent refresh rate (e.g., 144Hz gaming monitor + 60Hz secondary)
- Apps render at their primary output's rate
- Compositor re-presents to secondary outputs at their native rate
- No forced synchronization across displays

### 6. Workload Detection and Adaptation

SOL detects application workload characteristics and adjusts scheduling dynamically:

**Application Type Detection:**
```rust
enum WorkloadProfile {
    Game,           // High GPU/CPU, consistent frame timing critical
    VideoEditor,    // High IO throughput, GPU acceleration
    IDE,            // Bursty CPU, low latency typing
    Browser,        // Mixed workload, tab isolation
    Background,     // Minimal resources
}
```

**Automatic classification based on:**
- GPU usage patterns (consistent high usage = game)
- Input event frequency (high = interactive)
- Frame submission rate (constant = game/video)
- Process hints via SCP metadata

**Per-profile scheduling:**
```
Game profile:
  - CPU governor → performance
  - GPU clock boost
  - Background apps frozen
  - Compositor render budget increased for complex scenes

Video editor profile:
  - IO scheduler → deadline
  - Cache pressure relaxed
  - High memory limits

IDE profile:
  - Input latency minimized
  - CPU governor → schedutil (balance)
  - Fast wakeup from idle
```

### 7. Interaction-Aware Priority Boosting

The compositor tracks user interaction and temporarily boosts related processes:

```rust
// Input event triggers priority boost
on_input_event(event) {
    let target_window = hit_test(event.position);
    boost_priority(target_window.app_pid, duration: 100ms);
}

// Boost applies to:
// - Nice value: -5 for next 100ms
// - CPU governor: performance (if on battery, limit to 3 boosts/sec)
// - IO priority: realtime class
```

**Use cases:**
- Typing in terminal: Instant character echo
- Scrolling browser: Smooth 120fps
- Dragging window: No lag during movement
- Game input: Minimum input → render → present latency

**Decay:**
- Boost decays linearly over 100ms
- Repeated input events extend the boost
- Idle for >500ms: full decay back to baseline priority

### 8. Rapid Burst for Cold Starts

When an app launches (cold start only), SOL provides a **1-second performance burst** to accelerate initialization:

```rust
on_app_launch(app_pid) {
    if !is_warm_start(app_pid) {
        // Rapid Burst: 1 second maximum
        apply_rapid_burst(app_pid, max_duration: 1000ms);
    }
}

fn apply_rapid_burst(pid: u32, max_duration: Duration) {
    // 1. CPU frequency boost
    set_cpu_governor("performance");
    
    // 2. Priority elevation
    set_nice(pid, -10);  // Same as foreground game
    
    // 3. IO priority boost
    set_ionice(pid, IOPriority::Realtime);
    
    // 4. Automatic decay after 1s or first frame presented
    let decay_trigger = min(
        max_duration,
        wait_for_first_frame(pid)
    );
    
    schedule_decay(decay_trigger, || {
        set_cpu_governor("schedutil");
        set_nice(pid, 0);
        set_ionice(pid, IOPriority::BestEffort);
    });
}
```

**Rationale:**
- Shell knows app launch is coming (user clicked launcher)
- CPU governor is reactive (sees load after it happens)
- Cold start is the most latency-sensitive moment (user waiting)
- First impression matters: 0.5s launch feels instant, 2s launch feels slow

**Safety limits:**
- **Cold start only** - Warm restarts don't get the burst (app state already in memory/cache)
- **1 second hard cap** - Prevents runaway resource usage
- **Early termination** - Burst ends immediately when first frame is presented (app is visibly ready)
- **Battery awareness** - On battery <30%, burst limited to 500ms

**Detection of cold vs warm start:**
```rust
fn is_warm_start(app_pid: u32) -> bool {
    // Warm start indicators:
    // 1. Process already running (background → foreground)
    // 2. Launched within last 60s (cache still hot)
    // 3. Memory pages still resident (check /proc/[pid]/smaps)
    
    process_exists(app_pid) 
        || recently_launched(app_pid, within: 60s)
        || memory_resident_ratio(app_pid) > 0.8
}
```

**Example scenario:**
```
User clicks "Files" app in launcher:
  T+0ms:   Shell signals launch to compositor
  T+1ms:   Compositor applies Rapid Burst to Files app PID
  T+1ms:   CPU frequency ramps to max (governor → performance)
  T+50ms:  Files binary loads, parses config, initializes UI
  T+120ms: First frame rendered and presented
  T+120ms: Rapid Burst terminates early (first frame detected)
  T+121ms: CPU governor returns to schedutil
  
Result: App feels instant, 880ms of burst time saved
```

**Abuse prevention:**
- Apps cannot request burst themselves (only shell-initiated launch)
- Crash-loop detection: App crashing within burst window disqualifies it from burst on next launch
- Rate limiting: Same app max 1 burst per 5 seconds (prevents restart spam)

### 9. Security and Capability Model

Scheduling adapts to system state:

**Battery state:**
```
AC powered:
  - Full performance, no throttling
  - Background apps limited to 20% CPU

On battery (>30%):
  - CPU governor → schedutil
  - Background apps limited to 10% CPU
  - Compositor SCHED_FIFO maintained

On battery (<30%):
  - Background apps frozen (except critical services)
  - Compositor framerate target → 60Hz (even on 120Hz display)
  - GPU clock limited
```

**Thermal state:**
```
Normal (<70°C):
  - No throttling

Warm (70-85°C):
  - Background app limit → 5% CPU
  - Game nice value reduced from -10 to -5

Hot (>85°C):
  - Force 60fps frame cap (even for games)
  - Background apps frozen
  - Reduce compositor SCHED_FIFO priority to SCHED_OTHER
```

### 9. System Component Protection (Anti-Starvation)

Ensure system components remain responsive even when user processes consume 100% CPU/IO/memory.

**OOM protection (oom_score_adj):**
```
sol-compositor:     -900  # Never kill compositor
sol-audio:          -900  # Never kill audio
sol-networkd:       -800  # Network stack is critical
systemd-resolved:   -800  # DNS must work
sol-shell:          -800  # UI must respond
sol-settingsd:      -500  # System services
sol-portal:         -500
sol-notificationd:  -500
apps (foreground):  0     # Default, can be killed
apps (background):  100   # Kill background apps first
```

**IO priority (ionice):**
```
Compositor:  ionice -c1 -n0  # Realtime IO, highest priority
Audio:       ionice -c1 -n0  # Realtime IO
Shell:       ionice -c2 -n0  # Best-effort, highest
Network:     ionice -c2 -n1  # Best-effort, high (config/state files)
DNS:         ionice -c2 -n1  # Best-effort, high
System:      ionice -c2 -n2  # Best-effort, medium
Apps (fg):   ionice -c2 -n4  # Best-effort, normal
Apps (bg):   ionice -c3      # Idle class
Build:       ionice -c3      # Idle class (don't block system IO)
```

**Build process detection and containment:**

When system detects build processes (make, cargo, gcc, clang, rustc, ninja, meson, cmake), automatically move them to `sol-build/` cgroup:

```
Rationale:
  - Build processes are CPU/IO intensive but not latency-sensitive
  - Users tolerate slower builds, not slower UI
  - Cap at 80% total CPU to leave headroom for system/UI
  - Idle IO class prevents compile from blocking file manager, settings, etc.

Detection heuristics:
  - Process name matches: make, cargo, gcc, g++, clang, rustc, ninja, meson, cmake, etc.
  - High CPU + high IO + many child processes
  - Parent process is a build tool
```

**Network stack protection:**

Network services must remain responsive during high system load (e.g., compilation):

```
Why network needs protection:
  - DNS queries block app launches and web browsing
  - Connection establishment delays are user-visible
  - Background disk IO can starve network config reads/writes
  - Page cache pollution from builds evicts hot network data

Protection mechanisms:
  - sol-networkd/systemd-resolved: nice -10 (preempt build processes)
  - Dedicated sol-network/ cgroup with cpu.min: 5%
  - IO priority: best-effort high (ionice -c2 -n1)
  - Memory protection: memory.min: 64M (prevent swap-out)
```

### 10. Security and Capability Model

Applications cannot directly request elevated scheduling:

**Portal-based authorization:**
```
App requests "gaming-mode" capability via sol-portal:
  1. Portal checks app signature + user approval
  2. If approved: Portal sets cgroup + profile
  3. App never gets direct SCHED_FIFO access
```

**Watchdog protection:**
```rust
// Compositor render thread watchdog
const FRAME_BUDGET_60HZ: Duration = Duration::from_micros(16_670);

if render_time > FRAME_BUDGET_60HZ * 1.5 {
    // Automatic downgrade if compositor misbehaves
    set_scheduler(SCHED_OTHER);
    log_error!("Compositor exceeded frame budget, downgraded to CFS");
}
```

**Fairness guarantees:**
- Background apps always get ≥5% CPU (prevents starvation)
- Critical system services exempt from freezing
- Memory OOM killer respects foreground priority

## Alternative Considered

### Custom Kernel Scheduler
**Rejected** - Maintaining a kernel fork is expensive and conflicts with SOL's goal of using upstream Linux. Scheduling policy can be implemented in userspace.

### SCHED_DEADLINE for Compositor
**Deferred** - SCHED_DEADLINE provides stronger guarantees (admit or reject based on bandwidth), but requires more complex setup and bandwidth accounting. SCHED_FIFO + watchdog is simpler for Phase 1. Revisit in Phase 2 if frame timing analysis shows deadline misses.

### SCHED_FIFO for Games
**Rejected** - See reasoning above. Games get resources, not priority. Compositor always wins frame deadlines. Two SCHED_FIFO tasks competing creates unpredictable scheduling behavior.

### BPF Schedulers (sched_ext)
**Future work** - Linux 6.6+ supports BPF-based schedulers. Highly interesting for custom policies (e.g., frame-time variance minimization, latency-sensitive thread detection), but adds complexity. Revisit in Phase 2+ after collecting real-world scheduling metrics.

### Windows DWM-style Multimedia Class Scheduler
**Partial adoption** - Windows MMCSS provides application-requested priority boosts for multimedia tasks. SOL achieves similar results via workload detection + portal authorization, without allowing arbitrary app priority requests (security concern).

### macOS QoS Classes
**Inspiration** - macOS provides 4 QoS tiers (User Interactive, User Initiated, Utility, Background) that apps explicitly declare. SOL's workload profiles are similar but inferred rather than declared, reducing API surface and preventing misuse.

## Implementation Plan

### Phase 1: Core Scheduling Foundation
**Target: Compositor responsiveness + basic resource isolation**

1. **Compositor real-time scheduling**
   - Set SCHED_FIFO priority 2 for render/present thread
   - Set SCHED_FIFO priority 1 for input dispatch thread
   - Implement watchdog: downgrade to SCHED_OTHER if frame budget exceeded
   - Add frame timing telemetry (present timestamps, missed vsyncs)

2. **Audio server real-time scheduling**
   - Set SCHED_FIFO priority 10 for PipeWire/sol-audio DSP thread
   - Buffer size: 256-512 samples (5-11ms @ 48kHz)
   - Watchdog: warn if thread exceeds 50% of buffer period
   - Cgroup CPU guarantee: audio slice gets ≥10% CPU minimum

3. **Shell UI real-time scheduling**
   - Set SCHED_FIFO priority 1 for UI event loop
   - Keep background tasks at SCHED_OTHER nice -5
   - Ensures top bar, dock, launcher stay responsive under load

4. **Cgroup hierarchy setup**
   - Create cgroup structure at boot (systemd units)
   - Implement process mover: assign apps to correct cgroup on launch
   - Basic foreground/background distinction
   - CPU/IO weight enforcement
   - Audio slice with guaranteed minimum CPU allocation
   - Compositor/shell/network slices with cpu.min guarantees

5. **System component protection**
   - Set OOM scores: compositor/audio (-900), network (-800), shell (-800), system services (-500)
   - Set IO priorities: compositor/audio (realtime), network/system (best-effort high)
   - Network services: nice -10, dedicated cgroup with cpu.min: 5%

6. **Build process containment**
   - Detect build processes (make, cargo, gcc, clang, rustc, ninja, meson, cmake)
   - Auto-move to sol-build/ cgroup
   - Cap at 80% total CPU, idle IO class
   - Prevents compilation from dragging down UI/network

7. **Input latency optimization**
   - Priority inheritance for input → app pipeline
   - Measure input event → frame present latency

8. **CPU governor management**
   - Set performance governor when game detected
   - Revert to schedutil on game exit

**Deliverable**: Compositor maintains 60fps under load, audio never glitches under CPU saturation, input latency <10ms, UI stays responsive during compilation, network latency stable under disk IO load

### Phase 2: Workload Awareness and Optimization
**Target: Dynamic adaptation to app types**

1. **Workload detection system**
   - Implement WorkloadProfile classification
   - Heuristics: GPU usage, input frequency, frame submission rate
   - Allow apps to hint workload type via SCP metadata
   - Profile persistence across launches (remember app types)

2. **Per-profile scheduling policies**
   - Game profile: freeze background, performance governor, boost GPU clocks
   - Video editor profile: IO priority, high memory limits
   - IDE profile: fast idle wakeup, balanced power

3. **Rapid Burst for cold starts**
   - 1-second performance burst on app launch (cold start only)
   - **CPU boost**: governor → performance, nice -10, IO realtime class
   - **GPU boost**: governor → performance (AMD: `power_dpm_force_performance_level=high`), min_freq = max_freq
   - Early termination on first frame presented (CPU+GPU+IO restore together)
   - Crash-loop and rate-limit protection (max 3 bursts per 10 seconds per app)
   - **Battery-aware**: 500ms cap + skip GPU boost when <30% battery
   - **Thermal-aware**: Skip GPU boost in hot zone (>80°C)

4. **Frame-aligned scheduling**
   - Compositor announces vsync deadline via SCP protocol
   - Apps schedule render completion before deadline
   - Collect metrics on frame alignment effectiveness

5. **Multi-display independent refresh rates**
   - Per-output vsync signaling
   - Apps render at primary output rate
   - Compositor re-present to secondary outputs at native rates

6. **Interaction-aware priority boosting**
   - Input event → 100ms priority boost for target app
   - Nice value adjustment: -5 during interaction
   - Boost decay and extension on repeated input

**Deliverable**: Games show reduced frame-time variance, typing latency <5ms, smooth scrolling at 120fps, app launches feel instant (<500ms perceived latency)

### Phase 3: Advanced Power Management
**Target: Battery life without sacrificing responsiveness**

1. **Battery-aware scheduling**
   - Implement three-tier battery policy (AC / battery >30% / battery <30%)
   - Aggressive background app freezing on low battery
   - Framerate target reduction: 120Hz → 60Hz when <30% battery
   - Maintain compositor SCHED_FIFO even on battery (responsiveness matters)

2. **Thermal throttling**
   - Temperature monitoring via hwmon
   - Three thermal zones: normal / warm / hot
   - Progressive throttling: background → foreground → compositor
   - Frame rate capping at high temps

3. **Idle optimization**
   - Coalesce wakeups across apps
   - Extend vsync intervals when no content changes
   - Deep sleep for background apps (>5min idle)

**Deliverable**: 20% battery life improvement vs naive scheduling, no thermal throttling under normal workloads

### Phase 4: Observability and Tuning
**Target: Production debugging and performance analysis**

1. **Scheduling metrics dashboard**
   - Frame timing histograms (p50/p95/p99)
   - Input latency distribution
   - Per-app CPU/GPU usage
   - Cgroup resource utilization

2. **Tracing infrastructure**
   - eBPF probes for scheduling events
   - Ftrace integration for kernel scheduler visibility
   - Flamegraphs for CPU time attribution

3. **Adaptive tuning**
   - Machine learning on frame timing patterns (optional)
   - Per-app profile refinement based on collected data
   - A/B testing framework for policy changes

**Deliverable**: Production telemetry, ability to diagnose frame drops and latency spikes

### Phase 5: Advanced Features (Future)
**Target: Cutting-edge optimizations**

1. **BPF custom schedulers (sched_ext)**
   - Experiment with frame-time variance minimization
   - Game-specific policies (e.g., prioritize render thread over audio)
   - Userspace scheduler for non-critical tasks

2. **SCHED_DEADLINE for certified apps**
   - Portal-authorized deadline scheduling for pro audio/video apps
   - Bandwidth accounting and admission control
   - Stricter guarantees than SCHED_FIFO

3. **Predictive wakeup**
   - Learn app frame pacing patterns
   - Wake render threads ahead of predicted need
   - Reduce pipeline stalls

4. **Per-app scheduling profiles**
   - User-configurable policies ("always prioritize this app")
   - App store metadata: "this is a game" → auto-profile
   - Developer API to declare workload characteristics

## Consequences

### Positive
- **Guaranteed compositor responsiveness** - Frame timing independent of app load
- **Guaranteed audio reliability** - Zero buffer underruns even under CPU saturation
- **Game performance** - Maximum resources without security risk of RT priority
- **Instant app launches** - Rapid Burst makes cold starts feel <500ms
- **GPU launch boost** - First frame renders faster, critical for perceived responsiveness
- **Clear hierarchy** - Easy to reason about priority model (audio > compositor > input > apps)
- **No kernel fork** - Pure userspace policy on upstream Linux
- **Proven approach** - Aligns with Android/ChromeOS/macOS production systems
- **Power efficiency** - Aggressive background throttling saves battery
- **Multi-display flexibility** - Independent refresh rates per output
- **Interaction responsiveness** - Priority boost during active user input
- **Thermal safety** - Automatic throttling prevents overheating
- **Observability** - Rich metrics for debugging performance issues
- **First impression wins** - Apps feel fast at the moment users care most (launch)
- **Audio prioritization** - Human ear is less forgiving than eye; audio glitches are worse than frame drops
- **System protection** - UI/network stay responsive even during compilation
- **Build containment** - Compile jobs don't drag down interactive performance
- **Network stability** - DNS/networking remain fast under disk IO saturation
- **OOM resilience** - Critical components protected from memory pressure

### Negative
- **Complexity** - More moving parts than simple CFS
- **SCHED_FIFO risk** - Compositor/audio bugs could freeze system (mitigated by watchdog)
- **Cgroup overhead** - Process migration cost on window focus
- **Tuning required** - Optimal weights/thresholds need real-world data
- **Background app impact** - Apps may feel sluggish when game active (intentional trade-off)
- **Detection accuracy** - Workload classification heuristics may misidentify apps
- **Memory footprint** - Telemetry and tracing infrastructure adds overhead
- **Rapid Burst abuse potential** - Apps crash-looping could waste power (mitigated by rate limiting)
- **Cold vs warm detection complexity** - Heuristics may incorrectly classify starts
- **GPU boost power draw** - Short-term power spike during burst (acceptable for <1s)
- **Audio RT priority higher than compositor** - Audio bugs are now more critical (smaller, simpler code mitigates this)
- **Build detection overhead** - Process monitoring adds CPU cost
- **False positives** - Non-build processes may get misclassified and throttled
- **Network cgroup overhead** - Extra memory for per-service isolation

### Neutral
- **Platform OS approach** - We control the stack, so we can make these guarantees
- **Not backward-compatible** - Arbitrary Linux apps may not behave well (but SOL isn't trying to support them)
- **Power/performance trade-off** - Users on battery accept reduced performance for longer runtime
- **Portal authorization required** - Apps can't self-elevate (security wins, but adds friction for legitimate use cases)
- **Burst only on cold start** - Warm restarts don't get boost (acceptable since they're already fast)
- **GPU governor platform-specific** - AMD/Intel/NVIDIA have different control interfaces (need per-platform code)
- **Audio latency vs throughput** - Small buffers (low latency) require more frequent scheduling, higher overhead
- **Build throttling visibility** - Users may wonder why compilation is "slow" (actually: system is responsive)
- **Network cgroup isolates issues** - Buggy network service can't impact other system components (good isolation, harder debugging)

## Verification

Success criteria:
1. **Frame timing consistency** - Compositor maintains 60fps (16.67ms frame time) under game load, <1% frame drops
2. **Audio reliability** - Zero audio glitches (buffer underruns) even with CPU-bound apps at 100% load
3. **Input latency** - <10ms mouse/keyboard event → frame present latency (measured end-to-end)
4. **Game frame-time variance** - Reduced jitter compared to default CFS scheduler (measure with frame-time graphs)
5. **Background isolation** - Frozen background apps consume <1% CPU
6. **Multi-display independence** - 144Hz primary + 60Hz secondary without forced sync
7. **Interaction boost effectiveness** - Typing latency <5ms during active editing
8. **Power efficiency** - 15-20% longer battery life vs naive scheduling
9. **Thermal stability** - No thermal throttling during typical workloads (<85°C sustained)
10. **Cold start performance** - Apps launch in <500ms perceived time (with Rapid Burst vs >1s without)
11. **Burst efficiency** - 80%+ of bursts terminate early on first frame (not hitting 1s cap)
12. **Audio thread budget** - DSP thread uses <30% of buffer period on average
13. **UI responsiveness under compilation** - Compositor maintains 60fps, input <10ms latency even during `cargo build -j32`
14. **Network latency under load** - DNS queries <50ms, ping latency stable (±5ms) during heavy disk IO
15. **Build containment effectiveness** - Build processes use <80% total CPU, idle IO class confirmed
16. **System service protection** - settingsd/portal respond <100ms even under 100% CPU load

Metrics to collect:
- **Frame presentation timestamps** - Vsync hits/misses, frame time distribution
- **Audio buffer metrics** - Buffer fill level, underruns, xruns, DSP thread execution time
- **Input event latency** - Event timestamp → frame present timestamp delta
- **CPU scheduling latency** - Trace with ftrace/bpftrace (wakeup → running time)
- **Game benchmarks** - Frame-time graphs (0.1%/1%/avg), not just average FPS
- **Per-app resource usage** - CPU time, context switches, IO bandwidth
- **Cgroup statistics** - Throttled time, CPU pressure, memory pressure
- **Power consumption** - Battery drain rate per workload profile
- **Thermal data** - CPU/GPU temperature over time under load
- **App launch timing** - Cold start time (click → first frame), burst duration, early termination rate
- **Rapid Burst effectiveness** - Distribution of burst durations, percentage hitting 1s cap vs early termination, GPU frequency during burst
- **Cold vs warm detection accuracy** - Manual validation against heuristic classification
- **Audio-specific metrics** - JACK/PipeWire xrun reports, DSP load percentage, client wakeup jitter
- **UI responsiveness during builds** - Frame time distribution, input latency during compilation
- **Network latency under load** - DNS query time, ping latency, connection establishment time during disk IO
- **Build process metrics** - CPU usage distribution, cgroup throttle events, IO wait time
- **System service response time** - D-Bus method call latency for settingsd/portal/notificationd
- **OOM events** - Which processes killed, memory pressure at time of kill

Tools:
- `ftrace` - Kernel scheduler events
- `bpftrace`/`bcc` - Custom scheduling probes
- `perf` - CPU profiling and event counting
- Frame timing overlay in compositor (built-in)
- Power monitoring: `powertop`, `turbostat`
- Thermal: `sensors`, hwmon sysfs

## References

- **Android SurfaceFlinger scheduling**: SCHED_FIFO priority 2, cgroup foreground/background isolation
  - <https://source.android.com/docs/core/graphics/surfaceflinger-windowmanager>
- **macOS WindowServer**: Elevated priority in Darwin scheduler, QoS classes (User Interactive → Background)
  - <https://developer.apple.com/library/archive/documentation/Performance/Conceptual/power_efficiency_guidelines_osx/>
- **Steam Deck GameScope**: Compositor priority + performance governor + AMD GPU optimizations
  - <https://github.com/ValveSoftware/gamescope>
- **Windows DWM**: Multimedia Class Scheduler Service (MMCSS), dynamic priority boosting
  - <https://docs.microsoft.com/en-us/windows/win32/procthread/multimedia-class-scheduler-service>
- **Linux SCHED_DEADLINE**: Earliest Deadline First with bandwidth accounting
  - <https://www.kernel.org/doc/html/latest/scheduler/sched-deadline.html>
- **BPF schedulers (sched_ext)**: Userspace-defined scheduling policies via eBPF
  - <https://lwn.net/Articles/922405/>
  - <https://github.com/sched-ext/scx>
- **ChromeOS scheduling**: Real-time compositor, cgroup resource limits, per-app profiles
  - ChromeOS Architecture Documentation (internal)
- **Linux cgroup v2**: CPU, memory, IO controllers
  - <https://www.kernel.org/doc/html/latest/admin-guide/cgroup-v2.html>
- **Frame pacing and latency**: Research on compositor timing
  - "Understanding and Improving Frame Pacing in Wayland" (XDC 2019)
  - <https://www.collabora.com/news-and-blog/blog/2020/05/28/latency-in-wayland/>

## Related ADRs

- **ADR-0028**: No Wayland compatibility layer (SOL is a platform OS, SCP only)
- **ADR-0006**: Shell/compositor separation and IPC (crash safety, D-Bus communication)
- **ADR-0001**: Capability-based security model (apps cannot self-elevate scheduling priority)
