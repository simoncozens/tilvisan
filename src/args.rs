use std::path::PathBuf;

use clap::Parser;

use crate::{scripts::ScriptClassIndex, AutohintError, StemWidthMode, StemWidthModes};

#[derive(Parser, Debug, Clone)]
#[command(about = "TrueType autohinter", long_about = None)]
pub struct Args {
    /// Input font file. Use '-' for stdin.
    #[arg(value_name = "IN-FILE")]
    pub input: String,

    /// Output font file. Use '-' for stdout.
    #[arg(value_name = "OUT-FILE")]
    pub output: String,

    /// Set stem width mode for grayscale, GDI ClearType, and DW ClearType.
    /// Format: three letters 'n', 'q', or 's' (natural, quantized, strong).
    #[arg(short = 'a', long, value_parser=parse_stem_width_mode_values, default_value = "qsq")]
    pub stem_width_mode: StemWidthModes,

    /// Hint composite glyphs separately.
    #[arg(short = 'c', long)]
    pub composites: bool,

    /// Remove all hints.
    #[arg(short = 'd', long)]
    pub dehint: bool,

    /// Set default script.
    #[arg(short = 'D', long, value_parser = parse_script_class_index, default_value = "latn")]
    pub default_script: ScriptClassIndex,

    /// Set fallback script.
    #[arg(short = 'f', long, value_parser = parse_script_class_index, default_value = "none")]
    pub fallback_script: ScriptClassIndex,

    /// Set family suffix.
    #[arg(short = 'F', long, default_value = "")]
    pub family_suffix: String,

    /// Set fallback stem width (font units).
    #[arg(short = 'H', long, default_value_t = 0)]
    pub fallback_stem_width: u32,

    /// Ignore font restrictions (fsType bit 1).
    #[arg(short = 'i', long)]
    pub ignore_restrictions: bool,

    /// Add detailed autohint info to 'name' table.
    #[arg(short = 'I', long)]
    pub detailed_info: bool,

    /// Set minimum hinting range (PPEM).
    #[arg(short = 'l', long, value_parser = parse_hinting_range_min, default_value_t = 8)]
    pub hinting_range_min: u32,

    /// Set maximum hinting range (PPEM).
    #[arg(short = 'r', long, default_value_t = 50)]
    pub hinting_range_max: u32,

    /// Set hinting limit (PPEM).
    #[arg(short = 'G', long, default_value_t = 200)]
    pub hinting_limit: u32,

    /// Control file.
    #[arg(short = 'm', long)]
    pub control_file: Option<PathBuf>,

    /// Don't add autohinter info to 'name' table.
    #[arg(short = 'n', long)]
    pub no_info: bool,

    /// Pre-hinting (deprecated alias for adjust-subglyphs).
    #[arg(short = 'p', long)]
    pub pre_hinting: bool,

    /// Alias for adjust-subglyphs.
    #[arg(long)]
    pub adjust_subglyphs: bool,

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
    #[arg(short = 'x', long, value_parser = parse_increase_x_height, default_value_t = 14)]
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
            stem_width_mode: StemWidthModes::default(),
            composites: false,
            dehint: false,
            default_script: ScriptClassIndex::from_tag("latn")
                .expect("'latn' must exist in Skrifa script classes"),
            fallback_script: ScriptClassIndex::from_tag("none")
                .expect("'none' must exist in Skrifa script classes"),
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

pub(crate) fn parse_stem_width_mode_values(mode: &str) -> Result<StemWidthModes, String> {
    if mode.len() != 3 {
        return Err("Stem width mode string must consist of exactly three letters".to_string());
    }
    let parse_mode = |c| match c {
        'n' => Ok(StemWidthMode::Natural),
        'q' => Ok(StemWidthMode::Quantized),
        's' => Ok(StemWidthMode::Strong),
        _ => Err("Stem width mode letter must be 'n', 'q', or 's'".to_string()),
    };
    let chars: Vec<char> = mode.chars().collect();
    Ok(StemWidthModes {
        gray: parse_mode(chars[0])?,
        gdi_cleartype: parse_mode(chars[1])?,
        dw_cleartype: parse_mode(chars[2])?,
    })
}

pub(crate) fn parse_script_class_index(tag: &str) -> Result<ScriptClassIndex, String> {
    ScriptClassIndex::from_tag(tag)
}

pub(crate) fn parse_hinting_range_min(s: &str) -> Result<u32, String> {
    let min = s
        .parse::<u32>()
        .map_err(|_| format!("'{}' is not a valid number", s))?;
    if min >= 2 {
        Ok(min)
    } else {
        Err("hinting-range-min must be at least 2".to_string())
    }
}

pub(crate) fn parse_increase_x_height(s: &str) -> Result<u32, String> {
    let val = s
        .parse::<u32>()
        .map_err(|_| format!("'{}' is not a valid number", s))?;
    if val == 0 || val >= 6 {
        Ok(val)
    } else {
        Err("increase-x-height must be 0 or >= 6".to_string())
    }
}

impl Args {
    pub fn validate_cross_field_constraints(&self) -> Result<(), AutohintError> {
        if self.hinting_range_max < self.hinting_range_min {
            return Err(AutohintError::ValidationError(
                "hinting-range-max must be >= hinting-range-min".to_string(),
            ));
        }

        if self.hinting_limit > 0 && self.hinting_limit < self.hinting_range_max {
            return Err(AutohintError::ValidationError(
                "hinting-limit must be 0 or >= hinting-range-max".to_string(),
            ));
        }
        Ok(())
    }
}
