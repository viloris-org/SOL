pub mod basic;
pub mod external;
pub mod fileops;
pub mod registry;
pub mod sysutils;
pub mod textutils;

pub use basic::{Cd, Clear, Echo, Exit, Help, Ls, Pwd, Reset, Which};
pub use fileops::{Cat, Cp, Mkdir, Mv, Rm, Touch};
pub use registry::{Builtin, BuiltinRegistry};
pub use sysutils::{Basename, Date, Dirname, Env, False, Sleep, True, Uname, Whoami};
pub use textutils::{Grep, Head, Sort, Tail, Uniq, Wc};
