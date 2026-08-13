pub mod engine;
/// Opt-in layout comparison; see the module docs to run it.
#[cfg(test)]
mod measure;
pub mod pathmatch;
pub mod persist;
pub mod query;
pub mod types;
pub mod walker;

pub use engine::IndexEngine;
