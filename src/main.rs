mod mkv;
mod pgs;
mod srt;
mod text;

use std::{
    collections::{HashMap, HashSet},
    fs::File,
    path::Path,
};

use bindings::Windows::{
    Graphics::Imaging::BitmapEncoder,
    Storage::{CreationCollisionOption, FileAccessMode, StorageFolder},
};
use clap::{App, Arg, SubCommand};
use levenshtein::levenshtein;
use mkv::{find_subtitle_track_number_for_language, SubtitleIterator};
use rayon::iter::{IntoParallelRefIterator, ParallelIterator};
use webm_iterable::{matroska_spec::MatroskaSpec, WebmIterator};

use crate::mkv::load_first_n_english_subtitles;

fn main() -> windows::Result<()> {
    let matches = App::new("showorder")
        .arg(
            Arg::with_name("max-count")
                .short("n")
                .long("max-count")
                .takes_value(true)
                .default_value("5"),
        )
        .arg(
            Arg::with_name("track")
                .short("t")
                .long("track")
                .takes_value(true),
        )
        .arg(
            Arg::with_name("max distance")
                .short("m")
                .long("max")
                .takes_value(true),
        )
        .subcommand(
            SubCommand::with_name("list-tracks")
                .arg(Arg::with_name("mkv path").index(1).required(true)),
        )
        .subcommand(
            SubCommand::with_name("list").arg(Arg::with_name("mkv path").index(1).required(true)),
        )
        .subcommand(
            SubCommand::with_name("dump")
                .arg(
                    Arg::with_name("mkv path")
                        .index(1)
                        .requires("output path")
                        .required(true),
                )
                .arg(Arg::with_name("output path").index(2)),
        )
        .subcommand(
            SubCommand::with_name("match")
                .arg(
                    Arg::with_name("mkv path")
                        .index(1)
                        .requires("reference path")
                        .required(true),
                )
                .arg(Arg::with_name("reference path").index(2)),
        )
        .get_matches();

    windows::initialize_mta()?;

    let num_subtitles = usize::from_str_radix(matches.value_of("max-count").unwrap(), 10).unwrap();
    let track_number = if let Some(track_str) = matches.value_of("track") {
        Some(u64::from_str_radix(track_str, 10).unwrap())
    } else {
        None
    };
    let min_distance = if let Some(track_str) = matches.value_of("min distance") {
        Some(usize::from_str_radix(track_str, 10).unwrap())
    } else {
        None
    };

    if let Some(matches) = matches.subcommand_matches("match") {
        let mkv_path = matches.value_of("mkv path").unwrap();
        let ref_path = matches.value_of("reference path").unwrap();
        match_subtitles(mkv_path, ref_path, num_subtitles, track_number, min_distance)?;
    } else if let Some(matches) = matches.subcommand_matches("dump") {
        let mkv_path = matches.value_of("mkv path").unwrap();
        let output_path = matches.value_of("output path").unwrap();
        dump_subtitles(mkv_path, output_path, num_subtitles, track_number)?;
    } else if let Some(matches) = matches.subcommand_matches("list") {
        let mkv_path = matches.value_of("mkv path").unwrap();
        list_subtitles(mkv_path, num_subtitles, track_number)?;
    } else if let Some(matches) = matches.subcommand_matches("list-tracks") {
        let mkv_path = matches.value_of("mkv path").unwrap();
        list_tracks(mkv_path)?;
    } else {
        println!("Invalid input. Use --help to display help.")
    }

    Ok(())
}

fn list_tracks(mkv_path: &str) -> windows::Result<()> {
    let mut file = File::open(mkv_path).unwrap();
    let mut mkv_iter = WebmIterator::new(&mut file, &[MatroskaSpec::TrackEntry]);
    println!("Found English subtitle tracks:");
    while let Some(track_number) =
        find_subtitle_track_number_for_language(&mut mkv_iter, "eng", "en")
    {
        println!("  {}", track_number);
    }
    Ok(())
}

fn dump_subtitles(
    mkv_path: &str,
    output_path: &str,
    num_subtitles: usize,
    track_number: Option<u64>,
) -> windows::Result<()> {
    let mut file = File::open(mkv_path).unwrap();
    let iter = if let Some(track_number) = track_number {
        Some(SubtitleIterator::new_from_track_number(
            &mut file,
            track_number,
        )?)
    } else {
        SubtitleIterator::new(&mut file, "eng", "en")?
    };
    if let Some(iter) = iter {
        let path = Path::new(output_path).canonicalize().unwrap();
        let path = path.to_str().unwrap();
        let path = path.replace("\\\\?\\", "");
        let path = if path.starts_with("UNC") {
            path.replacen("UNC", "\\", 1)
        } else {
            path
        };
        let folder = StorageFolder::GetFolderFromPathAsync(path)?.get()?;
        for (i, bitmap) in iter.enumerate() {
            let file = folder
                .CreateFileAsync(
                    format!("{}.png", i),
                    CreationCollisionOption::ReplaceExisting,
                )?
                .get()?;
            let stream = file.OpenAsync(FileAccessMode::ReadWrite)?.get()?;
            let encoder =
                BitmapEncoder::CreateAsync(BitmapEncoder::PngEncoderId()?, stream)?.get()?;
            encoder.SetSoftwareBitmap(bitmap)?;
            encoder.FlushAsync()?.get()?;

            if i >= num_subtitles {
                break;
            }
        }
    } else {
        println!("No English subtitles found!");
    }
    Ok(())
}

