//! cgroup v2 hierarchy ownership and build-process containment.

use std::{
    collections::HashSet,
    fs::{self, OpenOptions},
    io::{self, Write},
    path::{Path, PathBuf},
};

use crate::{ApplyReport, ProcessClass, cgroup_profiles, linux::apply_process_controls};

const CONTROLLERS: &str = "+cpu +io +memory";

/// The provisioned SOL cgroup v2 hierarchy.
#[derive(Debug, Clone)]
pub struct CgroupHierarchy {
    root: PathBuf,
}

impl CgroupHierarchy {
    #[must_use]
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    #[must_use]
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Create all policy leaves and configure every available controller.
    pub fn provision(&self) -> io::Result<()> {
        if let Some(parent) = self.root.parent() {
            enable_controllers_if_present(parent)?;
        }
        fs::create_dir_all(&self.root)?;
        enable_controllers_if_present(&self.root)?;
        let kernel_cgroup = self.root.join("cgroup.procs").exists();

        for profile in cgroup_profiles() {
            let leaf = self.root.join(profile.name);
            fs::create_dir_all(&leaf)?;
            write_profile_control(&leaf.join("cpu.weight"), profile.cpu_weight, kernel_cgroup)?;
            let cpu_max = profile
                .cpu_limit
                .map_or_else(|| "max 100000".to_owned(), |limit| limit.as_cgroup_value());
            write_profile_control(&leaf.join("cpu.max"), cpu_max, kernel_cgroup)?;
            write_profile_control(
                &leaf.join("io.weight"),
                format!("default {}", profile.io_weight),
                kernel_cgroup,
            )?;
            if let Some(bytes) = profile.memory_min_bytes {
                write_profile_control(&leaf.join("memory.min"), bytes, kernel_cgroup)?;
            }
        }
        Ok(())
    }

    /// Atomically migrate a process between class leaves through cgroup.procs.
    pub fn move_process(&self, pid: u32, class: ProcessClass) -> io::Result<()> {
        write_control(
            &self.root.join(class.cgroup_name()).join("cgroup.procs"),
            pid,
        )
    }
}

fn enable_controllers_if_present(directory: &Path) -> io::Result<()> {
    let available = directory.join("cgroup.controllers");
    let subtree = directory.join("cgroup.subtree_control");
    if !available.exists() && !subtree.exists() {
        return Ok(());
    }
    write_control(&subtree, CONTROLLERS)
}

fn write_control(path: &Path, value: impl std::fmt::Display) -> io::Result<()> {
    let mut file = OpenOptions::new()
        .write(true)
        .create(!path.exists())
        .truncate(true)
        .open(path)?;
    file.write_all(value.to_string().as_bytes())
}

fn write_profile_control(
    path: &Path,
    value: impl std::fmt::Display,
    kernel_cgroup: bool,
) -> io::Result<()> {
    if kernel_cgroup && !path.exists() {
        return Ok(());
    }
    write_control(path, value)
}

/// Result of one build-process scan.
#[derive(Debug, Default)]
pub struct BuildContainment {
    pub moved: Vec<u32>,
    pub failures: Vec<(u32, io::Error)>,
}

/// High-level owner used by `sol-init`.
pub struct SchedulingManager {
    hierarchy: CgroupHierarchy,
    proc_root: PathBuf,
    contained_builds: HashSet<(u32, u64)>,
}

impl SchedulingManager {
    #[must_use]
    pub fn new(cgroup_root: impl Into<PathBuf>) -> Self {
        Self::with_proc_root(cgroup_root, "/proc")
    }

    #[must_use]
    pub fn with_proc_root(cgroup_root: impl Into<PathBuf>, proc_root: impl Into<PathBuf>) -> Self {
        Self {
            hierarchy: CgroupHierarchy::new(cgroup_root),
            proc_root: proc_root.into(),
            contained_builds: HashSet::new(),
        }
    }

    pub fn provision(&self) -> io::Result<()> {
        self.hierarchy.provision()
    }

