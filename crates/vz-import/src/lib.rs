//! Opening projects that were made in other editors.
//!
//! Only CapCut drafts for now. The importer builds an ordinary vidzcut [`vz_core::Project`]; nothing of the other
//! editor is copied (the media files stay where they are, the project only points to them).

pub mod capcut;

pub use capcut::{
    CapCutProject, ImportError, Imported, capcut_projects_dir, find_capcut_projects, import_capcut,
};
