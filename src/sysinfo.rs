//! The system variables a text block can show: the static ones, which the
//! system info cache holds, and the dynamic ones, read fresh at every banner.
//!
//! Dynamic values start no process and read only kernel interfaces, so a
//! banner stays fast and works with an empty `PATH`. Static values may run
//! system tools, as they are only resolved when the cache is written.

use std::fmt;

/// The value of a variable that cannot be found.
pub const UNKNOWN: &str = "unknown";

/// A variable whose value stays put between banners, so the system info cache
/// holds it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StaticVariable {
    /// `{hostname}`: the host name.
    Hostname,
    /// `{os}`: the OS name and version.
    Os,
    /// `{cpu}`: the CPU model.
    Cpu,
    /// `{gpu}`: the first graphics adapter's name.
    Gpu,
}

impl StaticVariable {
    /// Every static variable.
    pub const ALL: [Self; 4] = [Self::Hostname, Self::Os, Self::Cpu, Self::Gpu];

    /// The name a template writes between braces.
    pub const fn name(self) -> &'static str {
        match self {
            Self::Hostname => "hostname",
            Self::Os => "os",
            Self::Cpu => "cpu",
            Self::Gpu => "gpu",
        }
    }

    /// The static variable whose [`name`](Self::name) is `name`, if any.
    pub fn from_name(name: &str) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|variable| variable.name() == name)
    }

    /// The variable's value on this machine, [`UNKNOWN`] when it cannot be
    /// found. It may run system tools, which can take a few seconds.
    pub fn resolve(self) -> String {
        known(match self {
            Self::Hostname => platform::hostname(),
            Self::Os => platform::os(),
            Self::Cpu => platform::cpu(),
            Self::Gpu => platform::gpu(),
        })
    }
}

/// A variable whose value changes between banners, so each banner reads it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DynamicVariable {
    /// `{ram}`: the memory used of the total, as [`format_usage`] shows it.
    Ram,
    /// `{disk}`: the space used of the total on the system volume, as
    /// [`format_usage`] shows it.
    Disk,
    /// `{uptime}`: the time since boot, as [`format_uptime`] shows it.
    Uptime,
}

impl DynamicVariable {
    /// Every dynamic variable.
    pub const ALL: [Self; 3] = [Self::Ram, Self::Disk, Self::Uptime];

    /// The name a template writes between braces.
    pub const fn name(self) -> &'static str {
        match self {
            Self::Ram => "ram",
            Self::Disk => "disk",
            Self::Uptime => "uptime",
        }
    }

    /// The variable's value now, [`UNKNOWN`] when it cannot be found. It
    /// starts no process.
    pub fn resolve(self) -> String {
        known(match self {
            Self::Ram => platform::memory().map(|(used, total)| format_usage(used, total)),
            Self::Disk => platform::disk().map(|(used, total)| format_usage(used, total)),
            Self::Uptime => platform::uptime_seconds().map(format_uptime),
        })
    }
}

/// `value`, or [`UNKNOWN`] when it is missing or blank.
fn known(value: Option<String>) -> String {
    value
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| UNKNOWN.to_owned())
}

/// An uptime of `seconds` as `<d>d <h>h <m>m` in whole minutes, leaving out
/// the leading units that are zero: 273900 seconds is `3d 4h 5m`, 3600 is
/// `1h 0m` and 300 is `5m`.
pub fn format_uptime(seconds: u64) -> String {
    const MINUTE: u64 = 60;
    const HOUR: u64 = 60 * MINUTE;
    const DAY: u64 = 24 * HOUR;

    let days = seconds / DAY;
    let hours = seconds % DAY / HOUR;
    let minutes = seconds % HOUR / MINUTE;
    if days > 0 {
        format!("{days}d {hours}h {minutes}m")
    } else if hours > 0 {
        format!("{hours}h {minutes}m")
    } else {
        format!("{minutes}m")
    }
}

/// `used` bytes of `total` as `<used> GiB / <total> GiB`, each with one
/// decimal.
pub fn format_usage(used: u64, total: u64) -> String {
    format!("{} GiB / {} GiB", Gib(used), Gib(total))
}

/// A size in bytes shown in GiB (2^30 bytes) with one decimal, rounded half
/// away from zero.
struct Gib(u64);

impl fmt::Display for Gib {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        const SHIFT: u32 = 30;
        let tenths = (u128::from(self.0) * 10 + (1 << (SHIFT - 1))) >> SHIFT;
        write!(f, "{}.{}", tenths / 10, tenths % 10)
    }
}

