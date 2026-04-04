use std::path::PathBuf;

use clap::Parser;

#[derive(Parser, Debug, Clone)]
#[command(name = "ttfautohint", version = "1.8.4", about = "TrueType autohinter", long_about = None)]
pub struct Args {
    /// Input font file. Use '-' for stdin.
    #[arg(value_name = "IN-FILE")]
    pub input: String,

    /// Output font file. Use '-' for stdout.
    #[arg(value_name = "OUT-FILE")]
    pub output: String,

    /// Set stem width mode for grayscale, GDI ClearType, and DW ClearType.
    /// Format: three letters 'n', 'q', or 's' (natural, quantized, strong).
    #[arg(short = 'a', long, default_value = "qsq")]
    pub stem_width_mode: String,

    /// Hint composite glyphs separately.
    #[arg(short = 'c', long)]
    pub composites: bool,

    /// Remove all hints.
    #[arg(short = 'd', long)]
    pub dehint: bool,

    /// Set default script.
    #[arg(short = 'D', long, default_value = "latn")]
    pub default_script: String,

    /// Set fallback script.
    #[arg(short = 'f', long, default_value = "none")]
    pub fallback_script: String,

    /// Set family suffix.
    #[arg(short = 'F', long, default_value = "")]
    pub family_suffix: String,

    /// Set hinting limit (PPEM).
    #[arg(short = 'G', long, default_value_t = 200)]
    pub hinting_limit: u32,

    /// Set fallback stem width (font units).
    #[arg(short = 'H', long, default_value_t = 0)]
    pub fallback_stem_width: u32,

    /// Ignore font restrictions (fsType bit 1).
    #[arg(short = 'i', long)]
    pub ignore_restrictions: bool,

    /// Add detailed ttfautohint info to 'name' table.
    #[arg(short = 'I', long)]
    pub detailed_info: bool,

    /// Set minimum hinting range (PPEM).
    #[arg(short = 'l', long, default_value_t = 8)]
    pub hinting_range_min: u32,

    /// Control file.
    #[arg(short = 'm', long)]
    pub control_file: Option<PathBuf>,

    /// Don't add ttfautohint info to 'name' table.
    #[arg(short = 'n', long)]
    pub no_info: bool,

    /// Pre-hinting (deprecated alias for adjust-subglyphs).
    #[arg(short = 'p', long)]
    pub pre_hinting: bool,

    /// Alias for adjust-subglyphs.
    #[arg(long)]
    pub adjust_subglyphs: bool,

    /// Set maximum hinting range (PPEM).
    #[arg(short = 'r', long, default_value_t = 50)]
    pub hinting_range_max: u32,

    /// Reference font file.
    #[arg(short = 'R', long)]
    pub reference: Option<PathBuf>,

    /// Use fallback scaling instead of hinting.
    #[arg(short = 'S', long)]
    pub fallback_scaling: bool,

    /// Font is a symbol font.
    #[arg(short = 's', long)]
    pub symbol: bool,

    /// Add TTFA table.
    #[arg(short = 't', long)]
    pub ttfa_table: bool,

    /// Show TTFA table from input font and exit.
    #[arg(short = 'T', long)]
    pub ttfa_info: bool,

    /// Windows compatibility (blue zones for usWinAscent/Descent).
    #[arg(short = 'W', long)]
    pub windows_compatibility: bool,

    /// Set increase x-height limit (PPEM).
    #[arg(short = 'x', long, default_value_t = 14)]
    pub increase_x_height: u32,

    /// X-height snapping exceptions.
    #[arg(short = 'X', long, default_value = "")]
    pub x_height_snapping_exceptions: String,

    /// Reference font index.
    #[arg(short = 'Z', long, default_value_t = 0)]
    pub reference_index: u32,

    /// Debug mode.
    #[arg(long)]
    pub debug: bool,

    /// Epoch for reproducible builds (seconds since 1970-01-01).
    #[arg(long)]
    pub epoch: Option<u64>,
}

impl Default for Args {
    fn default() -> Self {
        Self {
            input: String::new(),
            output: String::new(),
            stem_width_mode: "qsq".to_string(),
            composites: false,
            dehint: false,
            default_script: "latn".to_string(),
            fallback_script: "none".to_string(),
            family_suffix: String::new(),
            hinting_limit: 200,
            fallback_stem_width: 0,
            ignore_restrictions: false,
            detailed_info: false,
            hinting_range_min: 8,
            control_file: None,
            no_info: false,
            pre_hinting: false,
            adjust_subglyphs: false,
            hinting_range_max: 50,
            reference: None,
            fallback_scaling: false,
            symbol: false,
            ttfa_table: false,
            ttfa_info: false,
            windows_compatibility: false,
            increase_x_height: 14,
            x_height_snapping_exceptions: String::new(),
            reference_index: 0,
            debug: false,
            epoch: None,
        }
    }
}
