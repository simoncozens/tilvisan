use crate::control::NumberSetAst;
use crate::AutohintError;
use std::collections::BTreeMap;

#[derive(Default, Debug, Clone)]
pub struct InfoData {
    pub no_info: bool,
    pub detailed_info: bool,
    pub info_string: Vec<u8>,
    pub info_string_wide: Vec<u8>,

    pub family_suffix: String,
    pub family_data: BTreeMap<(u16, u16, u16), Family>,

    pub hinting_range_min: i32,
    pub hinting_range_max: i32,
    pub hinting_limit: i32,

    pub gray_stem_width_mode: i32,
    pub gdi_cleartype_stem_width_mode: i32,
    pub dw_cleartype_stem_width_mode: i32,

    pub increase_x_height: i32,
    pub x_height_snapping_exceptions_string: String,
    pub fallback_stem_width: i32,

    pub windows_compatibility: bool,
    pub adjust_subglyphs: bool,
    pub hint_composites: bool,
    pub default_script: String,
    pub fallback_script: String,
    pub fallback_scaling: bool,
    pub symbol: bool,
    pub dehint: bool,
    pub ttfa_info: bool,

    pub control_name: Option<String>,
    pub reference_name: Option<String>,
    pub reference_index: i32,
}

impl InfoData {
    pub fn from_args(args: crate::args::Args) -> Result<Self, AutohintError> {
        let mut idata = InfoData {
            no_info: args.no_info,
            detailed_info: args.detailed_info,
            info_string: Vec::new(),
            info_string_wide: Vec::new(),
            family_suffix: args.family_suffix,
            family_data: std::collections::BTreeMap::new(),
            hinting_range_min: args.hinting_range_min,
            hinting_range_max: args.hinting_range_max,
            hinting_limit: args.hinting_limit,
            gray_stem_width_mode: 0,
            gdi_cleartype_stem_width_mode: 0,
            dw_cleartype_stem_width_mode: 0,
            increase_x_height: args.increase_x_height,
            x_height_snapping_exceptions_string: args.x_height_snapping_exceptions,
            fallback_stem_width: args.fallback_stem_width,
            windows_compatibility: args.windows_compatibility,
            adjust_subglyphs: args.adjust_subglyphs || args.pre_hinting,
            hint_composites: args.composites,
            default_script: args.default_script,
            fallback_script: args.fallback_script,
            fallback_scaling: args.fallback_scaling,
            symbol: args.symbol,
            dehint: args.dehint,
            ttfa_info: args.ttfa_table,
            control_name: args
                .control_file
                .as_ref()
                .map(|p| p.to_string_lossy().into_owned()),
            reference_name: args
                .reference
                .as_ref()
                .map(|p| p.to_string_lossy().into_owned()),
            reference_index: args.reference_index,
        };

        // Parse stem width mode
        if args.stem_width_mode.len() != 3 {
            return Err(AutohintError::InvalidArgument(
                "Stem width mode string must consist of exactly three letters".to_string(),
            ));
        }
        let parse_mode = |c| match c {
            'n' => Ok(-1),
            'q' => Ok(0),
            's' => Ok(1),
            _ => Err(AutohintError::InvalidArgument(
                "Stem width mode letter must be 'n', 'q', or 's'".to_string(),
            )),
        };
        let chars: Vec<char> = args.stem_width_mode.chars().collect();
        idata.gray_stem_width_mode = parse_mode(chars[0])?;
        idata.gdi_cleartype_stem_width_mode = parse_mode(chars[1])?;
        idata.dw_cleartype_stem_width_mode = parse_mode(chars[2])?;

        if !idata.no_info {
            let ret = build_version_string(&mut idata);
            if ret != 0 {
                eprintln!("Warning: Can't build version string (error {})", ret);
            }
        }

        Ok(idata)
    }
}

#[derive(Default, Debug, Clone)]
pub struct Family {
    // Indices into the per-record Vec<u8> slice being built in update_name_table.
    name_id_1: Option<usize>,
    name_id_4: Option<usize>,
    name_id_6: Option<usize>,
    name_id_16: Option<usize>,
    name_id_21: Option<usize>,
    family_name: Option<Vec<u8>>,
}

