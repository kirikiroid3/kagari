//! The loaded projects: those of the workspace folders and those found above opened files.

use std::path::Path;

use kagari_lang::Program;
use kagari_project::{MANIFEST_FILE, Project, ProjectError};

use crate::server::Server;

pub(crate) struct LoadedProject {
    pub project: Project,
    pub program: Program,
}

impl Server {
    /// (Re)loads every project from scratch: one per workspace folder with a manifest, plus the
    /// project of every open document outside those.
    pub fn load_projects(&mut self) {
        self.projects.clear();
        self.shown_errors.clear();
        for root in self.roots.clone() {
            let manifest = root.join(MANIFEST_FILE);
            if manifest.is_file() {
                let loaded = Project::load(&manifest);
                self.add_project(loaded);
            }
        }
        let open: Vec<_> = self.open.keys().cloned().collect();
        for path in open {
            self.find_project(&path);
        }
    }

    /// Loads the project of `file` when no loaded project contains it.
    pub fn find_project(&mut self, file: &Path) {
        if self.project_for(file).is_some() {
            return;
        }
        let Some(dir) = file.parent() else { return };
        match Project::find(dir) {
            // A file outside every project only gets syntax diagnostics.
            Err(ProjectError::NotFound { .. }) => {}
            loaded => self.add_project(loaded),
        }
    }

    fn add_project(&mut self, loaded: Result<Project, ProjectError>) {
        match loaded {
            Ok(project) => {
                if self
                    .projects
                    .iter()
                    .all(|other| other.project.root != project.root)
                {
                    let program = project.program(&self.db);
                    self.projects.push(LoadedProject { project, program });
                }
            }
            Err(error) => {
                let message = error.to_string();
                if self.shown_errors.insert(message.clone()) {
                    self.show_error(message);
                }
            }
        }
    }

    /// The loaded project with the deepest root containing `path`.
    pub fn project_for(&self, path: &Path) -> Option<&LoadedProject> {
        self.projects
            .iter()
            .filter(|loaded| path.starts_with(&loaded.project.root))
            .max_by_key(|loaded| loaded.project.root.components().count())
    }
}