fn list_subtitles(
    mkv_path: &str,
    num_subtitles: usize,
    track_number: Option<u64>,
) -> windows::Result<()> {
    // Collect subtitles from the file(s)
    println!("Loading subtitles from mkv files...");
    let files = process_input_path(&mkv_path, num_subtitles, track_number)?;
    print_subtitles(&files);
    Ok(())
}

fn match_subtitles(
    mkv_path: &str,
    ref_path: &str,
    num_subtitles: usize,
    track_number: Option<u64>,
    max_distance: Option<usize>,
) -> windows::Result<()> {
    // Collect subtitles from the file(s)
    println!("Loading subtitles from mkv files...");
    let files = process_input_path(&mkv_path, num_subtitles, track_number)?;

    // If we couldn't find any subtitles, exit
    if files.is_empty() {
        println!("No English subtitles found!");
        return Ok(());
    }

    // Load reference data
    println!("Loading reference data...");
    let ref_files = process_reference_path(&ref_path, num_subtitles)?;

    // Flatten our data
    let subtitles = flatten_subtitles(&files);
    let ref_subtitles = flatten_subtitles(&ref_files);

    // Compare subtitles
    println!("Comparing subtitles...");
    let distances = compute_distances(&&subtitles, &&ref_subtitles);

    // Output distances
    print_distances(&distances);

    // Map files to reference files
    // While we do this, we also want to know if a reference file
    // is mapped more than once, and which reference files went unmapped.
    let mut mappings = Vec::<(String, String)>::new();
    let mut seen_ref_files = HashMap::<&str, usize>::new();
    for (mkv_path, file_distances) in &distances {
        // First will be the loweset
        let (ref_file, distance) = &file_distances[0];

        let add = if let Some(max_distance) = max_distance {
            *distance < max_distance
        } else {
            true
        };
        
        if add {
            mappings.push((mkv_path.clone(), ref_file.clone()));
            let count = seen_ref_files.entry(ref_file).or_insert(0);
            *count += 1;
        }
    }

    // Make sure we haven't mapped something to the same reference file multiple times.
    let mut duplicates = Vec::<(String, usize)>::new();
    let mut unmapped = HashSet::<String>::new();
    for (ref_file, _) in &ref_subtitles {
        let count = *seen_ref_files.get(ref_file.as_str()).unwrap_or(&0);
        if count == 0 {
            unmapped.insert(ref_file.clone());
        } else if count > 1 {
            duplicates.push((ref_file.clone(), count));
        }
    }

    // Check to see if we have high confidence the mapping is correct. High confidence means:
    //   * Each reference file is mapped to only 1 other file
    //   * Mkv files can still be unmapped (e.g. extras)
    let is_high_confidence = duplicates.is_empty();

    // Output mapping
    print_mapping(&mappings);
    print_unmapped(&unmapped);
    if is_high_confidence {
        print!("(High Confidence) ");
    }
    print_final_mapping(&mappings);
    println!("");
    if is_high_confidence {
        print_powershell_rename_script(&mappings);
    }

    Ok(())
}

fn process_input_path<P: AsRef<Path>>(
    path: P,
    num_subtitles: usize,
    track_number: Option<u64>,
) -> windows::Result<Vec<(String, Vec<String>)>> {
    let path = path.as_ref();
    let mut result = Vec::new();
    if path.is_dir() {
        let paths: Vec<_> = std::fs::read_dir(path)
            .unwrap()
            .map(|p| p.unwrap())
            .collect();
        result = paths
            .par_iter()
            .filter_map(|p| {
                let path = p.path();
                if let Some(ext) = path.extension() {
                    if ext == "mkv" {
                        if let Some(subtitles) =
                            load_first_n_english_subtitles(&path, num_subtitles, track_number)
                                .unwrap()
                        {
                            // Sometimes there's a subtitle track with no subtitles in it...
                            if !subtitles.is_empty() {
                                let path = std::fs::canonicalize(path).unwrap();
                                let path = path.to_str().unwrap().to_owned();
                                return Some((path, subtitles));
                            }
                        }
                    }
                }
                None
            })
            .collect();
    } else if path.exists() && path.is_file() {
        if let Some(ext) = path.extension() {
            if ext == "mkv" {
                if let Some(subtitles) =
                    load_first_n_english_subtitles(&path, num_subtitles, track_number).unwrap()
                {
                    // Sometimes there's a subtitle track with no subtitles in it...
                    if !subtitles.is_empty() {
                        let path = std::fs::canonicalize(path).unwrap();
                        let path = path.to_str().unwrap().to_owned();
                        result.push((path, subtitles));
                    }
                }
            }
        }
    } else {
        panic!("Invalid input path: {:?}", path)
    }
    Ok(result)
}

