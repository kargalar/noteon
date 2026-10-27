//! Turkish number formatting for display; stored values remain locale independent.

const GROUP_SIZE: usize = 3;

pub fn format_display(input: &str) -> String {
    let trimmed = input.trim();
    let Ok(number) = trimmed.parse::<f64>() else {
        return input.to_owned();
    };
    if !number.is_finite() {
        return input.to_owned();
    }
    let expanded;
    let value = if trimmed.contains(['e', 'E']) {
        expanded = number.to_string();
        expanded.as_str()
    } else {
        trimmed
    };
    let (sign, unsigned) = value
        .strip_prefix('-')
        .map(|value| ("-", value))
        .or_else(|| value.strip_prefix('+').map(|value| ("+", value)))
        .unwrap_or(("", value));
    let (integer, fraction) = unsigned
        .split_once('.')
        .map_or((unsigned, None), |(integer, fraction)| {
            (integer, Some(fraction))
        });
    let mut result = String::with_capacity(value.len() + integer.len() / GROUP_SIZE);
    result.push_str(sign);
    if integer.is_empty() {
        result.push('0');
    }
    for (index, digit) in integer.chars().enumerate() {
        if index > 0 && (integer.len() - index).is_multiple_of(GROUP_SIZE) {
            result.push('.');
        }
        result.push(digit);
    }
    if let Some(fraction) = fraction.filter(|fraction| !fraction.is_empty()) {
        result.push(',');
        result.push_str(fraction);
    }
    result
}

#[cfg(test)]
mod tests {
    use super::format_display;

    #[test]
    fn groups_numbers_without_rounding_or_changing_invalid_input() {
        for (input, expected) in [
            ("40000", "40.000"),
            ("40000.50", "40.000,50"),
            ("-1234567.89", "-1.234.567,89"),
            ("+1000", "+1.000"),
            ("999", "999"),
            ("0", "0"),
            (".5", "0,5"),
            ("1e6", "1.000.000"),
            ("9007199254740993", "9.007.199.254.740.993"),
            ("", ""),
            ("500+20", "500+20"),
            ("NaN", "NaN"),
            ("inf", "inf"),
        ] {
            assert_eq!(format_display(input), expected, "{input}");
        }
    }
}
