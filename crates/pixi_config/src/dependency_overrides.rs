//! Per-developer dependency source overrides.
//!
//! Configured under `[dependency-overrides]`, typically in a project-local,
//! untracked `.pixi/config.toml`, this lets a developer locally swap where a
//! dependency's package comes from (e.g. a local path instead of a
//! published package, or a private dev channel) without editing the
//! committed `pixi.toml`.
//!
//! Each entry accepts exactly the same shapes as a dependency entry in
//! `pixi.toml` itself (a plain version string, or a table with
//! `version`/`channel`, `path`, `git`, etc.) and is kept here as an opaque
//! JSON value rather than a typed spec: `pixi_config` can't depend on
//! `pixi_spec`, which already knows how to parse every one of those shapes,
//! without creating a dependency cycle through `pixi_git`/`pixi_utils`/
//! `pixi_auth`. The value is only interpreted as a real dependency spec
//! where the override is applied.

use indexmap::IndexMap;
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// A per-developer, local override of where a dependency's package comes
/// from, keyed by package name, in the same raw shape a dependency entry in
/// `pixi.toml` would use.
///
/// An override only ever replaces the *source* of a package that is already
/// a dependency somewhere in the workspace manifest; it never adds a new
/// dependency on its own.
#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(transparent)]
pub struct DependencyOverridesMap(pub IndexMap<String, Value>);

impl DependencyOverridesMap {
    /// Returns true if there are no overrides configured.
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}