/// The trimmed, non-empty value of the first line of `text` that reads
/// `key<separator>value`, the key trimmed too.
#[cfg(any(target_os = "macos", target_os = "linux", test))]
fn field<'a>(text: &'a str, key: &str, separator: char) -> Option<&'a str> {
    text.lines().find_map(|line| {
        let (name, value) = line.split_once(separator)?;
        let value = value.trim();
        (name.trim() == key && !value.is_empty()).then_some(value)
    })
}

/// The first `Model name` in the output of `lscpu`, which prints one per core
/// cluster and `-` for a core it cannot name.
#[cfg(any(target_os = "linux", test))]
fn lscpu_model(output: &str) -> Option<&str> {
    output
        .lines()
        .filter_map(|line| field(line, "Model name", ':'))
        .find(|name| *name != "-")
}

/// The standard output of `program` run with `args` in the C locale, so its
/// labels are in English, with stdin closed and stderr dropped, or `None`
/// when it cannot start or fails.
#[cfg(any(target_os = "macos", target_os = "linux"))]
fn tool_output(program: &str, args: &[&str]) -> Option<String> {
    use std::process::{Command, Stdio};

    let output = Command::new(program)
        .args(args)
        .env("LC_ALL", "C")
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output()
        .ok()?;
    output
        .status
        .success()
        .then(|| String::from_utf8_lossy(&output.stdout).into_owned())
}

/// The readers macOS and Linux share through POSIX.
#[cfg(any(target_os = "macos", target_os = "linux"))]
mod posix {
    use std::ffi::CStr;
    use std::mem::MaybeUninit;

    /// The volume whose space `{disk}` shows: on macOS `/` is the read-only
    /// system volume, and the user's files live on the data volume.
    #[cfg(target_os = "macos")]
    const DISK_VOLUME: &CStr = c"/System/Volumes/Data";
    #[cfg(target_os = "linux")]
    const DISK_VOLUME: &CStr = c"/";

    /// The host name from `gethostname`.
    pub(super) fn hostname() -> Option<String> {
        let mut buffer = [0_u8; 256];
        // SAFETY: the pointer and length describe `buffer`, and gethostname
        // writes at most that many bytes.
        let status = unsafe { libc::gethostname(buffer.as_mut_ptr().cast(), buffer.len()) };
        if status != 0 {
            return None;
        }
        let name = CStr::from_bytes_until_nul(&buffer).ok()?;
        Some(name.to_string_lossy().into_owned())
    }

    /// The bytes used and the total bytes of the disk volume, from `statvfs`:
    /// used is `(f_blocks − f_bfree) × f_frsize` of `f_blocks × f_frsize`.
    #[allow(
        clippy::useless_conversion,
        reason = "the statvfs field types differ between platforms"
    )]
    pub(super) fn disk() -> Option<(u64, u64)> {
        let mut stat = MaybeUninit::<libc::statvfs>::uninit();
        // SAFETY: DISK_VOLUME is NUL-terminated, and `stat` is valid for
        // writes of one statvfs.
        let status = unsafe { libc::statvfs(DISK_VOLUME.as_ptr(), stat.as_mut_ptr()) };
        if status != 0 {
            return None;
        }
        // SAFETY: statvfs succeeded, so it filled `stat`.
        let stat = unsafe { stat.assume_init() };
        let block = u64::from(stat.f_frsize);
        let blocks = u64::from(stat.f_blocks);
        let used = blocks.saturating_sub(u64::from(stat.f_bfree));
        Some((used.checked_mul(block)?, blocks.checked_mul(block)?))
    }
}

#[cfg(target_os = "macos")]
use macos as platform;

/// The macOS readers.
#[cfg(target_os = "macos")]
mod macos {
    use std::ffi::CStr;
    use std::mem::{self, MaybeUninit};
    use std::ptr;
    use std::time::{SystemTime, UNIX_EPOCH};

    pub(super) use super::posix::{disk, hostname};

    /// `macOS` and the product version from sysctl `kern.osproductversion`.
    pub(super) fn os() -> Option<String> {
        sysctl_string(c"kern.osproductversion").map(|version| format!("macOS {version}"))
    }

    /// The CPU model from sysctl `machdep.cpu.brand_string`.
    pub(super) fn cpu() -> Option<String> {
        sysctl_string(c"machdep.cpu.brand_string")
    }

    /// The first adapter's `Chipset Model` from `system_profiler`.
    pub(super) fn gpu() -> Option<String> {
        let report = super::tool_output("system_profiler", &["SPDisplaysDataType"])?;
        super::field(&report, "Chipset Model", ':').map(str::to_owned)
    }

