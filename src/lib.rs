mod args;
mod bytecode;
mod c_api;
mod c_font;
mod control;
mod control_index;
mod cvt;
mod emitter;
mod error;
pub mod features;
mod fpgm;
mod gasp;
mod globals;
mod glyf;
mod gpos;
mod head;
mod hmtx;
mod info;
mod intset;
mod loader;
mod logger;
mod maxp;
mod name;
mod opcodes;
mod orchestrate;
mod post;
mod prep;
mod recorder;
mod style_metadata;
mod tablestore;

pub use args::Args;
pub use error::AutohintError;
pub use info::{build_version_string, InfoData};
pub use orchestrate::{ttfautohint, TtfautohintCall};

#[cfg(test)]
mod tests {}
