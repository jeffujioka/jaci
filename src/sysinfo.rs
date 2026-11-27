//! The system variables a text block can show: the static ones, which the
//! system info cache holds, and the dynamic ones, read fresh at every banner.
//!
//! Dynamic values start no process and read only kernel interfaces, so a
//! banner stays fast and works with an empty `PATH`. Static values may run
//! system tools, as they are only resolved when the cache is written.

use std::ffi::CStr;
use std::fmt::{self, Write as _};
use std::iter;

use crate::ansi::{self, DIM, RESET};
use crate::named::Named;

/// The value of a variable that cannot be found.
pub const UNKNOWN: &str = "unknown";

/// The cells of a meter.
const METER_CELLS: usize = 10;
/// The gauge colour of a meter under 70%: `#94CF77`.
const GAUGE_GREEN: [u8; 3] = [0x94, 0xCF, 0x77];
/// The gauge colour of a meter from 70% and under 90%: `#E3CE7B`.
const GAUGE_GOLD: [u8; 3] = [0xE3, 0xCE, 0x7B];
/// The gauge colour of a meter from 90%: `#E06C75`.
const GAUGE_RED: [u8; 3] = [0xE0, 0x6C, 0x75];

/// A variable whose value stays put between banners, so the system info cache
/// holds it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StaticVariable {
    /// `{user}`: the account name of the process's user id.
    User,
    /// `{hostname}`: the host name.
    Hostname,
    /// `{os}`: the OS name and version.
    Os,
    /// `{cpu}`: the CPU model.
    Cpu,
    /// `{gpu}`: the first graphics adapter's name.
    Gpu,
}

impl Named for StaticVariable {
    const ALL: &'static [Self] = &[Self::User, Self::Hostname, Self::Os, Self::Cpu, Self::Gpu];

    /// The name a template writes between braces.
    fn name(self) -> &'static str {
        match self {
            Self::User => "user",
            Self::Hostname => "hostname",
            Self::Os => "os",
            Self::Cpu => "cpu",
            Self::Gpu => "gpu",
        }
    }
}

