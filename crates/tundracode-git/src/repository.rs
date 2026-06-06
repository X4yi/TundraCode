use anyhow::Result;
use git2::{Repository, StatusOptions};
use std::path::Path;

pub struct GitRepository {
    pub repo: Repository,
}

impl GitRepository {
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        let repo = Repository::open(path)?;
        Ok(Self { repo })
    }

    pub fn branch(&self) -> Result<String> {
        let head = self.repo.head()?;
        let name = head.shorthand().unwrap_or("HEAD").to_string();
        Ok(name)
    }

    pub fn is_dirty(&self) -> Result<bool> {
        let mut opts = StatusOptions::new();
        opts.include_untracked(true);
        let statuses = self.repo.statuses(Some(&mut opts))?;
        Ok(statuses.iter().count() > 0)
    }
}
