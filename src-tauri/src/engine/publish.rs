use std::collections::BTreeSet;

use super::context::{Engine, MARKER_FILE};
use super::error::{Error, Result};
use super::fs_util::write_atomic;
use super::manifest::Manifest;

/// Only this project's folder is checked out; other projects stay in the index untouched.
pub(super) fn sparse_paths(engine: &Engine, key_hash: &str) -> Vec<String> {
    vec![format!("/{MARKER_FILE}"), "/.gitattributes".into(), format!("/{}/", engine.project_path(key_hash))]
}

/// Independent of how a given git version treats sparse checkouts: a save of one project
/// must leave every other path of the snapshot byte-identical.
fn others_unchanged(engine: &Engine, key_hash: &str, base: &str, commit: &str) -> Result<()> {
    let repo = &engine.repo;
    let without = |entries: Vec<String>, name: &str| -> Vec<String> { entries.into_iter().filter(|e| !e.ends_with(&format!("	{name}"))).collect() };
    let (base_root, commit_root) = (repo.ls_tree(base, "")?, repo.ls_tree(commit, "")?);
    // A store that has no project yet has no `p` tree.
    let projects = |rev: &str, root: &[String]| if root.iter().any(|e| e.ends_with("\tp")) { repo.ls_tree(rev, "p") } else { Ok(Vec::new()) };
    let projects_same = without(projects(base, &base_root)?, key_hash) == without(projects(commit, &commit_root)?, key_hash);
    let root_same = without(base_root, "p") == without(commit_root, "p");
    if root_same && projects_same {
        Ok(())
    } else {
        Err(Error::Invalid("snapshot would change other projects".into()))
    }
}

pub(super) fn commit_and_push(engine: &Engine, key_hash: &str, manifest: &Manifest, expected: Option<&str>) -> Result<()> {
    // Key setup always publishes `keys/identity.age` first, so an empty remote was reset: data
    // pushed now would sit next to no key, unreadable for every other machine.
    let Some(expected) = expected else { return Err(Error::StoreReset) };
    let root = engine.repo.dir();
    let project_dir = root.join(engine.project_path(key_hash));
    write_atomic(&project_dir.join("manifest.age"), &manifest.seal(&engine.keys)?)?;
    let referenced: BTreeSet<String> = manifest.files.values().flat_map(|e| e.chunks.iter().map(|c| format!("{}.zst.age", c.name))).collect();
    if let Ok(entries) = std::fs::read_dir(project_dir.join("c")) {
        for e in entries.flatten().filter(|e| !referenced.contains(&e.file_name().to_string_lossy().into_owned())) {
            let _ = std::fs::remove_file(e.path());
        }
    }
    let commit = engine.repo.snapshot_commit()?;
    let in_tree: BTreeSet<String> = engine.repo.tree_paths(&commit, &engine.project_path(key_hash))?.into_iter().collect();
    if let Some(missing) = referenced.iter().find(|name| !in_tree.contains(&format!("{}/c/{name}", engine.project_path(key_hash)))) {
        return Err(Error::Invalid(format!("chunk {missing} missing from snapshot")));
    }
    others_unchanged(engine, key_hash, expected, &commit)?;
    engine.repo.push_lease(&commit, Some(expected))
}
