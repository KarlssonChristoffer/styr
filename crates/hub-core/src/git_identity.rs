//! Who git commits as on this machine (`git config --global user.name`/`user.email`): read for the
//! machine's ready list (`ready.rs`) and set from there or by an engine's share step
//! (`engines/share.rs`). Its own module so those two do not name each other.

use anyhow::{Result, bail};

use crate::model::GitIdentity;

/// Who git commits as on this machine, when both name and email are set.
pub fn identity() -> Option<GitIdentity> {
    let home = crate::folders::home();
    let get = |key: &str| crate::git::run(&home, &["config", "--global", key]).map(|v| v.trim().to_owned()).filter(|v| !v.is_empty());
    Some(GitIdentity { name: get("user.name")?, email: get("user.email")? })
}

/// Sets who git commits as on this machine.
pub fn set_identity(id: &GitIdentity) -> Result<()> {
    let name = id.name.trim();
    let email = id.email.trim();
    if name.is_empty() || !email.contains('@') || email.chars().any(char::is_whitespace) {
        bail!("give a name and an email address");
    }
    let home = crate::folders::home();
    for (key, value) in [("user.name", name), ("user.email", email)] {
        if crate::git::run(&home, &["config", "--global", key, value]).is_none() {
            bail!("git could not set {key} there");
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_name_without_an_email_is_refused() {
        let id = GitIdentity { name: "Christoffer".into(), email: "nope".into() };
        assert!(set_identity(&id).is_err());
        let id = GitIdentity { name: " ".into(), email: "a@b.c".into() };
        assert!(set_identity(&id).is_err());
    }
}