    /// The bytes of memory used and the total: the total is sysctl
    /// `hw.memsize`, and used is the total less the free, inactive and
    /// speculative pages of `host_statistics64`, in pages of sysctl
    /// `vm.pagesize`.
    pub(super) fn memory() -> Option<(u64, u64)> {
        // SAFETY: hw.memsize is a 64-bit integer, and any bits are a u64.
        let total = unsafe { sysctl_value::<u64>(c"hw.memsize") }?;
        // SAFETY: vm_statistics64 holds only integers, so all zeros is valid.
        let mut info: libc::vm_statistics64 = unsafe { mem::zeroed() };
        let mut count = libc::HOST_VM_INFO64_COUNT;
        #[expect(deprecated, reason = "libc points to mach2, which jaci does not use")]
        // SAFETY: mach_host_self has no preconditions.
        let host = unsafe { libc::mach_host_self() };
        // SAFETY: `info` is valid for writes of HOST_VM_INFO64_COUNT
        // integers, the count `count` holds.
        let status = unsafe {
            libc::host_statistics64(
                host,
                libc::HOST_VM_INFO64,
                ptr::from_mut(&mut info).cast(),
                &raw mut count,
            )
        };
        if status != libc::KERN_SUCCESS {
            return None;
        }
        // The counters are in kernel pages, which sysctl vm.pagesize gives.
        // vm_page_size and hw.pagesize give the process's page size instead,
        // 4 KiB under Rosetta on 16 KiB Apple silicon.
        // SAFETY: vm.pagesize is a C int, and any bits are a c_int.
        let page = u64::try_from(unsafe { sysctl_value::<libc::c_int>(c"vm.pagesize") }?).ok()?;
        let pages = u64::from(info.free_count)
            + u64::from(info.inactive_count)
            + u64::from(info.speculative_count);
        let available = pages.checked_mul(page)?;
        Some((total.saturating_sub(available), total))
    }

    /// The seconds since sysctl `kern.boottime`.
    pub(super) fn uptime_seconds() -> Option<u64> {
        // SAFETY: kern.boottime is a timeval, which holds only integers.
        let boot = unsafe { sysctl_value::<libc::timeval>(c"kern.boottime") }?;
        let boot = u64::try_from(boot.tv_sec).ok()?;
        let now = SystemTime::now().duration_since(UNIX_EPOCH).ok()?.as_secs();
        now.checked_sub(boot)
    }

    /// The string value of the sysctl `name`.
    fn sysctl_string(name: &CStr) -> Option<String> {
        let mut size = 0;
        // SAFETY: `name` is NUL-terminated, and a null value pointer asks
        // only for the size, written to `size`.
        let status = unsafe {
            libc::sysctlbyname(
                name.as_ptr(),
                ptr::null_mut(),
                &raw mut size,
                ptr::null_mut(),
                0,
            )
        };
        if status != 0 {
            return None;
        }
        let mut buffer = vec![0_u8; size];
        // SAFETY: `name` is NUL-terminated, and `buffer` is valid for writes
        // of the `size` bytes `size` holds.
        let status = unsafe {
            libc::sysctlbyname(
                name.as_ptr(),
                buffer.as_mut_ptr().cast(),
                &raw mut size,
                ptr::null_mut(),
                0,
            )
        };
        if status != 0 {
            return None;
        }
        buffer.truncate(size);
        let value = CStr::from_bytes_until_nul(&buffer).ok()?;
        Some(value.to_string_lossy().into_owned())
    }

    /// The value of the sysctl `name`, when it is exactly one `T`.
    ///
    /// # Safety
    ///
    /// Every pattern of `size_of::<T>()` bytes must be a valid `T`.
    unsafe fn sysctl_value<T>(name: &CStr) -> Option<T> {
        let mut value = MaybeUninit::<T>::uninit();
        let mut size = mem::size_of::<T>();
        // SAFETY: `name` is NUL-terminated, and `value` is valid for writes
        // of the `size` bytes `size` holds.
        let status = unsafe {
            libc::sysctlbyname(
                name.as_ptr(),
                value.as_mut_ptr().cast(),
                &raw mut size,
                ptr::null_mut(),
                0,
            )
        };
        if status != 0 || size != mem::size_of::<T>() {
            return None;
        }
        // SAFETY: sysctlbyname wrote all the bytes of `value`, and the caller
        // vouches that any bytes are a valid `T`.
        Some(unsafe { value.assume_init() })
    }
}

#[cfg(target_os = "linux")]
use linux as platform;

/// The Linux readers.
#[cfg(target_os = "linux")]
mod linux {
    use std::fs;

    pub(super) use super::posix::{disk, hostname};

    /// The fields of `/proc/cpuinfo` that name the CPU when it has no
    /// `model name` and `lscpu` names none, in the order they are tried:
    /// older Arm has `Hardware` or `Processor`, and MIPS `cpu model`.
    const OTHER_CPU_FIELDS: [&str; 3] = ["Hardware", "Processor", "cpu model"];

