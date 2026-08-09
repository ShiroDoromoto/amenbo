//! **Which layer a plugin's gate, settings and secrets are written at** (`AMB-D-601`).
//!
//! A plugin lives at one layer, and its author picks which: [`Scope::Project`] writes a project's rows, and
//! [`Scope::Machine`] writes the device's. There is no second axis
//! beside it — no machine default a project overrides — so a caller never has two places to choose between
//! (`AMB-D-434`'s third surviving ground).
//!
//! **Derived, never assembled.** Underneath, the layer is a nullable `project_id`: a project's id, or NULL
//! for the device row. That is one `Option<i64>` among several a plugin call already carries, and passing it
//! raw is how a forgotten project id becomes a silent device write — `NOT NULL DEFAULT 0` used to catch
//! exactly that at commit, and does not any more. So this type is built in one place, from the declaration
//! ([`Layer::of`]), and the stores below take what it hands them.

use crate::error::Result;
use crate::plugin_manifest::Scope;

/// Where one plugin's gate, settings and secrets are written (`AMB-D-601`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Layer {
    /// One project's rows — the default layer, and where every plugin lived before `scope` came back.
    Project(i64),
    /// The device's rows: one answer for the whole machine, held by no project.
    Device,
}

impl Layer {
    /// The layer a plugin declaring `scope` addresses from a face standing in `project`.
    ///
    /// The declaration decides, and the caller's location only feeds it: `Scope::Machine` ignores where the
    /// face stands (the device has one answer wherever it is asked from), while `Scope::Project` has no
    /// answer at all without a project and is refused here rather than quietly resolved device-wide —
    /// [`plugin_trust::require_project`](crate::plugin_trust::require_project)'s wording, so a caller reads
    /// the same sentence whichever door turned it away.
    pub fn of(scope: Scope, project: Option<i64>) -> Result<Self> {
        match scope {
            Scope::Project => Ok(Layer::Project(crate::plugin_trust::require_project(project)?)),
            Scope::Machine => Ok(Layer::Device),
        }
    }

    /// The `project_id` a row at this layer carries — `None` **is** the device row, not a missing id.
    pub fn project_id(self) -> Option<i64> {
        match self {
            Layer::Project(id) => Some(id),
            Layer::Device => None,
        }
    }

    /// Whether this is the device layer, for a face that words the two differently.
    pub fn is_device(self) -> bool {
        matches!(self, Layer::Device)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The declaration is what picks the layer — the face's location only answers to it.
    #[test]
    fn the_declaration_picks_the_layer() {
        assert_eq!(Layer::of(Scope::Project, Some(7)).unwrap(), Layer::Project(7));
        assert_eq!(Layer::of(Scope::Machine, Some(7)).unwrap(), Layer::Device);
        assert_eq!(
            Layer::of(Scope::Machine, None).unwrap(),
            Layer::Device,
            "the device has one answer wherever it is asked from",
        );
    }

    /// A project's plugin asked for from no project is refused, not answered device-wide: that layer is the
    /// declaration's to pick, and this one did not pick it (`AMB-D-601`).
    #[test]
    fn a_projects_plugin_has_no_layer_without_a_project() {
        let err = Layer::of(Scope::Project, None).unwrap_err();
        assert!(format!("{err:?}").contains("per project"), "the reason is named: {err:?}");
    }

    /// The device row's key is NULL, which is the answer itself rather than an id nobody supplied.
    #[test]
    fn the_device_rows_key_is_the_absent_one() {
        assert_eq!(Layer::Project(3).project_id(), Some(3));
        assert_eq!(Layer::Device.project_id(), None);
        assert!(Layer::Device.is_device());
        assert!(!Layer::Project(3).is_device());
    }
}
