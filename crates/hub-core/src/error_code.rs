//! The kind of an error at the API border. Inside the core errors are `anyhow`; at the border every
//! error body is `{ "error": "<text>" }`, and a client could not tell "not there" from "busy" from
//! "Hub's own bug" except by the text. [`ErrorCode`] names that, and the body gets an additional
//! `"code"` field (additive: old clients ignore it, old hubds do not send it, so a client treats a
//! missing code as unknown). The HTTP status is untouched on purpose: clients branch on 404/405
//! (`tooOld`), so the code is the only thing that says more.

use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ErrorCode {
    /// The thing asked for is not there (a thread, a project, an engine).
    NotFound,
    /// It is there but busy or held by someone else; trying again later can work.
    Conflict,
    /// Hub says no: bad input, not allowed here, a rule. Changing the request is the fix.
    Refused,
    /// Something Hub depends on failed: another machine, a provider, GitHub.
    Upstream,
    /// Hub's own fault (database, bug). Nothing the caller did.
    Internal,
}

impl ErrorCode {
    /// What an HTTP status says when nothing more is known.
    pub fn of_status(status: u16) -> Self {
        match status {
            404 => Self::NotFound,
            409 | 423 | 429 => Self::Conflict,
            502..=504 => Self::Upstream,
            500..=599 => Self::Internal,
            _ => Self::Refused,
        }
    }

    /// What an error from a handler is, looking at what is in its chain.
    pub fn of_error(err: &anyhow::Error) -> Self {
        let any = |f: &dyn Fn(&(dyn std::error::Error + 'static)) -> bool| err.chain().any(f);
        if any(&|c| c.downcast_ref::<rusqlite::Error>().is_some()) {
            Self::Internal
        } else if crate::store::gone(err) {
            Self::NotFound
        } else if crate::keeper::held_elsewhere(err) {
            Self::Conflict
        } else if crate::remote::too_old(err) || any(&|c| c.downcast_ref::<reqwest::Error>().is_some()) {
            Self::Upstream
        } else {
            Self::Refused
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_status_without_more_to_go_on_gives_its_kind() {
        assert_eq!(ErrorCode::of_status(404), ErrorCode::NotFound);
        assert_eq!(ErrorCode::of_status(409), ErrorCode::Conflict);
        assert_eq!(ErrorCode::of_status(400), ErrorCode::Refused);
        assert_eq!(ErrorCode::of_status(403), ErrorCode::Refused);
        assert_eq!(ErrorCode::of_status(502), ErrorCode::Upstream);
        assert_eq!(ErrorCode::of_status(500), ErrorCode::Internal);
    }

    #[test]
    fn an_error_is_told_by_what_is_in_its_chain() {
        let gone = anyhow::Error::new(crate::store::ThreadGone("t".into())).context("sending");
        assert_eq!(ErrorCode::of_error(&gone), ErrorCode::NotFound);
        assert_eq!(ErrorCode::of_error(&anyhow::anyhow!("pick a project")), ErrorCode::Refused);
        let sql = anyhow::Error::new(rusqlite::Error::QueryReturnedNoRows).context("reading");
        assert_eq!(ErrorCode::of_error(&sql), ErrorCode::Internal);
        let old = anyhow::Error::new(crate::remote::TooOld("/api/x".into()));
        assert_eq!(ErrorCode::of_error(&old), ErrorCode::Upstream);
    }

    #[test]
    fn the_code_is_written_in_snake_case() {
        assert_eq!(serde_json::to_string(&ErrorCode::NotFound).unwrap(), "\"not_found\"");
    }
}
