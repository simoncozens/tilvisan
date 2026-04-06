use std::{
    collections::{HashMap, HashSet},
    fs,
    path::Path,
    process::{Command, Stdio},
};

use libtest_mimic::{Arguments, Trial};
use similar::TextDiff;
use ttfautohint_rs::{
    ttfautohint, Args, InfoData, ScriptClassIndex, StemWidthModes, TtfautohintCall,
};

fn main() {
    let args = Arguments::from_args();
    // glob all files in resources/test/fonts
    let fonts = glob::glob("resources/test/fonts/*.ttf")
        .unwrap()
        .filter_map(Result::ok)
        .collect::<Vec<_>>();
    println!("Found {} font files to test.", fonts.len());
    let fontcount = fonts.len();
    let mut tests = vec![];
    for file in fonts {
        let filename = file.file_stem().unwrap().to_str().unwrap().to_string();
        tests.push(Trial::test(filename.clone(), move || {
            // run ./target/debug/ttfautohint on the file and save output to a temp directory
            // run ttx on the output and expected file and compare the ttx outputs, ignoring checkSumAdjustment
            let output_dir = Path::new("test_output").join(&filename);
            fs::create_dir_all(&output_dir).unwrap();
            let output_file = output_dir.join(format!("{filename}.ttf"));
            // Remove it if it already exists
            if output_file.exists() {
                fs::remove_file(&output_file).unwrap();
            }
            eprintln!("Running ttfautohint on file {file:?}");
            run_ttfautohint(&file, &output_file);
            let expected_file = Path::new("resources/test/hinted").join(file.file_name().unwrap());
            eprintln!("Comparing output {output_file:?} with expected {expected_file:?}");
            compare_with_expected(&output_dir, &output_file, &expected_file);
            Ok(())
        }));
    }
    assert_eq!(tests.len(), fontcount);
    let conclusion = libtest_mimic::run(&args, tests);
    conclusion.exit();
}

fn diff_ttx(expected_ttx: &Path, output_ttx: &Path) -> String {
    let expected = fs::read_to_string(expected_ttx).unwrap();
    let output = fs::read_to_string(output_ttx).unwrap();
    let expected_per_table: HashMap<String, Vec<String>> = split_into_tables(&expected);
    let output_per_table: HashMap<String, Vec<String>> = split_into_tables(&output);
    let all_tables = expected_per_table
        .keys()
        .chain(output_per_table.keys())
        .collect::<HashSet<_>>();
    let mut result = String::new();
    let mut num_glyphs_wrong = false;
    // Put maxp first

    for table in std::iter::once(&"maxp".to_string())
        .chain(all_tables.iter().copied().filter(|t| *t != "maxp"))
    {
        match (expected_per_table.get(table), output_per_table.get(table)) {
            (Some(expected_lines), Some(output_lines)) => {
                // if expected_lines != output_lines {
                //     result += &format!("{table} differed...\n");
                // }
                // continue;
                if expected_lines != output_lines {
                    if num_glyphs_wrong && (table != "GlyphOrder") {
                        result += &format!(
                            "Table '{table}' differed and numGlyphs was wrong, all bets are off.\n"
                        );
                        continue;
                    }
                    if expected_lines.len() + output_lines.len() > 5000 {
                        result += &format!("Table '{table}' differed and was too big to diff.\n");
                        continue;
                    }
                    let diff =
                        &TextDiff::from_lines(&expected_lines.join("\n"), &output_lines.join("\n"))
                            .unified_diff()
                            .header("Expected", "Output")
                            .to_string();

                    // if diff.len() > 1000 {
                    //     result += &format!("Table '{table}' differed but was too big to report.\n");
                    //     continue;
                    // }
                    result +=
                        &(format!("\nDifference found in table '{table}':\n") + diff + "\n\n");
                    if table == "maxp" {
                        let expected_num_glyphs = expected_lines
                            .iter()
                            .find(|line| line.contains("numGlyphs"))
                            .unwrap();
                        let found_num_glyphs = output_lines
                            .iter()
                            .find(|line| line.contains("numGlyphs"))
                            .unwrap();
                        if expected_num_glyphs != found_num_glyphs {
                            num_glyphs_wrong = true;
                        }
                    }
                }
            }
            (Some(_), None) => {
                result += &format!("Output did not contain table {table}\n");
            }
            (None, Some(_output_lines)) => {
                if table == "BASE" {
                    // Some Harfbuzz tests drop BASE table and we don't, so ignore if it's missing in expected
                    continue;
                }
                result += &format!("Output contained extraneous table {table}\n",);
            }
            (None, None) => unreachable!(),
        }
    }
    result
}

