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