fn print_subtitles(files: &Vec<(String, Vec<String>)>) {
    for (file, subtitles) in files {
        let path = Path::new(file);
        println!("{}:", path.file_name().unwrap().to_string_lossy());
        for subtitle in subtitles {
            println!("  \"{}\"", subtitle);
        }
    }
}

fn process_reference_path<P: AsRef<Path>>(
    path: P,
    num_subtitles: usize,
) -> windows::Result<Vec<(String, Vec<String>)>> {
    let path = path.as_ref();
    let mut result = Vec::new();
    if path.is_dir() {
        let paths: Vec<_> = std::fs::read_dir(path)
            .unwrap()
            .map(|p| p.unwrap())
            .collect();
        result = paths
            .par_iter()
            .filter_map(|p| {
                let path = p.path();
                if let Some(ext) = path.extension() {
                    if ext == "srt" {
                        let subtitles = srt::parse_n_subtitles(&path, num_subtitles);
                        if !subtitles.is_empty() {
                            let path = std::fs::canonicalize(path).unwrap();
                            let path = path.to_str().unwrap().to_owned();
                            return Some((path, subtitles));
                        }
                    }
                }
                None
            })
            .collect();
    } else if path.exists() && path.is_file() {
        if let Some(ext) = path.extension() {
            if ext == "srt" {
                let subtitles = srt::parse_n_subtitles(&path, num_subtitles);
                if !subtitles.is_empty() {
                    let path = std::fs::canonicalize(path).unwrap();
                    let path = path.to_str().unwrap().to_owned();
                    result.push((path, subtitles));
                }
            }
        }
    } else {
        panic!("Invalid reference path: {:?}", path)
    }
    Ok(result)
}

fn flatten_subtitles(files: &Vec<(String, Vec<String>)>) -> Vec<(String, String)> {
    files
        .iter()
        .map(|(file, subtitle)| (file.clone(), subtitle.join(" ")))
        .collect()
}

fn print_distances(distances: &HashMap<String, Vec<(String, usize)>>) {
    println!("Distances:");
    for (mkv_path, file_distances) in distances {
        let path = Path::new(mkv_path);
        println!("{} :", path.file_name().unwrap().to_str().unwrap());
        for (ref_file, distance) in file_distances {
            let path = Path::new(ref_file);
            let file_name = path.file_name().unwrap().to_str().unwrap();
            println!("  {} - {}", distance, file_name);
        }
    }
}

fn print_mapping(mapping: &[(String, String)]) {
    println!("Results:");
    for (mkv_path, ref_file) in mapping {
        let mkv_path = Path::new(mkv_path);
        let ref_path = Path::new(ref_file);
        let mkv_file_name = mkv_path.file_name().unwrap().to_str().unwrap();
        let ref_file_name = ref_path.file_name().unwrap().to_str().unwrap();
        println!("  {} -> {}", mkv_file_name, ref_file_name);
    }
}

fn print_unmapped(unmapped: &HashSet<String>) {
    if !unmapped.is_empty() {
        println!("Unmapped reference files:");
        for mkv_path in unmapped {
            let mkv_path = Path::new(mkv_path);
            let mkv_file_name = mkv_path.file_name().unwrap().to_str().unwrap();
            println!("  {}", mkv_file_name);
        }
    }
}

fn print_final_mapping(mapping: &[(String, String)]) {
    println!("Final mapping:");
    for (mkv_path, ref_file) in mapping {
        let mkv_path = Path::new(mkv_path);
        let ref_path = Path::new(ref_file);
        let mkv_file_name = mkv_path.file_name().unwrap().to_str().unwrap();
        let ref_file_name = ref_path.file_name().unwrap().to_str().unwrap();
        println!("  {} -> {}", mkv_file_name, ref_file_name);
    }
}

