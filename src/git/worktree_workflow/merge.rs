use super::*;

pub(super) struct Publication {
    pub performed: bool,
    pub target_oid: String,
    pub warning: Option<String>,
}

fn symbolic_head(repo: &Repository) -> Result<String> {
    repo.find_reference("HEAD")
        .map_err(err)?
        .symbolic_target()
        .map_err(err)?
        .map(str::to_owned)
        .ok_or("The target worktree is no longer attached to a branch.".into())
}

fn verify_refs(
    repo: &Repository,
    target_ref: &str,
    target_oid: Oid,
    source_ref: &str,
    source_oid: Oid,
) -> Result<()> {
    if repo.refname_to_id(target_ref).map_err(err)? != target_oid
        || repo.refname_to_id(source_ref).map_err(err)? != source_oid
    {
        return Err("A branch changed after merge analysis. Analyze again.".into());
    }
    Ok(())
}

fn verify_checkout_registration(
    repository: &Path,
    branch: &str,
    expected_path: &Path,
) -> Result<()> {
    let info = inspect(repository)?.ok_or("Not a Git repository.")?;
    let actual = checked_out_path(&info, branch)
        .ok_or("The target branch is no longer checked out in the expected worktree.")?;
    if actual != expected_path {
        return Err("The target branch moved to another worktree. Analyze again.".into());
    }
    Ok(())
}

fn verify_checkout_head(repo: &Repository, target_ref: &str) -> Result<()> {
    if symbolic_head(repo)? != target_ref {
        return Err("The target worktree switched branches. Analyze again before merging.".into());
    }
    Ok(())
}

fn verify_not_checked_out(repository: &Path, branch: &str) -> Result<()> {
    let info = inspect(repository)?.ok_or("Not a Git repository.")?;
    if checked_out_path(&info, branch).is_some() {
        return Err(
            "The target branch became checked out while the merge was being prepared. Analyze again."
                .into(),
        );
    }
    Ok(())
}

fn write_merge_state(
    repo: &Repository,
    source_oid: Oid,
    target_oid: Oid,
    message: &str,
) -> Result<()> {
    let git_dir = repo.path();
    let write = |name: &str, value: String| {
        std::fs::write(git_dir.join(name), value)
            .map_err(|error| format!("Could not write {name}: {error}"))
    };
    let result = write("ORIG_HEAD", format!("{target_oid}\n"))
        .and_then(|_| write("MERGE_HEAD", format!("{source_oid}\n")))
        .and_then(|_| write("MERGE_MODE", String::new()))
        .and_then(|_| write("MERGE_MSG", format!("{message}\n")));
    if let Err(error) = result {
        let _ = repo.cleanup_state();
        return Err(error);
    }
    Ok(())
}

fn merge_state_cleanup_warning(repo: &Repository) -> Option<String> {
    repo.cleanup_state().err().map(|error| {
        format!(
            "published merge state cleanup needs attention: {}",
            error.message()
        )
    })
}

fn combine_warnings(first: Option<String>, second: Option<String>) -> Option<String> {
    match (first, second) {
        (Some(first), Some(second)) => Some(format!("{first}; {second}")),
        (Some(warning), None) | (None, Some(warning)) => Some(warning),
        (None, None) => None,
    }
}

fn checkout_tree(repo: &Repository, tree_id: Oid) -> Result<()> {
    let tree = repo.find_tree(tree_id).map_err(err)?;
    let object = tree.as_object();
    let mut checkout = CheckoutBuilder::new();
    checkout.safe().overwrite_ignored(false);
    repo.checkout_tree(object, Some(&mut checkout))
        .map_err(err)?;
    let mut index = repo.index().map_err(err)?;
    index.read_tree(&tree).map_err(err)?;
    index.write().map_err(err)
}

