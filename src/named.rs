//! Enums whose values the config, the command line and the templates write
//! by name.

/// An enum whose every value has a name of its own.
pub trait Named: Copy + 'static {
    /// Every value, in the order lists of them show.
    const ALL: &'static [Self];

    /// The name the config, the command line or a template gives `self`.
    fn name(self) -> &'static str;

    /// The value named `name`, if any.
    fn from_name(name: &str) -> Option<Self> {
        Self::ALL.iter().copied().find(|value| value.name() == name)
    }

    /// The names of [`ALL`](Self::ALL), as `a, b, c`.
    fn names() -> String {
        Self::ALL
            .iter()
            .map(|value| value.name())
            .collect::<Vec<_>>()
            .join(", ")
    }

    /// `one of a, b, c`, the [`names`](Self::names) a value must be one of.
    fn one_of() -> String {
        format!("one of {}", Self::names())
    }
}
