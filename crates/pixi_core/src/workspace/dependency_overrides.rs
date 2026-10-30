//! Applies locally configured dependency overrides
//! (`[dependency-overrides]` in, typically, a project-local, untracked
//! `.pixi/config.toml`) to a workspace's resolved dependencies. This lets a
//! developer swap a published package for a local path/git checkout or a
//! private channel without touching the committed `pixi.toml`.
//!
//! An override only ever replaces the spec of a dependency that is already
//! present; it never adds a new one. A misconfigured override is reported
//! with a `tracing::warn!` and left out, rather than failing the whole
//! operation, since this is a local developer convenience, not something
//! the rest of the team needs to be blocked by.

use std::{path::Path, str::FromStr};

use pixi_config::DependencyOverridesMap;
use pixi_manifest::CondaDependencies;
use pixi_spec::PixiSpec;
use rattler_conda_types::PackageName;

/// Applies `overrides` to `dependencies`, replacing the spec of any
/// dependency that is both already present in `dependencies` and named in
/// `overrides`. Relative `path` overrides are resolved against
/// `workspace_root`. Overrides for packages that are not an existing
/// dependency, or that are themselves invalid, are ignored (with a warning).
///
/// Each override is parsed with [`PixiSpec`]'s own `Deserialize` impl, the
/// same one `pixi.toml`'s own dependency tables use, so an override accepts
/// every shape a real dependency entry does without pixi_config needing to
/// duplicate any of its fields.
pub(crate) fn apply_dependency_overrides(
    mut dependencies: CondaDependencies,
    overrides: &DependencyOverridesMap,
    workspace_root: &Path,
) -> CondaDependencies {
    if overrides.is_empty() {
        return dependencies;
    }

    for (name, value) in overrides.0.iter() {
        let Ok(package_name) = PackageName::from_str(name) else {
            tracing::warn!("ignoring dependency-overrides.{name}: not a valid package name");
            continue;
        };
        if !dependencies.contains_key(&package_name) {
            // Overrides never introduce a dependency that isn't already declared.
            continue;
        }
        match serde_json::from_value::<PixiSpec>(value.clone()) {
            Ok(spec) => {
                dependencies.insert_overwrite(package_name, anchor_paths(spec, workspace_root))
            }
            Err(error) => tracing::warn!("ignoring dependency-overrides.{name}: {error}"),
        }
    }

    dependencies
}

/// Resolves a path-based override's path against `workspace_root`, the same
/// way a relative path dependency in `pixi.toml` is resolved against the
/// manifest's own directory.
fn anchor_paths(mut spec: PixiSpec, workspace_root: &Path) -> PixiSpec {
    match &mut spec {
        PixiSpec::PathSource(path_source) => {
            path_source.path = resolve_path(workspace_root, path_source.path.as_ref()).into();
        }
        PixiSpec::PathBinary(path_binary) => {
            path_binary.path = resolve_path(workspace_root, path_binary.path.as_ref()).into();
        }
        _ => {}
    }
    spec
}

fn resolve_path(workspace_root: &Path, path: &str) -> String {
    let candidate = Path::new(path);
    let joined = if candidate.is_absolute() {
        candidate.to_path_buf()
    } else {
        workspace_root.join(candidate)
    };
    pixi_path::normalize::normalize_std(&joined)
        .to_string_lossy()
        .into_owned()
}

#[cfg(test)]
mod tests {
    use pixi_config::DependencyOverridesMap;
    use rattler_conda_types::{ParseStrictness, VersionSpec};

    use super::*;

    fn deps(names: &[&str]) -> CondaDependencies {
        names
            .iter()
            .map(|name| {
                (
                    PackageName::from_str(name).unwrap(),
                    PixiSpec::Version(
                        VersionSpec::from_str("*", ParseStrictness::Lenient).unwrap(),
                    ),
                )
            })
            .collect()
    }

    fn overrides(entries: &[(&str, serde_json::Value)]) -> DependencyOverridesMap {
        DependencyOverridesMap(
            entries
                .iter()
                .map(|(name, value)| (name.to_string(), value.clone()))
                .collect(),
        )
    }

    #[test]
    fn replaces_an_existing_dependency_with_a_local_path() {
        let result = apply_dependency_overrides(
            deps(&["numpy", "scipy"]),
            &overrides(&[("numpy", serde_json::json!({ "path": "../numpy" }))]),
            Path::new("/workspace"),
        );

        // `../numpy` relative to the workspace root is a sibling directory.
        let numpy_spec = result
            .get_single(&PackageName::from_str("numpy").unwrap())
            .unwrap()
            .unwrap();
        assert_eq!(numpy_spec.to_string(), "/numpy");

        // The untouched dependency keeps its original spec.
        let scipy_spec = result
            .get_single(&PackageName::from_str("scipy").unwrap())
            .unwrap()
            .unwrap();
        assert_eq!(scipy_spec.to_string(), "*");
    }

    #[test]
    fn supports_the_same_shapes_as_a_pixi_toml_dependency() {
        let result = apply_dependency_overrides(
            deps(&["numpy", "requests"]),
            &overrides(&[
                (
                    "numpy",
                    serde_json::json!({ "version": "*", "channel": "my-dev-channel" }),
                ),
                (
                    "requests",
                    serde_json::json!({ "git": "https://github.com/org/requests.git", "branch": "main" }),
                ),
            ]),
            Path::new("/workspace"),
        );

        assert!(matches!(
            result
                .get_single(&PackageName::from_str("numpy").unwrap())
                .unwrap()
                .unwrap(),
            PixiSpec::DetailedVersion(_)
        ));
        assert!(matches!(
            result
                .get_single(&PackageName::from_str("requests").unwrap())
                .unwrap()
                .unwrap(),
            PixiSpec::Git(_)
        ));
    }

    #[test]
    fn ignores_an_override_for_a_package_that_is_not_a_dependency() {
        let result = apply_dependency_overrides(
            deps(&["numpy"]),
            &overrides(&[("not-a-dependency", serde_json::json!("1.0"))]),
            Path::new("/workspace"),
        );

        assert!(!result.contains_key(&PackageName::from_str("not-a-dependency").unwrap()));
    }

    #[test]
    fn ignores_a_malformed_override_and_keeps_the_original_spec() {
        let result = apply_dependency_overrides(
            deps(&["numpy"]),
            &overrides(&[(
                "numpy",
                serde_json::json!({ "path": "../numpy", "git": "https://example.com/numpy.git" }),
            )]),
            Path::new("/workspace"),
        );

        let numpy_spec = result
            .get_single(&PackageName::from_str("numpy").unwrap())
            .unwrap()
            .unwrap();
        assert_eq!(numpy_spec.to_string(), "*");
    }
}