fn post_merge_warning(path: &Path, env: Option<&ShellEnvironment>) -> Option<String> {
    let result: Result<Option<String>> = (|| {
        let repo = Repository::open(path).map_err(err)?;
        if !hook_exists(&repo, "post-merge")? {
            return Ok(None);
        }
        let env = env.ok_or("Shell environment is unavailable for post-merge hook.")?;
        let signature = repo
            .signature()
            .map_err(|_| "Git author identity is unavailable for post-merge hook.".to_owned())?;
        let cancel = AtomicBool::new(false);
        let report = |_phase: &str| {};
        let mut hooks = super::super::hooks::Hooks::new(&repo, env, &cancel, &report)?;
        Ok(hooks
            .run("post-merge", &[std::ffi::OsStr::new("0")], &signature)
            .err()
            .map(|error| format!("post-merge needs attention: {error}")))
    })();
    match result {
        Ok(warning) => warning,
        Err(error) => Some(format!("post-merge could not run: {error}")),
    }
}

fn publish_fast_forward(
    repository: &Path,
    info: &super::super::RepositoryInfo,
    analysis: &BranchAnalysis,
    source: &git2::Commit<'_>,
    target: &git2::Commit<'_>,
    env: Option<&ShellEnvironment>,
) -> Result<Publication> {
    let target_ref = format!("refs/heads/{}", analysis.target);
    let source_ref = format!("refs/heads/{}", analysis.source);
    if let Some(path) = checked_out_path(info, &analysis.target) {
        let repo = Repository::open(&path).map_err(err)?;
        verify_checkout_registration(repository, &analysis.target, &path)?;
        verify_checkout_head(&repo, &target_ref)?;
        if statuses(&repo)?.0 > 0 {
            return Err(
                "The target worktree has local changes. Review it and analyze again.".into(),
            );
        }
        let mut transaction = repo.transaction().map_err(err)?;
        transaction.lock_ref("HEAD").map_err(err)?;
        transaction.lock_ref(&target_ref).map_err(err)?;
        transaction.lock_ref(&source_ref).map_err(err)?;
        verify_checkout_registration(repository, &analysis.target, &path)?;
        verify_checkout_head(&repo, &target_ref)?;
        verify_refs(&repo, &target_ref, target.id(), &source_ref, source.id())?;
        if statuses(&repo)?.0 > 0 {
            return Err("The target worktree changed before fast-forward publication.".into());
        }
        checkout_tree(&repo, source.tree_id())?;
        transaction
            .set_target(
                &target_ref,
                source.id(),
                None,
                "Canopy worktree merge: fast-forward",
            )
            .map_err(err)?;
        transaction.commit().map_err(err)?;
        return Ok(Publication {
            performed: true,
            target_oid: source.id().to_string(),
            warning: post_merge_warning(&path, env),
        });
    }

    let repo = root_repo(repository)?;
    verify_not_checked_out(repository, &analysis.target)?;
    let mut transaction = repo.transaction().map_err(err)?;
    transaction.lock_ref(&target_ref).map_err(err)?;
    transaction.lock_ref(&source_ref).map_err(err)?;
    verify_not_checked_out(repository, &analysis.target)?;
    verify_refs(&repo, &target_ref, target.id(), &source_ref, source.id())?;
    transaction
        .set_target(
            &target_ref,
            source.id(),
            None,
            "Canopy worktree merge: fast-forward",
        )
        .map_err(err)?;
    transaction.commit().map_err(err)?;
    Ok(Publication {
        performed: true,
        target_oid: source.id().to_string(),
        warning: None,
    })
}

fn run_pre_publication_hooks(
    repo: &Repository,
    message: &str,
    signature: &git2::Signature<'_>,
    env: &ShellEnvironment,
) -> Result<(String, git2::Oid)> {
    let cancel = AtomicBool::new(false);
    let report = |_phase: &str| {};
    let mut hooks = super::super::hooks::Hooks::new(repo, env, &cancel, &report)?;
    hooks.run("pre-merge-commit", &[], signature)?;
    let mut message_file =
        tempfile::NamedTempFile::new_in(repo.path()).map_err(|error| error.to_string())?;
    message_file
        .write_all(message.as_bytes())
        .map_err(|error| error.to_string())?;
    message_file.flush().map_err(|error| error.to_string())?;
    hooks.run(
        "prepare-commit-msg",
        &[
            message_file.path().as_os_str(),
            std::ffi::OsStr::new("merge"),
        ],
        signature,
    )?;
    hooks.run("commit-msg", &[message_file.path().as_os_str()], signature)?;
    let message = super::super::hooks::read_message(message_file.path())?;
    let mut index = repo.index().map_err(err)?;
    index.read(true).map_err(err)?;
    if index.has_conflicts() {
        return Err("A merge hook left conflicts in the target index.".into());
    }
    let tree = index.write_tree().map_err(err)?;
    Ok((message, tree))
}

