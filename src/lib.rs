mod args;
mod bytecode;
mod c_api;
mod control;
mod control_index;
mod cvt;
mod error;
pub mod features;
mod font;
mod fpgm;
mod gasp;
mod globals;
mod glyf;
mod gpos;
mod head;
mod info;
mod intset;
mod loader;
mod maxp;
mod name;
mod opcodes;
mod orchestrate;
mod prep;
mod recorder;
mod scripts;
mod style;
mod style_metadata;

pub use args::Args;
pub use error::AutohintError;
pub use info::{build_version_string, InfoData};
pub use orchestrate::{ttfautohint, TtfautohintCall};
pub use scripts::ScriptClassIndex;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StemWidthMode {
    Natural,
    Quantized,
    Strong,
}

impl std::fmt::Display for StemWidthMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let c = match self {
            StemWidthMode::Natural => 'n',
            StemWidthMode::Quantized => 'q',
            StemWidthMode::Strong => 's',
        };
        write!(f, "{}", c)
    }
}

impl StemWidthMode {
    fn to_word(self) -> i32 {
        match self {
            StemWidthMode::Natural => -100,
            StemWidthMode::Quantized => 0,
            StemWidthMode::Strong => 100,
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct StemWidthModes {
    pub(crate) gray: StemWidthMode,
    pub(crate) gdi_cleartype: StemWidthMode,
    pub(crate) dw_cleartype: StemWidthMode,
}

impl std::fmt::Display for StemWidthModes {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}{}{}",
            self.gray, self.gdi_cleartype, self.dw_cleartype
        )
    }
}

impl Default for StemWidthModes {
    fn default() -> Self {
        Self {
            gray: StemWidthMode::Quantized,
            gdi_cleartype: StemWidthMode::Strong,
            dw_cleartype: StemWidthMode::Quantized,
        }
    }
}
