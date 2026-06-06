use anyhow::Result;
use git2::{Repository, Signature};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileStatus {
    pub path: String,
    pub is_modified: bool,
    pub is_new: bool,
    pub is_staged: bool,
}

pub fn stage(repo: &Repository, path: &str) -> Result<()> {
    let mut index = repo.index()?;
    index.add_path(std::path::Path::new(path))?;
    index.write()?;
    Ok(())
}

pub fn commit(repo: &Repository, message: &str) -> Result<()> {
    let mut index = repo.index()?;
    let oid = index.write_tree()?;
    let signature = Signature::now("TundraCode", "tundracode@local")?;
    let parent_commit = repo.head()?.peel_to_commit()?;
    let tree = repo.find_tree(oid)?;

    repo.commit(
        Some("HEAD"),
        &signature,
        &signature,
        message,
        &tree,
        &[&parent_commit],
    )?;

    Ok(())
}

pub fn get_status(repo: &Repository) -> Result<Vec<FileStatus>> {
    let mut opts = git2::StatusOptions::new();
    opts.include_untracked(true);
    let statuses = repo.statuses(Some(&mut opts))?;

    let mut result = Vec::new();
    for entry in statuses.iter() {
        if let Some(path) = entry
            .head_to_index()
            .map(|d| d.new_file().path())
            .or_else(|| entry.index_to_workdir().map(|d| d.new_file().path()))
            .flatten()
        {
            let status = entry.status();
            result.push(FileStatus {
                path: path.to_string_lossy().to_string(),
                is_modified: status.contains(git2::Status::WT_MODIFIED)
                    || status.contains(git2::Status::INDEX_MODIFIED),
                is_new: status.contains(git2::Status::WT_NEW)
                    || status.contains(git2::Status::INDEX_NEW),
                is_staged: status.contains(git2::Status::INDEX_NEW)
                    || status.contains(git2::Status::INDEX_MODIFIED)
                    || status.contains(git2::Status::INDEX_DELETED),
            });
        }
    }

    Ok(result)
}

pub fn get_branch(repo: &Repository) -> Result<String> {
    let head = repo.head()?;
    Ok(head.shorthand().unwrap_or("HEAD").to_string())
}
