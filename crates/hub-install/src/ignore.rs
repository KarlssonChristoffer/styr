//! A discard on purpose, shared by every crate of the workspace (hub-core re-exports it from
//! `hub_core::locked`; the app's shell, `hubd` and the launcher reach it here). It sits in
//! hub-install because that is the lowest crate the others depend on, and it needs nothing.
//!
//! `x.ignore("why")` says the failure is expected and nobody needs to see it, and `why` says why
//! at the call site (`tx.send(msg).ignore("the window may have closed")`). It replaces a bare
//! `let _ =`, which the `let_discard` rule flags because it cannot tell a deliberate discard from
//! a failed write nobody noticed. Does nothing at run time; a failure somebody should see is
//! logged instead (`warn_on_err` in hub-core). `why` must say something: an empty one fails a
//! debug build and the rule.

pub trait Ignore {
    fn ignore(self, why: &'static str);
}

impl<T, E> Ignore for Result<T, E> {
    #[inline]
    fn ignore(self, why: &'static str) {
        debug_assert!(!why.trim().is_empty(), "ignore() needs a reason");
    }
}

impl<T> Ignore for Option<T> {
    #[inline]
    fn ignore(self, why: &'static str) {
        debug_assert!(!why.trim().is_empty(), "ignore() needs a reason");
    }
}
