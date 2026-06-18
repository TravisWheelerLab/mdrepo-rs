use serde_json::Value;

use crate::types::JsonCheck;

/// Walk a dot-notation + bracket-index path into a JSON value.
/// Handles "field", "field.sub", "arr[0]", "arr[0].field", "a[1].b[2].c", etc.
fn resolve_path<'a>(value: &'a Value, path: &str) -> Option<&'a Value> {
    let mut current = value;
    let mut chars = path.chars().peekable();
    let mut key = String::new();

    loop {
        match chars.next() {
            None => {
                if !key.is_empty() {
                    current = current.get(&key)?;
                }
                return Some(current);
            }
            Some('.') => {
                if !key.is_empty() {
                    current = current.get(&key)?;
                    key.clear();
                }
            }
            Some('[') => {
                if !key.is_empty() {
                    current = current.get(&key)?;
                    key.clear();
                }
                let mut idx_str = String::new();
                for c in chars.by_ref() {
                    if c == ']' {
                        break;
                    }
                    idx_str.push(c);
                }
                let idx: usize = idx_str.parse().ok()?;
                current = current.get(idx)?;
                if chars.peek() == Some(&'.') {
                    chars.next();
                }
            }
            Some(c) => {
                key.push(c);
            }
        }
    }
}

/// Parse `body` as JSON and run all `checks` against it, appending failures.
pub fn apply_json_checks(
    body: &str,
    checks: &[JsonCheck],
    path_hint: Option<&str>,
    failures: &mut Vec<String>,
) {
    let root: Value = match serde_json::from_str(body) {
        Ok(v) => v,
        Err(e) => {
            failures.push(format!("failed to parse JSON: {e}"));
            return;
        }
    };

    let suffix = path_hint
        .map(|p| format!(" (at {p})"))
        .unwrap_or_default();

    for check in checks {
        apply_one(&root, check, &suffix, failures);
    }
}

fn apply_one(root: &Value, check: &JsonCheck, suffix: &str, failures: &mut Vec<String>) {
    let resolved = resolve_path(root, &check.path);

    if !check.exists {
        if resolved.is_some() {
            failures.push(format!(
                "json {:?} exists but should not{suffix}",
                check.path
            ));
        }
        return;
    }

    let Some(v) = resolved else {
        failures.push(format!("json path {:?} not found{suffix}", check.path));
        return;
    };

    if let Some(expected) = &check.equals {
        if v != expected {
            failures.push(format!(
                "json {:?}: expected {expected}, got {v}{suffix}",
                check.path
            ));
        }
    }

    if let Some(needle) = &check.contains {
        match v.as_str() {
            Some(s) if s.contains(needle.as_str()) => {}
            Some(s) => failures.push(format!(
                "json {:?}: {needle:?} not in {s:?}{suffix}",
                check.path
            )),
            None => failures.push(format!(
                "json {:?}: contains check requires a string, got {v}{suffix}",
                check.path
            )),
        }
    }

    if let Some(threshold) = check.gt {
        match v.as_f64() {
            Some(n) if n > threshold => {}
            Some(n) => failures.push(format!(
                "json {:?}: expected > {threshold}, got {n}{suffix}",
                check.path
            )),
            None => failures.push(format!(
                "json {:?}: gt requires a number, got {v}{suffix}",
                check.path
            )),
        }
    }

    if let Some(threshold) = check.lt {
        match v.as_f64() {
            Some(n) if n < threshold => {}
            Some(n) => failures.push(format!(
                "json {:?}: expected < {threshold}, got {n}{suffix}",
                check.path
            )),
            None => failures.push(format!(
                "json {:?}: lt requires a number, got {v}{suffix}",
                check.path
            )),
        }
    }
}