fn publish_merge_commit(
    main: &Repository,
    repository: &Path,
    info: &super::super::RepositoryInfo,
    analysis: &BranchAnalysis,
    source: &git2::Commit<'_>,
    target: &git2::Commit<'_>,
    env: Option<&ShellEnvironment>,
) -> Result<Publication> {
    let mut merge_index = main.merge_commits(target, source, None).map_err(err)?;
    if merge_index.has_conflicts() {
        return Err("The branches now conflict. No refs or worktree files were changed.".into());
    }
    let initial_tree_id = merge_index.write_tree_to(main).map_err(err)?;
    let target_ref = format!("refs/heads/{}", analysis.target);
    let source_ref = format!("refs/heads/{}", analysis.source);
    let checked_out = checked_out_path(info, &analysis.target);
    if checked_out.is_none()
        && [
            "pre-merge-commit",
            "prepare-commit-msg",
            "commit-msg",
            "post-merge",
        ]
        .iter()
        .any(|name| hook_exists(main, name).unwrap_or(false))
    {
        return Err("The target branch has merge hooks but is not checked out. Open it as a worktree before merging.".into());
    }

    let signature = main
        .signature()
        .map_err(|_| "Configure user.name and user.email before merging.".to_owned())?;
    let mut message = format!(
        "Merge branch '{}' into {}",
        analysis.source, analysis.target
    );
    let mut tree_id = initial_tree_id;
    let mut prepared_for_hooks = false;
    if let Some(path) = checked_out.as_ref() {
        let repo = Repository::open(path).map_err(err)?;
        verify_checkout_registration(repository, &analysis.target, path)?;
        verify_checkout_head(&repo, &target_ref)?;
        if statuses(&repo)?.0 > 0 {
            return Err(
                "The target worktree has local changes. Review it and analyze again.".into(),
            );
        }
        let pre_hooks = ["pre-merge-commit", "prepare-commit-msg", "commit-msg"]
            .iter()
            .any(|name| hook_exists(&repo, name).unwrap_or(false));
        if pre_hooks {
            let mut preparation = repo.transaction().map_err(err)?;
            preparation.lock_ref("HEAD").map_err(err)?;
            preparation.lock_ref(&target_ref).map_err(err)?;
            preparation.lock_ref(&source_ref).map_err(err)?;
            verify_checkout_registration(repository, &analysis.target, path)?;
            verify_checkout_head(&repo, &target_ref)?;
            verify_refs(&repo, &target_ref, target.id(), &source_ref, source.id())?;
            if statuses(&repo)?.0 > 0 {
                return Err("The target worktree changed before merge hooks started.".into());
            }
            write_merge_state(&repo, source.id(), target.id(), &message)?;
            checkout_tree(&repo, initial_tree_id)?;
            drop(preparation);
            prepared_for_hooks = true;
            let env = env.ok_or("Shell environment is unavailable for merge hooks.")?;
            (message, tree_id) = run_pre_publication_hooks(&repo, &message, &signature, env)?;
        }
    }

    let tree = main.find_tree(tree_id).map_err(err)?;
    let config = main.config().map_err(err)?;
    let signing = super::super::signing::info(&config)?;
    let buffer = main
        .commit_create_buffer(&signature, &signature, &message, &tree, &[target, source])
        .map_err(err)?;
    let cancel = AtomicBool::new(false);
    let signed = if signing.enabled {
        let env = env.ok_or("Shell environment is unavailable for signed merge.")?;
        Some(super::super::signing::sign(
            &config,
            &buffer,
            signature.email().unwrap_or_default(),
            checked_out.as_deref().unwrap_or(repository),
            env,
            &cancel,
        )?)
    } else {
        None
    };
    let commit_oid = if let Some(signed) = signed {
        main.commit_signed(
            std::str::from_utf8(&buffer).map_err(|_| "Commit encoding is unsupported.")?,
            &signed,
            None,
        )
        .map_err(err)?
    } else {
        main.odb()
            .map_err(err)?
            .write(git2::ObjectType::Commit, &buffer)
            .map_err(err)?
    };

    if let Some(path) = checked_out.as_ref() {
        let repo = Repository::open(path).map_err(err)?;
        verify_checkout_registration(repository, &analysis.target, path)?;
        let mut transaction = repo.transaction().map_err(err)?;
        transaction.lock_ref("HEAD").map_err(err)?;
        transaction.lock_ref(&target_ref).map_err(err)?;
        transaction.lock_ref(&source_ref).map_err(err)?;
        verify_checkout_registration(repository, &analysis.target, path)?;
        verify_checkout_head(&repo, &target_ref)?;
        verify_refs(&repo, &target_ref, target.id(), &source_ref, source.id())?;
        if prepared_for_hooks {
            let mut index = repo.index().map_err(err)?;
            index.read(true).map_err(err)?;
            if index.write_tree().map_err(err)? != tree_id {
                return Err("The target index changed while the merge was being signed. Review it and analyze again.".into());
            }
        } else {
            if statuses(&repo)?.0 > 0 {
                return Err("The target worktree changed while the merge was being signed.".into());
            }
            checkout_tree(&repo, tree_id)?;
        }
        transaction
            .set_target(&target_ref, commit_oid, Some(&signature), &message)
            .map_err(err)?;
        transaction.commit().map_err(err)?;
        let state_warning = merge_state_cleanup_warning(&repo);
        return Ok(Publication {
            performed: true,
            target_oid: commit_oid.to_string(),
            warning: combine_warnings(state_warning, post_merge_warning(path, env)),
        });
    }

    verify_not_checked_out(repository, &analysis.target)?;
    let mut transaction = main.transaction().map_err(err)?;
    transaction.lock_ref(&target_ref).map_err(err)?;
    transaction.lock_ref(&source_ref).map_err(err)?;
    verify_not_checked_out(repository, &analysis.target)?;
    verify_refs(main, &target_ref, target.id(), &source_ref, source.id())?;
    transaction
        .set_target(&target_ref, commit_oid, Some(&signature), &message)
        .map_err(err)?;
    transaction.commit().map_err(err)?;
    Ok(Publication {
        performed: true,
        target_oid: commit_oid.to_string(),
        warning: None,
    })
}

pub(super) fn publish(
    repository: &Path,
    analysis: &BranchAnalysis,
    env: Option<&ShellEnvironment>,
) -> Result<Publication> {
    let main = root_repo(repository)?;
    let source = local_commit(&main, &analysis.source)?;
    let target = local_commit(&main, &analysis.target)?;
    if source.id().to_string() != analysis.source_oid
        || target.id().to_string() != analysis.target_oid
    {
        return Err("A branch changed after analysis. Analyze again before merging.".into());
    }
    if analysis.merge == MergeKind::AlreadyIntegrated {
        return Ok(Publication {
            performed: false,
            target_oid: target.id().to_string(),
            warning: None,
        });
    }
    let info = inspect(repository)?.ok_or("Not a Git repository.")?;
    match analysis.merge {
        MergeKind::FastForward => {
            publish_fast_forward(repository, &info, analysis, &source, &target, env)
        }
        MergeKind::MergeCommit => {
            publish_merge_commit(&main, repository, &info, analysis, &source, &target, env)
        }
        _ => Err("The merge cannot be performed from the current analysis.".into()),
    }
}
