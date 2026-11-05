use crate::contracts::VpnSnapshot;
fn clean(input: &str) -> String {
    let mut chars = input.chars().peekable();
    let mut output = String::new();
    while let Some(c) = chars.next() {
        if c == '\u{1b}' && chars.peek() == Some(&'[') {
            chars.next();
            for next in chars.by_ref() {
                if ('@'..='~').contains(&next) {
                    break;
                }
            }
        } else if !c.is_control() || c == '\n' || c == '\t' {
            output.push(c);
        }
    }
    output
}
fn field<'a>(input: &'a str, key: &str) -> Option<&'a str> {
    let values: Vec<_> = input
        .lines()
        .filter_map(|line| line.split_once(':'))
        .filter(|(name, _)| name.trim() == key)
        .map(|(_, value)| value.trim())
        .collect();
    (values.len() == 1 && !values[0].is_empty()).then(|| values[0])
}
pub fn parse(status: &str, settings: &str, now: u64) -> VpnSnapshot {
    let status = clean(status);
    let settings = clean(settings);
    let country = field(&status, "Country").map(str::to_string);
    let state = match field(&status, "Status") {
        Some("Connected") if country.is_some() => "connected",
        Some("Disconnected") => "disconnected",
        Some("Connecting") => "connecting",
        Some("Disconnecting") => "disconnecting",
        _ => "unknown",
    };
    let kill_switch = match field(&settings, "Kill Switch")
        .map(str::to_lowercase)
        .as_deref()
    {
        Some("enabled") => "enabled",
        Some("disabled") => "disabled",
        _ => "unknown",
    };
    VpnSnapshot {
        revision: 0,
        state: state.into(),
        country: if state == "connected" { country } else { None },
        kill_switch: kill_switch.into(),
        observed_at_ms: now,
        error_code: if state == "unknown" {
            Some("unrecognized_provider_output".into())
        } else {
            None
        },
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn observed_connected_and_disabled_settings_are_distinct() {
        let result = parse(
            include_str!("../../../tests/fixtures/nordvpn/status-observed.txt"),
            include_str!("../../../tests/fixtures/nordvpn/settings-observed.txt"),
            1,
        );
        assert_eq!(result.state, "connected");
    }
    #[test]
    fn unrecognized_output_never_means_connected() {
        for status in [
            "Connected",
            "Status: Connected\nStatus: Disconnected\nCountry: Finland",
            "Status: Conectado",
            "Status: Connected\nCountry:",
            "daemon unavailable",
        ] {
            assert_ne!(parse(status, "Kill Switch: enabled", 1).state, "connected");
        }
    }
    #[test]
    fn ansi_extra_fields_and_disconnected_are_parsed() {
        assert_eq!(
            parse(
                "\u{1b}[32mStatus: Connected\u{1b}[0m\nCountry: Finland\nExtra: field",
                "Kill Switch: enabled",
                1
            )
            .country
            .as_deref(),
            Some("Finland")
        );
        assert_eq!(
            parse("Status: Disconnected", "Kill Switch: disabled", 1).state,
            "disconnected"
        );
    }
}