pub fn build_version_string(idata: &mut InfoData) -> i32 {
    let version = "1.8.4"; // TODO: Get from build system

    let mut d = format!("; ttfautohint (v{})", version);

    if !idata.detailed_info {
        finalize_info_string(idata, d);
        return 0;
    }

    if idata.dehint {
        d.push_str(" -d");
        finalize_info_string(idata, d);
        return 0;
    }

    d.push_str(&format!(" -l {}", idata.hinting_range_min));
    d.push_str(&format!(" -r {}", idata.hinting_range_max));
    d.push_str(&format!(" -G {}", idata.hinting_limit));
    d.push_str(&format!(" -x {}", idata.increase_x_height));
    if idata.fallback_stem_width != 0 {
        d.push_str(&format!(" -H {}", idata.fallback_stem_width));
    }

    d.push_str(&format!(" -D {}", idata.default_script));
    d.push_str(&format!(" -f {}", idata.fallback_script));

    if let Some(control_name) = &idata.control_name {
        let bn = std::path::Path::new(control_name)
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or(control_name);
        d.push_str(&format!(" -m \"{}\"", bn));
    }

    if let Some(reference_name) = &idata.reference_name {
        let bn = std::path::Path::new(reference_name)
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or(reference_name);
        d.push_str(&format!(" -R \"{}\"", bn));
        d.push_str(&format!(" -Z {}", idata.reference_index));
    }

    let mode_letters = ['n', 'q', 's'];
    let mode = format!(
        "{}{}{}",
        mode_letters[(idata.gray_stem_width_mode + 1) as usize],
        mode_letters[(idata.gdi_cleartype_stem_width_mode + 1) as usize],
        mode_letters[(idata.dw_cleartype_stem_width_mode + 1) as usize]
    );
    d.push_str(&format!(" -a {}", mode));

    if idata.windows_compatibility {
        d.push_str(" -W");
    }
    if idata.adjust_subglyphs {
        d.push_str(" -p");
    }
    if idata.hint_composites {
        d.push_str(" -c");
    }
    if idata.symbol {
        d.push_str(" -s");
    }
    if idata.fallback_scaling {
        d.push_str(" -S");
    }
    if idata.ttfa_info {
        d.push_str(" -t");
    }

    if !idata.x_height_snapping_exceptions_string.is_empty() {
        if let Ok(set) = NumberSetAst::parse(&idata.x_height_snapping_exceptions_string) {
            if let Ok(s) = set.canonicalize(6, 0x7FFF) {
                let s: String = s;
                if !s.is_empty() && s.len() <= 0xFFFF / 2 - d.len() {
                    d.push_str(&format!(" -X \"{}\"", s));
                }
            }
        }
    }

    finalize_info_string(idata, d);
    0
}

fn finalize_info_string(idata: &mut InfoData, d: String) {
    idata.info_string = d.into_bytes();

    idata.info_string_wide.clear();
    for &b in &idata.info_string {
        idata.info_string_wide.push(0);
        idata.info_string_wide.push(b);
    }
}

/// Called for each name record during name-table processing.
/// Modifies `data` in place for version strings (name_id 5).
/// For family-name IDs (1, 4, 6, 16, 21), stores `record_idx` so that
/// `process_name_post` can append a suffix after all records are visited.
pub fn process_name_record(
    platform_id: u16,
    encoding_id: u16,
    language_id: u16,
    name_id: u16,
    record_idx: usize,
    data: &mut Vec<u8>,
    idata: &mut InfoData,
) {
    if data.is_empty() {
        return;
    }

    if !idata.no_info && name_id == 5 {
        info_name_id_5_vec(platform_id, encoding_id, data, idata);
    }

    if !idata.family_suffix.is_empty() && matches!(name_id, 1 | 4 | 6 | 16 | 21) {
        let entry = idata
            .family_data
            .entry((platform_id, encoding_id, language_id))
            .or_default();
        match name_id {
            1 => entry.name_id_1 = Some(record_idx),
            4 => entry.name_id_4 = Some(record_idx),
            6 => entry.name_id_6 = Some(record_idx),
            16 => entry.name_id_16 = Some(record_idx),
            21 => entry.name_id_21 = Some(record_idx),
            _ => {}
        }
    }
}