fn run_ttfautohint(input_file: &Path, output_file: &Path) {
    let args = Args {
        input: input_file.to_string_lossy().into_owned(),
        output: output_file.to_string_lossy().into_owned(),
        stem_width_mode: StemWidthModes::default(),
        composites: true,
        dehint: false,
        default_script: ScriptClassIndex::from_tag("latn").unwrap(),
        fallback_script: ScriptClassIndex::from_tag("none").unwrap(),
        family_suffix: "".to_string(),
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
        x_height_snapping_exceptions: "".to_string(),
        reference_index: 0,
        debug: false,
        epoch: None,
    };

    let call = TtfautohintCall::from_args(&args)
        .unwrap_or_else(|e| panic!("Failed to construct call for {input_file:?}: {e}"));
    let mut info_data = InfoData::from_args(&args)
        .unwrap_or_else(|e| panic!("Failed to construct info data for {input_file:?}: {e}"));

    let output_bytes = ttfautohint(&call, &args, &mut info_data)
        .unwrap_or_else(|e| panic!("ttfautohint failed on file {input_file:?}: {e}"));

    fs::write(output_file, output_bytes)
        .unwrap_or_else(|e| panic!("Failed to write output file {output_file:?}: {e}"));
}

fn split_into_tables(output: &str) -> HashMap<String, Vec<String>> {
    let mut current_table = None;
    let mut hashmap: HashMap<String, Vec<String>> = HashMap::new();
    for line in output.lines() {
        if line.contains("checkSumAdjustment") || line.contains("modified") {
            continue;
        }
        if let Some(table_name) = line.strip_prefix("  <") {
            if table_name.starts_with('/') {
                current_table = None;
            } else {
                current_table = Some(table_name.trim_end_matches('>'));
            }
        } else if let Some(table_name) = current_table {
            if table_name == "head" && line.contains("indexToLocFormat") {
                continue;
            }
            hashmap
                .entry(table_name.to_owned())
                .or_default()
                .push(line.to_owned());
        }
    }
    hashmap
}

fn compare_with_expected(output_dir: &Path, output_file: &Path, expected_file: &Path) {
    let Ok(expected) = fs::read(expected_file) else {
        println!("Expected file {expected_file:?} does not exist, skipping comparison.");
        return;
    };
    let output = fs::read(output_file).unwrap();
    if expected != output {
        let expected_file_prefix = expected_file.file_stem().unwrap().to_str().unwrap();
        let expected_ttx = format!("{expected_file_prefix}.expected.ttx");
        let expected_ttx = output_dir.join(expected_ttx);
        Command::new("ttx")
            .arg("-o")
            .arg(&expected_ttx)
            .arg(expected_file)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .map(|s| s.success())
            .expect("ttx failed to parse the expected file {expected_file}");

        let output_ttx = output_file.with_extension("ttx");
        Command::new("ttx")
            .arg("-o")
            .arg(&output_ttx)
            .arg(output_file)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .map(|s| s.success())
            .expect("ttx failed to parse the output file {output_file}");

        let ttx_diff = diff_ttx(&expected_ttx, &output_ttx);
        if ttx_diff.trim_ascii().is_empty() {
            return;
        }
        //TODO: print more info about the test state
        panic!(
            "failed on {expected_file:?}\n{ttx_diff}\nError: ttx for expected and actual does not match."
        );
    }
}
