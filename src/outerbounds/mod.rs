//! `ana platform` — a thin passthrough to the outerbounds CLI.
//!
//! All arguments are forwarded verbatim; ana adds no behavior of its own.

mod run;

pub use run::run;