/// Called after all records have been visited. Appends the family suffix to
/// the collected family-name records.
pub fn process_name_post(idata: &mut InfoData, records: &mut [Vec<u8>]) {
    if idata.family_data.is_empty() {
        return;
    }
    if idata.family_suffix.is_empty() {
        idata.family_data.clear();
        return;
    }

    // Step 1: Determine the representative family name for each triplet.
    for family in idata.family_data.values_mut() {
        let idx = family.name_id_16.or(family.name_id_1);
        if let Some(i) = idx {
            family.family_name = Some(records[i].clone());
        }
    }

    // Step 2: Pre-calculate best family name per (platform, encoding) pair.
    let mut best_names = BTreeMap::<(u16, u16), Vec<u8>>::new();
    for ((p, e, _), f) in &idata.family_data {
        if let Some(name) = &f.family_name {
            best_names.entry((*p, *e)).or_insert_with(|| name.clone());
        }
    }

    // Step 3: Append suffix to each collected record.
    let keys: Vec<_> = idata.family_data.keys().cloned().collect();
    for key in keys {
        let is_wide = !(key.0 == 1 || (key.0 == 3 && !(key.1 == 1 || key.1 == 10)));

        let Some(fname) = best_names.get(&(key.0, key.1)) else {
            continue;
        };

        let suffix: Vec<u8> = if is_wide {
            idata
                .family_suffix
                .as_bytes()
                .iter()
                .flat_map(|&b| [0u8, b])
                .collect()
        } else {
            idata.family_suffix.as_bytes().to_vec()
        };

        let ps_suffix: Vec<u8> = if is_wide {
            idata
                .family_suffix
                .as_bytes()
                .iter()
                .filter(|&&b| b != b' ')
                .flat_map(|&b| [0u8, b])
                .collect()
        } else {
            idata
                .family_suffix
                .as_bytes()
                .iter()
                .filter(|&&b| b != b' ')
                .cloned()
                .collect()
        };

        let ps_fname: Vec<u8> = if is_wide {
            fname
                .chunks_exact(2)
                .filter(|c| *c != [0u8, b' '])
                .flatten()
                .cloned()
                .collect()
        } else {
            fname.iter().filter(|&&b| b != b' ').cloned().collect()
        };

        let family = idata.family_data.get_mut(&key).unwrap();
        if let Some(idx) = family.name_id_1 {
            insert_suffix_vec(&mut records[idx], fname, &suffix);
        }
        if let Some(idx) = family.name_id_4 {
            insert_suffix_vec(&mut records[idx], fname, &suffix);
        }
        if let Some(idx) = family.name_id_16 {
            insert_suffix_vec(&mut records[idx], fname, &suffix);
        }
        if let Some(idx) = family.name_id_21 {
            insert_suffix_vec(&mut records[idx], fname, &suffix);
        }
        if let Some(idx) = family.name_id_6 {
            insert_suffix_vec(&mut records[idx], &ps_fname, &ps_suffix);
        }
    }

    idata.family_data.clear();
}

fn info_name_id_5_vec(platform_id: u16, encoding_id: u16, data: &mut Vec<u8>, idata: &InfoData) {
    let ttfautohint_tag: &[u8] = b"; ttfautohint";
    let ttfautohint_tag_wide: Vec<u8> = ttfautohint_tag.iter().flat_map(|&b| [0u8, b]).collect();

    let is_narrow =
        platform_id == 1 || (platform_id == 3 && !(encoding_id == 1 || encoding_id == 10));
    let (v, s, offset): (&[u8], &[u8], usize) = if is_narrow {
        (&idata.info_string, ttfautohint_tag, 2)
    } else {
        (&idata.info_string_wide, &ttfautohint_tag_wide, 4)
    };

    fn find_sub(haystack: &[u8], needle: &[u8]) -> Option<usize> {
        if needle.is_empty() {
            return None;
        }
        haystack.windows(needle.len()).position(|w| w == needle)
    }

    // Remove any existing ttfautohint marker in the string.
    if let Some(s_start) = find_sub(data, s) {
        let mut s_end = s_start + offset;
        while s_end < data.len() {
            if data[s_end] == b';' {
                if offset == 2 {
                    break;
                } else if s_end > 0 && data[s_end - 1] == 0 {
                    s_end -= 1;
                    break;
                }
            }
            s_end += 1;
        }
        data.drain(s_start..s_end);
    }

    // Append new version info if it fits within the u16 length limit.
    if !v.is_empty() && data.len() <= 0xFFFF - v.len() {
        data.extend_from_slice(v);
    }
}

fn insert_suffix_vec(data: &mut Vec<u8>, name: &[u8], suffix: &[u8]) {
    fn find_sub(haystack: &[u8], needle: &[u8]) -> Option<usize> {
        if needle.is_empty() {
            return None;
        }
        haystack.windows(needle.len()).position(|w| w == needle)
    }

    let new_vec = if let Some(idx) = find_sub(data, name) {
        let end = idx + name.len();
        let mut v = Vec::with_capacity(data.len() + suffix.len());
        v.extend_from_slice(&data[..end]);
        v.extend_from_slice(suffix);
        v.extend_from_slice(&data[end..]);
        v
    } else {
        let mut v = Vec::with_capacity(data.len() + suffix.len());
        v.extend_from_slice(data);
        v.extend_from_slice(suffix);
        v
    };

    if new_vec.len() <= 0xFFFF {
        *data = new_vec;
    }
}
