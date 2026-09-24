use std::sync::Arc;

use super::{Detail, Transcript};
use crate::engine::error::{Error, Result};

/// Agents inside agents inside the session: deeper refs are not something the viewer builds.
const MAX_DEPTH: usize = 8;

pub enum Resolved {
    Detail(Detail),
    /// A subagent's own transcript.
    Agent(Arc<Transcript>),
}

/// What a path of refs points at (`agent:<id>/…/out:<tool id>`), each ref resolved in the
/// transcript the previous one opened. `open_agent(scope, id)` loads a subagent's transcript;
/// `scope` names it uniquely within the session (`agent:<a>/agent:<b>/`).
pub fn resolve(root: Arc<Transcript>, reference: &str, mut open_agent: impl FnMut(&str, &str) -> Result<Arc<Transcript>>) -> Result<Resolved> {
    let parts: Vec<&str> = reference.split('/').collect();
    if parts.len() > MAX_DEPTH {
        return Err(Error::Invalid("reference too deep".into()));
    }
    let (mut current, mut scope) = (root, String::new());
    for (i, part) in parts.iter().enumerate() {
        let last = i + 1 == parts.len();
        match current.detail(part).ok_or(Error::SessionNotFound)? {
            Detail::Agent(id) => {
                scope.push_str(&format!("agent:{id}/"));
                current = open_agent(&scope, &id)?;
                if last {
                    return Ok(Resolved::Agent(current));
                }
            }
            detail if last => return Ok(Resolved::Detail(detail)),
            _ => break,
        }
    }
    Err(Error::SessionNotFound)
}