impl StaticVariable {
    /// The variable's value on this machine, [`UNKNOWN`] when it cannot be
    /// found. It may run system tools, which can take a few seconds.
    pub fn resolve(self) -> String {
        known(match self {
            Self::User => platform::user(),
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
    /// `{ram_meter}`: the memory used of the total, as [`format_meter`]
    /// shows it.
    RamMeter,
    /// `{disk_meter}`: the space used of the total on the volume `{disk}`
    /// reads, as [`format_meter`] shows it.
    DiskMeter,
    /// `{date}`: the local date, as [`format_date`] shows it.
    Date,
    /// `{time}`: the local time, as [`format_time`] shows it.
    Time,
    /// `{week}`: the local ISO 8601 week, as [`format_week`] shows it.
    Week,
}

impl Named for DynamicVariable {
    /// In the order of declaration, so a variable's position in it is
    /// `variable as usize`.
    const ALL: &'static [Self] = &[
        Self::Ram,
        Self::Disk,
        Self::Uptime,
        Self::RamMeter,
        Self::DiskMeter,
        Self::Date,
        Self::Time,
        Self::Week,
    ];

    /// The name a template writes between braces.
    fn name(self) -> &'static str {
        match self {
            Self::Ram => "ram",
            Self::Disk => "disk",
            Self::Uptime => "uptime",
            Self::RamMeter => "ram_meter",
            Self::DiskMeter => "disk_meter",
            Self::Date => "date",
            Self::Time => "time",
            Self::Week => "week",
        }
    }
}

impl DynamicVariable {
    /// The variable's value now, [`UNKNOWN`] when it cannot be found. It
    /// starts no process.
    pub fn resolve(self) -> String {
        known(match self {
            Self::Ram => platform::memory().map(|(used, total)| format_usage(used, total)),
            Self::Disk => platform::disk().map(|(used, total)| format_usage(used, total)),
            Self::Uptime => platform::uptime_seconds().map(format_uptime),
            Self::RamMeter => platform::memory().map(|(used, total)| format_meter(used, total)),
            Self::DiskMeter => platform::disk().map(|(used, total)| format_meter(used, total)),
            Self::Date => platform::local_time().map(|time| format_date(&time)),
            Self::Time => platform::local_time().map(|time| format_time(&time)),
            Self::Week => platform::local_time().map(|time| format_week(&time)),
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

/// `used` bytes of `total` as a meter of two lines, or [`UNKNOWN`] when
/// `total` is 0. The percentage is round(used ÷ total × 100) and the filled
/// cells of [`METER_CELLS`] round(percentage ÷ 10), both half away from zero;
/// the gauge colour is [`GAUGE_GREEN`] under 70%, [`GAUGE_GOLD`] under 90% and
/// [`GAUGE_RED`] from 90%. The first line is the filled `▰` in the gauge
/// colour, the empty `▱` dim, a space and the percentage in the gauge colour,
/// a run of no cells left out with its escapes; the second is
/// `<used> / <total> GiB` dim, each with one decimal.
pub fn format_meter(used: u64, total: u64) -> String {
    if total == 0 {
        return UNKNOWN.to_owned();
    }
    let (used_bytes, total_bytes) = (u128::from(used), u128::from(total));
    let percent = (used_bytes * 200 + total_bytes) / (total_bytes * 2);
    let filled =
        usize::try_from((percent + 5) / 10).map_or(METER_CELLS, |cells| cells.min(METER_CELLS));
    let gauge = if percent < 70 {
        GAUGE_GREEN
    } else if percent < 90 {
        GAUGE_GOLD
    } else {
        GAUGE_RED
    };
    let mut meter = String::new();
    if filled > 0 {
        ansi::push_foreground(&mut meter, gauge);
        meter.extend(iter::repeat_n('▰', filled));
        meter.push_str(RESET);
    }
    let empty = METER_CELLS - filled;
    if empty > 0 {
        meter.push_str(DIM);
        meter.extend(iter::repeat_n('▱', empty));
        meter.push_str(RESET);
    }
    meter.push(' ');
    ansi::push_foreground(&mut meter, gauge);
    write!(
        meter,
        "{percent}%{RESET}\n{DIM}{} / {} GiB{RESET}",
        Gib(used),
        Gib(total)
    )
    .expect("writing to a String cannot fail");
    meter
}

/// The local `time` as `<weekday> <day> <month> <year>`, such as
/// `Thu 26 Nov 2026`: the day without a leading zero, the names English and
/// three letters long.
pub fn format_date(time: &libc::tm) -> String {
    known(
        strftime(time, c"%a")
            .zip(strftime(time, c"%b %Y"))
            .map(|(weekday, month_year)| format!("{weekday} {} {month_year}", time.tm_mday)),
    )
}

/// The local `time` as 24-hour `HH:MM`, such as `14:32`.
pub fn format_time(time: &libc::tm) -> String {
    known(strftime(time, c"%H:%M"))
}

/// The ISO 8601 week of the local `time` as `W` and two digits, such as
/// `W48`.
pub fn format_week(time: &libc::tm) -> String {
    known(strftime(time, c"W%V"))
}

/// `time` formatted by the C library's `strftime` as `format` says, in the C
/// locale, as jaci never calls `setlocale`; `None` when it does not fit.
fn strftime(time: &libc::tm, format: &CStr) -> Option<String> {
    let mut buffer = [0_u8; 64];
    // SAFETY: the pointer and length describe `buffer`, `format` is
    // NUL-terminated and `time` is a valid tm.
    let length = unsafe {
        libc::strftime(
            buffer.as_mut_ptr().cast(),
            buffer.len(),
            format.as_ptr(),
            time,
        )
    };
    (length > 0).then(|| String::from_utf8_lossy(&buffer[..length]).into_owned())
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
    use std::ptr;

    /// The volume whose space `{disk}` shows: on macOS `/` is the read-only
    /// system volume, and the user's files live on the data volume.
    #[cfg(target_os = "macos")]
    const DISK_VOLUME: &CStr = c"/System/Volumes/Data";
    #[cfg(target_os = "linux")]
    const DISK_VOLUME: &CStr = c"/";

    /// The account name of the process's user id, from `getpwuid_r`.
    pub(super) fn user() -> Option<String> {
        /// The largest buffer tried for the entry's strings.
        const MAX_BUFFER: usize = 1 << 20;

        let mut buffer = vec![0_u8; 1024];
        loop {
            let mut entry = MaybeUninit::<libc::passwd>::uninit();
            let mut found = ptr::null_mut();
            // SAFETY: getuid has no preconditions; `entry` is valid for
            // writes of one passwd, and the pointer and length describe
            // `buffer`, which getpwuid_r writes the entry's strings into.
            let status = unsafe {
                libc::getpwuid_r(
                    libc::getuid(),
                    entry.as_mut_ptr(),
                    buffer.as_mut_ptr().cast(),
                    buffer.len(),
                    &raw mut found,
                )
            };
            if status == libc::ERANGE && buffer.len() < MAX_BUFFER {
                buffer.resize(buffer.len() * 2, 0);
                continue;
            }
            if status != 0 || found.is_null() {
                return None;
            }
            // SAFETY: getpwuid_r found the entry, so `found` points to
            // `entry`, whose pw_name is a NUL-terminated string in `buffer`.
            let name = unsafe { CStr::from_ptr((*found).pw_name) };
            return Some(name.to_string_lossy().into_owned());
        }
    }

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

    /// The local time now, from `time` and `localtime_r`.
    pub(super) fn local_time() -> Option<libc::tm> {
        // SAFETY: a null pointer asks time only to return the time.
        let now = unsafe { libc::time(ptr::null_mut()) };
        let mut time = MaybeUninit::<libc::tm>::uninit();
        // SAFETY: `now` is a time_t, and `time` is valid for writes of one
        // tm.
        let result = unsafe { libc::localtime_r(&raw const now, time.as_mut_ptr()) };
        if result.is_null() {
            return None;
        }
        // SAFETY: localtime_r succeeded, so it filled `time`.
        Some(unsafe { time.assume_init() })
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

    pub(super) use super::posix::{disk, hostname, local_time, user};

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

    pub(super) use super::posix::{disk, hostname, local_time, user};

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

#[cfg(test)]
mod tests {
    use super::{DynamicVariable, Named, lscpu_model};

    #[test]
    fn each_dynamic_variable_sits_at_its_discriminant() {
        for (index, &variable) in DynamicVariable::ALL.iter().enumerate() {
            assert_eq!(variable as usize, index, "{}", variable.name());
        }
    }

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