    /// Apply cgroup, CPU nice, OOM, and I/O policy independently.
    #[must_use]
    pub fn apply(&self, pid: u32, class: ProcessClass) -> ApplyReport {
        let mut report = apply_process_controls(&self.proc_root, pid, class);
        report.capture("cgroup", self.hierarchy.move_process(pid, class));
        report
    }

    /// Detect known compiler/build executables and contain newly observed
    /// process generations. PID start time prevents PID-reuse mistakes.
    pub fn contain_build_processes(&mut self) -> io::Result<BuildContainment> {
        let mut containment = BuildContainment::default();
        for entry in fs::read_dir(&self.proc_root)? {
            let entry = match entry {
                Ok(entry) => entry,
                Err(_) => continue,
            };
            let Some(pid) = entry
                .file_name()
                .to_str()
                .and_then(|name| name.parse::<u32>().ok())
            else {
                continue;
            };
            let comm = match fs::read_to_string(entry.path().join("comm")) {
                Ok(comm) => comm,
                Err(_) => continue,
            };
            if !is_build_tool(comm.trim()) {
                continue;
            }
            let start_time = match process_start_time(&entry.path().join("stat")) {
                Ok(start_time) => start_time,
                Err(_) => continue,
            };
            if !self.contained_builds.insert((pid, start_time)) {
                continue;
            }
            let report = self.apply(pid, ProcessClass::Build);
            if report.is_success() {
                containment.moved.push(pid);
            } else {
                for failure in report.failures {
                    containment.failures.push((pid, failure.error));
                }
            }
        }
        Ok(containment)
    }
}

fn process_start_time(stat_path: &Path) -> io::Result<u64> {
    let stat = fs::read_to_string(stat_path)?;
    let end_of_comm = stat.rfind(')').ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            "proc stat has no command terminator",
        )
    })?;
    // Fields after the command start at field 3 (`state`); starttime is field
    // 22, therefore index 19 in this suffix.
    stat[end_of_comm + 1..]
        .split_whitespace()
        .nth(19)
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "proc stat is truncated"))?
        .parse()
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))
}

fn is_build_tool(command: &str) -> bool {
    matches!(
        command,
        "make"
            | "gmake"
            | "cargo"
            | "rustc"
            | "gcc"
            | "g++"
            | "cc"
            | "c++"
            | "clang"
            | "clang++"
            | "ninja"
            | "meson"
            | "cmake"
            | "ld"
            | "lld"
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_root(label: &str) -> PathBuf {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!(
            "sol-scheduler-{label}-{}-{unique}",
            std::process::id()
        ))
    }

    #[test]
    fn provisions_phase_one_hierarchy() {
        let parent = temp_root("hierarchy");
        let root = parent.join("sol");
        let hierarchy = CgroupHierarchy::new(&root);
        hierarchy.provision().unwrap();

        assert_eq!(
            fs::read_to_string(root.join("sol-compositor/cpu.weight")).unwrap(),
            "1000"
        );
        assert_eq!(
            fs::read_to_string(root.join("sol-background/cpu.max")).unwrap(),
            "20000 100000"
        );
        assert_eq!(
            fs::read_to_string(root.join("sol-network/memory.min")).unwrap(),
            (64 * 1024 * 1024).to_string()
        );
        fs::remove_dir_all(parent).unwrap();
    }

    #[test]
    fn recognizes_build_tools_without_substring_false_positives() {
        assert!(is_build_tool("cargo"));
        assert!(is_build_tool("clang++"));
        assert!(!is_build_tool("cargo-watchdog"));
        assert!(!is_build_tool("maker"));
    }

    #[test]
    fn parses_start_time_when_command_contains_spaces() {
        let root = temp_root("stat");
        fs::create_dir_all(&root).unwrap();
        let stat = root.join("stat");
        let mut fields = vec!["S"; 20];
        fields[19] = "98765";
        fs::write(&stat, format!("42 (build worker) {}", fields.join(" "))).unwrap();
        assert_eq!(process_start_time(&stat).unwrap(), 98_765);
        fs::remove_dir_all(root).unwrap();
    }
}
