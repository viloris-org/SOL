pub mod basic;
pub mod external;
pub mod registry;

pub use basic::{Cd, Echo, Exit, Ls, Pwd};
pub use registry::{Builtin, BuiltinRegistry};