    /// `PRETTY_NAME` from `/etc/os-release`, or from `/usr/lib/os-release`
    /// where the former is missing.
    pub(super) fn os() -> Option<String> {
        let release = fs::read_to_string("/etc/os-release")
            .or_else(|_| fs::read_to_string("/usr/lib/os-release"))
            .ok()?;
        let name = super::field(&release, "PRETTY_NAME", '=')?;
        Some(name.trim_matches(['"', '\'']).to_owned())
    }

    /// The `model name` of `/proc/cpuinfo`; else, as aarch64 names only part
    /// numbers there, the `lscpu` model; else the first of
    /// [`OTHER_CPU_FIELDS`].
    pub(super) fn cpu() -> Option<String> {
        let cpuinfo = fs::read_to_string("/proc/cpuinfo").unwrap_or_default();
        super::field(&cpuinfo, "model name", ':')
            .map(str::to_owned)
            .or_else(|| {
                let output = super::tool_output("lscpu", &[])?;
                super::lscpu_model(&output).map(str::to_owned)
            })
            .or_else(|| {
                OTHER_CPU_FIELDS
                    .iter()
                    .find_map(|key| super::field(&cpuinfo, key, ':'))
                    .map(str::to_owned)
            })
    }

    /// The first VGA or 3D controller that `lspci` lists, after its class.
    pub(super) fn gpu() -> Option<String> {
        const CLASSES: [&str; 2] = ["VGA compatible controller: ", "3D controller: "];

        let devices = super::tool_output("lspci", &[])?;
        devices.lines().find_map(|line| {
            let (_slot, device) = line.split_once(' ')?;
            CLASSES
                .iter()
                .find_map(|class| device.strip_prefix(class))
                .map(|name| name.trim().to_owned())
        })
    }

    /// The bytes of memory used and the total from `/proc/meminfo`: used is
    /// `MemTotal` less `MemAvailable`.
    pub(super) fn memory() -> Option<(u64, u64)> {
        let meminfo = fs::read_to_string("/proc/meminfo").ok()?;
        let bytes = |key| -> Option<u64> {
            let kib: u64 = super::field(&meminfo, key, ':')?
                .strip_suffix("kB")?
                .trim_end()
                .parse()
                .ok()?;
            kib.checked_mul(1024)
        };
        let total = bytes("MemTotal")?;
        let available = bytes("MemAvailable")?;
        Some((total.saturating_sub(available), total))
    }

    /// The whole seconds of the first field of `/proc/uptime`.
    pub(super) fn uptime_seconds() -> Option<u64> {
        let uptime = fs::read_to_string("/proc/uptime").ok()?;
        let seconds = uptime.split_whitespace().next()?;
        let whole = seconds.split_once('.').map_or(seconds, |(whole, _)| whole);
        whole.parse().ok()
    }
}

/// The readers of the other targets, which find nothing.
#[cfg(not(any(target_os = "macos", target_os = "linux")))]
mod platform {
    pub(super) fn hostname() -> Option<String> {
        None
    }

    pub(super) fn os() -> Option<String> {
        None
    }

    pub(super) fn cpu() -> Option<String> {
        None
    }

    pub(super) fn gpu() -> Option<String> {
        None
    }

    pub(super) fn memory() -> Option<(u64, u64)> {
        None
    }

    pub(super) fn disk() -> Option<(u64, u64)> {
        None
    }

    pub(super) fn uptime_seconds() -> Option<u64> {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::lscpu_model;

    #[test]
    fn lscpu_model_skips_unknown_names() {
        // Shaped like lscpu on a big.LITTLE board: one Model name per cluster.
        let output = "\
Architecture:             aarch64
  CPU op-mode(s):         32-bit, 64-bit
  Byte Order:             Little Endian
CPU(s):                   8
Vendor ID:                ARM
  Model name:             -
    Model:                0
  Model name:             Cortex-A76
    Model:                0
";
        assert_eq!(lscpu_model(output), Some("Cortex-A76"));
    }

    #[test]
    fn lscpu_model_is_none_when_only_unknown() {
        // Captured: lscpu 2.38.1 in a Debian bookworm aarch64 container on
        // Apple silicon.
        let output = "\
Architecture:                            aarch64
CPU op-mode(s):                          64-bit
Byte Order:                              Little Endian
CPU(s):                                  14
On-line CPU(s) list:                     0-13
Vendor ID:                               Apple
Model name:                              -
Model:                                   0
Thread(s) per core:                      1
";
        assert_eq!(lscpu_model(output), None);
    }
}
