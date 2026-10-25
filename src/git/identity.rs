//! Transport-independent repository identity. Only the server and repository
//! path leave the host; credentials, query strings and local paths never do.
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct RepositoryIdentity {
    server: String,
    path: String,
}

impl RepositoryIdentity {
    pub fn from_origin(origin: &str) -> Option<Self> {
        let origin = origin.trim();
        let url = if origin.contains("://") {
            url::Url::parse(origin).ok()?
        } else {
            // Git's scp syntax, including an optional user. A local path isn't
            // a network identity and must never merge two computers' projects.
            let (server, path) = origin.split_once(':')?;
            if server.contains('/') || server.contains('\\') || path.is_empty() {
                return None;
            }
            if !path.starts_with('/')
                && server
                    .rsplit_once('@')
                    .is_some_and(|(user, _)| user != "git")
            {
                // Relative paths under different SSH users' home directories
                // cannot be identified with a forge URL without server knowledge.
                return None;
            }
            url::Url::parse(&format!("ssh://{server}/{path}")).ok()?
        };
        if !matches!(url.scheme(), "https" | "http" | "ssh" | "git") {
            return None;
        }
        let mut server = url.host_str()?.to_ascii_lowercase();
        if let Some(port) = url.port()
            && !matches!((url.scheme(), port), ("ssh", 22) | ("git", 9418))
        {
            server.push_str(&format!(":{port}"));
        }
        let path = url.path().trim_matches('/');
        let path = path.strip_suffix(".git").unwrap_or(path);
        if path.is_empty()
            || path.len() > 2048
            || path
                .split('/')
                .any(|p| p.is_empty() || p == "." || p == "..")
        {
            return None;
        }
        Some(Self {
            path: if server == "github.com" {
                path.to_ascii_lowercase()
            } else {
                path.into()
            },
            server,
        })
    }

    pub fn server(&self) -> &str {
        &self.server
    }
    pub fn path(&self) -> &str {
        &self.path
    }
    /// A sanitized canonical URL, independent of the clone transport.
    pub fn canonical_url(&self) -> String {
        format!("https://{}/{}", self.server, self.path)
    }
}

/// Lightweight metadata lookup on a worker; does not enumerate worktrees or
/// contact a remote. A missing/invalid origin leaves this checkout independent.
pub fn checkout_identity(path: &std::path::Path) -> (Option<RepositoryIdentity>, Option<String>) {
    let Ok(repo) = git2::Repository::discover(path) else {
        return (None, None);
    };
    let origin = repo
        .find_remote("origin")
        .ok()
        .and_then(|remote| remote.url().ok().and_then(RepositoryIdentity::from_origin));
    let branch = Some(super::worktree_identity::read_head(&repo).description());
    (origin, branch)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn transport_and_credentials_do_not_split_a_repository() {
        let ssh =
            RepositoryIdentity::from_origin("git@github.com:ITSolTech/canopy-desktop.git").unwrap();
        let https = RepositoryIdentity::from_origin(
            "https://user:secret@github.com/itsoltech/canopy-desktop.git?token=secret#private",
        )
        .unwrap();
        assert_eq!(ssh, https);
        assert_eq!(
            ssh.canonical_url(),
            "https://github.com/itsoltech/canopy-desktop"
        );
        assert_ne!(
            ssh,
            RepositoryIdentity::from_origin("https://gitlab.com/itsoltech/canopy-desktop").unwrap()
        );
        assert!(RepositoryIdentity::from_origin("/home/user/canopy-desktop").is_none());
        assert!(RepositoryIdentity::from_origin("file:///home/user/canopy-desktop").is_none());
        assert!(RepositoryIdentity::from_origin("alice@server:private/project.git").is_none());
    }

    #[test]
    fn linked_worktrees_report_the_common_origin_and_their_own_branch() {
        let dir = tempfile::tempdir().unwrap();
        let repo = git2::Repository::init(dir.path().join("project")).unwrap();
        repo.remote(
            "origin",
            "https://user:private@github.com/itsoltech/canopy-desktop.git",
        )
        .unwrap();
        let tree = repo.index().unwrap().write_tree().unwrap();
        let tree = repo.find_tree(tree).unwrap();
        let signature = git2::Signature::now("Test", "test@example.invalid").unwrap();
        repo.commit(Some("HEAD"), &signature, &signature, "initial", &tree, &[])
            .unwrap();
        let linked = dir.path().join("feature");
        repo.worktree("feature", &linked, None).unwrap();
        let (origin, branch) = checkout_identity(&linked);
        assert_eq!(
            origin.unwrap().canonical_url(),
            "https://github.com/itsoltech/canopy-desktop"
        );
        assert_eq!(branch.as_deref(), Some("feature"));
        assert_eq!(
            super::super::inspect(&linked).unwrap().unwrap().origin,
            checkout_identity(repo.workdir().unwrap()).0
        );
    }
}
