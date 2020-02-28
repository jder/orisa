use git2;
use std::path::{Path, PathBuf};

#[derive(Clone)]
pub struct Repo {
  root: PathBuf,
  remote_name: String,
  branch_name: String,
}

impl Repo {
  pub fn new(root: &Path, remote_name: String, branch_name: String) -> Repo {
    let root = root.to_path_buf();

    Repo {
      root,
      remote_name,
      branch_name,
    }
  }

  pub fn pull_latest(&self) -> Result<String, git2::Error> {
    let repo = git2::Repository::open(&self.root)?;
    let mut remote = repo.find_remote(&self.remote_name)?;
    let mut callbacks = git2::RemoteCallbacks::new();
    let mut returned_ssh = false;
    callbacks.sideband_progress(|msg| {
      log::info!("Git progress: {}", String::from_utf8_lossy(msg));
      return true;
    });
    callbacks.credentials(|_url, username, _types| {
      if returned_ssh {
        Err(git2::Error::from_str("no more users"))
      } else {
        returned_ssh = true;
        git2::Cred::ssh_key_from_agent(username.unwrap_or("git"))
      }
    });
    let mut options = git2::FetchOptions::new();
    options.remote_callbacks(callbacks);
    remote.fetch(&[&self.branch_name], Some(&mut options), None)?;
    let mut branch = repo.find_branch(&self.branch_name, git2::BranchType::Local)?;
    let commit = branch.upstream()?.get().peel_to_commit()?;

    let description = format!("{} ({})", commit.id(), commit.summary().unwrap_or(""));

    if commit.id() == branch.get().peel_to_commit()?.id() {
      Ok(format!("Already at {}", description))
    } else {
      self.move_to(&mut branch, &repo, &commit)?;

      Ok(format!("Updated to {}", description))
    }
  }

  pub fn move_to(
    &self,
    branch: &mut git2::Branch,
    repo: &git2::Repository,
    commit: &git2::Commit,
  ) -> Result<(), git2::Error> {
    // This seems like the wrong order (first we checkout the content, then update the branch)
    // but otherwise this doesn't work -- if you move the branch first, any changes appear
    // "dirty" to the checkout_* functions and so the default "safe" mode refuses to update them.
    // The alternative is to use "force" mode but this is scary if you *do* have changes.
    repo.checkout_tree(
      commit.as_object(),
      Some(
        git2::build::CheckoutBuilder::new()
          .recreate_missing(true)
          .remove_untracked(true),
      ),
    )?;

    branch
      .get_mut()
      .set_target(commit.id(), "orisa automatic update")?;

    Ok(())
  }
}
