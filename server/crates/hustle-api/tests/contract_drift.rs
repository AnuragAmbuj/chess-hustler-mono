//! Contract-drift gate (ADR-009 / ADR-012): every route in `ROUTES` must exist
//! as a path + method pair in `contracts/openapi.yaml`. Adding a route without
//! updating the contract fails CI — no exceptions.

use hustle_api::ROUTES;

const CONTRACT: &str = include_str!("../../../../contracts/openapi.yaml");

#[test]
fn every_served_route_is_in_the_contract() {
    for (method, path) in ROUTES {
        assert!(
            contract_has(CONTRACT, method.as_str(), path),
            "route missing from contracts/openapi.yaml: {} {}",
            method,
            path
        );
    }
}

#[test]
fn contract_has_no_route_not_served_by_the_server() {
    for (path, method) in contract_paths_and_methods(CONTRACT) {
        let served = ROUTES
            .iter()
            .any(|(m, p)| p == &path && m.as_str().eq_ignore_ascii_case(&method));
        assert!(
            served,
            "contract declares {method} {path} but hustle-api does not serve it"
        );
    }
}

/// Minimal block-style YAML path scanner: collects (path, method) pairs from a
/// `paths:` block where methods are nested directly under each path key.
fn contract_paths_and_methods(yaml: &str) -> Vec<(String, String)> {
    const METHODS: [&str; 6] = ["get", "post", "put", "patch", "delete", "head"];
    let mut out = Vec::new();
    let mut in_paths = false;
    let mut current_path: Option<String> = None;
    for line in yaml.lines() {
        if line.starts_with("paths:") {
            in_paths = true;
            continue;
        }
        if in_paths && line.starts_with("components:") {
            break;
        }
        if !in_paths {
            continue;
        }
        let trimmed = line.trim();
        if line.starts_with("  /") && trimmed.ends_with(':') {
            current_path = Some(trimmed.trim_end_matches(':').to_string());
            continue;
        }
        if let Some(path) = &current_path {
            let method = trimmed.trim_end_matches(':');
            if line.starts_with("    ") && METHODS.contains(&method) {
                out.push((path.clone(), method.to_string()));
            }
        }
    }
    out
}

fn contract_has(yaml: &str, method: &str, path: &str) -> bool {
    contract_paths_and_methods(yaml)
        .iter()
        .any(|(p, m)| p == path && m.eq_ignore_ascii_case(method))
}
