pub(super) fn unique_name<'a>(
    existing: impl Iterator<Item = &'a str> + Clone,
    desired: &str,
) -> String {
    let desired = desired.trim();
    let is_available = |candidate: &str| {
        !existing
            .clone()
            .any(|name| name.eq_ignore_ascii_case(candidate))
    };
    if is_available(desired) {
        return desired.to_owned();
    }

    let mut suffix = 2_usize;
    loop {
        let candidate = format!("{desired} {suffix}");
        if is_available(&candidate) {
            return candidate;
        }
        suffix += 1;
    }
}