fn print_powershell_rename_script(mapping: &[(String, String)]) {
    println!("Rename script:");
    for (mkv_path, ref_file) in mapping {
        let mkv_path = Path::new(mkv_path);
        let ref_path = Path::new(ref_file);
        let mkv_file_name = mkv_path.file_name().unwrap().to_str().unwrap();
        let ref_file_stem = ref_path
            .file_stem()
            .unwrap()
            .to_str()
            .unwrap()
            .replace(".eng", "");
        println!(
            "Rename-Item -Path \"{}\" -NewName \"{}.mkv\"",
            mkv_file_name, ref_file_stem
        );
    }
}

fn compute_distances(
    subtitles: &[(String, String)],
    ref_subtitles: &[(String, String)],
) -> HashMap<String, Vec<(String, usize)>> {
    let mut distances = HashMap::<String, Vec<(String, usize)>>::new();
    for (file, subtitle) in subtitles {
        let file_path = Path::new(file);
        println!(
            "  Inspecting \"{}\"",
            file_path.file_name().unwrap().to_str().unwrap()
        );
        for (ref_file, ref_subtitle) in ref_subtitles {
            // Normalize to shortest
            let length = subtitle.len().min(ref_subtitle.len());
            let normalized_subtitle = &subtitle[0..length];
            let normalized_ref_subtitle = &ref_subtitle[0..length];

            let distance = levenshtein(normalized_subtitle, normalized_ref_subtitle);
            let matches = distances.entry(file.clone()).or_insert(Vec::new());
            matches.push((ref_file.clone(), distance));
        }
    }

    // Sort distances
    for (_, file_distances) in &mut distances {
        file_distances.sort_by(|(_, distance1), (_, distance2)| distance1.cmp(distance2));
    }

    distances
}

#[cfg(test)]
mod test {
    use std::{collections::HashMap, path::Path};

    use crate::{compute_distances, flatten_subtitles, process_input_path, process_reference_path};

    #[test]
    fn popeye_basic() -> windows::Result<()> {
        let subtitles = process_input_path("data/popeye/mkv", 5, None)?;
        let mut subtitles = flatten_subtitles(&subtitles);
        assert_eq!(subtitles.len(), 4);
        subtitles.sort_by(|(file1, _), (file2, _)| file1.cmp(file2));
        let subtitles = subtitles
            .iter()
            .map(|(file, subtitle)| {
                let path = Path::new(file);
                let file_name = path.file_name().unwrap().to_str().unwrap();
                (file_name, subtitle.as_str())
            })
            .collect::<Vec<_>>();
        let mut iter = subtitles.iter();
        assert_eq!(iter.next(), Some(&("Title T00-1.mkv", "oh oh wwhat happened ohh let me go let me go let me go nonono dont drop me now oh man the lifeboats")));
        assert_eq!(iter.next(), Some(&("Title T01-2.mkv", "whos the most phenominal extra ordinary fellow yous sinbad the sailor how do you like that stooges on one of my travels i ran into this now there was a thrill id be sorry to miss")));
        assert_eq!(iter.next(), Some(&("Title T02-3.mkv", "woah whats this hey let me down you big overgrown canary what are you doing taking me for a ride or something come back to me there you are with gravy")));
        assert_eq!(iter.next(), Some(&("Title T03-4.mkv", "im sinbad the sailor so hearty and hale i live on an island on the back ofa whale its a whale of an island thats not a bad joke its lord and its master is this handsom bloke")));
        Ok(())
    }

    #[test]
    fn popeye_match() -> windows::Result<()> {
        let num_subtitles = 5;
        let subtitles = process_input_path("data/popeye/mkv", num_subtitles, None)?;
        let subtitles = flatten_subtitles(&subtitles);
        let ref_subtitles = process_reference_path("data/popeye/srt", num_subtitles)?;
        let ref_subtitles = flatten_subtitles(&ref_subtitles);

        let distances = compute_distances(&subtitles, &ref_subtitles);
        let closest: HashMap<_, _> = distances
            .iter()
            .map(|(file, distances)| {
                let path = Path::new(file);
                let file_name = path.file_name().unwrap().to_str().unwrap();
                let ref_path = Path::new(&distances[0].0);
                let ref_file_name = ref_path.file_name().unwrap().to_str().unwrap();
                (file_name, ref_file_name)
            })
            .collect();

        let expected: HashMap<_, _> = [
            ("Title T00-1.mkv", "popeye p3.eng.srt"),
            ("Title T01-2.mkv", "popeye p2.eng.srt"),
            ("Title T02-3.mkv", "popeye p4.eng.srt"),
            ("Title T03-4.mkv", "popeye p1.eng.srt"),
        ]
        .iter()
        .cloned()
        .collect();

        for (actual_file, actual_ref_file) in closest {
            let expected_value = expected.get(actual_file).unwrap();
            assert_eq!(actual_ref_file, *expected_value);
        }

        Ok(())
    }
}
