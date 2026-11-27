use jaci::named::Named;
use jaci::sysinfo::{self, DynamicVariable, StaticVariable};

#[test]
fn static_variables_have_values() {
    for variable in StaticVariable::ALL {
        let value = variable.resolve();
        assert!(!value.is_empty(), "{{{}}} is empty", variable.name());
    }
}

#[test]
fn dynamic_variables_have_values() {
    for variable in DynamicVariable::ALL {
        let value = variable.resolve();
        assert!(!value.is_empty(), "{{{}}} is empty", variable.name());
        assert_ne!(
            value,
            sysinfo::UNKNOWN,
            "{{{}}} is unknown",
            variable.name()
        );
    }
}

#[test]
fn uptime_leaves_out_leading_zero_units() {
    assert_eq!(sysinfo::format_uptime(273_900), "3d 4h 5m");
    assert_eq!(sysinfo::format_uptime(300), "5m");
}

#[test]
fn sizes_show_gib_with_one_decimal() {
    assert_eq!(
        sysinfo::format_usage(6_979_321_856, 17_179_869_184),
        "6.5 GiB / 16.0 GiB"
    );
}

#[test]
fn user_variable_has_value() {
    assert!(!StaticVariable::User.resolve().is_empty());
}

#[test]
fn meter_shows_share_used() {
    assert_eq!(
        sysinfo::format_meter(28_132_035_789, 38_654_705_664),
        "\x1b[38;2;227;206;123m▰▰▰▰▰▰▰\x1b[0m\x1b[2m▱▱▱\x1b[0m \x1b[38;2;227;206;123m73%\x1b[0m\n\
         \x1b[2m26.2 / 36.0 GiB\x1b[0m"
    );
}

#[test]
fn gauge_colour_follows_share() {
    for (used, colour) in [
        (690, "148;207;119"),
        (700, "227;206;123"),
        (890, "227;206;123"),
        (900, "224;108;117"),
    ] {
        let meter = sysinfo::format_meter(used, 1000);
        let gauge = format!("\x1b[38;2;{colour}m");
        assert!(meter.starts_with(&gauge), "{used}: {meter:?}");
    }
}

#[test]
fn empty_and_full_meters() {
    let first_line = |used| {
        let meter = sysinfo::format_meter(used, 1000);
        meter.lines().next().unwrap().to_owned()
    };
    assert_eq!(
        first_line(0),
        "\x1b[2m▱▱▱▱▱▱▱▱▱▱\x1b[0m \x1b[38;2;148;207;119m0%\x1b[0m"
    );
    assert_eq!(
        first_line(1000),
        "\x1b[38;2;224;108;117m▰▰▰▰▰▰▰▰▰▰\x1b[0m \x1b[38;2;224;108;117m100%\x1b[0m"
    );
}

#[test]
fn no_total_shows_unknown() {
    assert_eq!(sysinfo::format_meter(0, 0), sysinfo::UNKNOWN);
}

/// The local time `year`-`month`-`day` `hour`:`minute`:`second`, with its
/// weekday and day of the year filled in.
fn local_time(year: i32, month: i32, day: i32, hour: i32, minute: i32, second: i32) -> libc::tm {
    // SAFETY: tm holds integers and, on Linux, a zone pointer, which may be
    // null.
    let mut time: libc::tm = unsafe { std::mem::zeroed() };
    time.tm_year = year - 1900;
    time.tm_mon = month - 1;
    time.tm_mday = day;
    time.tm_hour = hour;
    time.tm_min = minute;
    time.tm_sec = second;
    // SAFETY: `time` is a valid tm, which timegm normalises in place,
    // filling in tm_wday and tm_yday.
    unsafe { libc::timegm(&raw mut time) };
    time
}

#[test]
fn local_time_is_shown() {
    let time = local_time(2026, 11, 26, 14, 32, 50);
    assert_eq!(sysinfo::format_date(&time), "Thu 26 Nov 2026");
    assert_eq!(sysinfo::format_time(&time), "14:32");
    assert_eq!(sysinfo::format_week(&time), "W48");
}

#[test]
fn date_time_single_digits() {
    let time = local_time(2027, 1, 5, 9, 5, 0);
    assert_eq!(sysinfo::format_date(&time), "Tue 5 Jan 2027");
    assert_eq!(sysinfo::format_time(&time), "09:05");
    assert_eq!(sysinfo::format_week(&time), "W01");
}

#[test]
fn iso_week_at_year_boundary() {
    let time = local_time(2027, 1, 1, 12, 0, 0);
    assert_eq!(sysinfo::format_week(&time), "W53");
}
