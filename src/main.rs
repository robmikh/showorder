mod mkv;
mod pgs;
mod srt;
mod text;

use std::{
    collections::{HashMap, HashSet},
    path::Path,
};

use levenshtein::levenshtein;
use rayon::iter::{IntoParallelRefIterator, ParallelIterator};

use crate::mkv::load_first_n_english_subtitles;

fn main() -> windows::Result<()> {
    windows::initialize_sta()?;

    let args: Vec<_> = std::env::args().collect();
    if args.len() <= 1 {
        panic!("Invalid number of args!");
    }
    let mkv_path = &args[1];

    // TODO: Make num subtitles configurable
    let num_subtitles = 5;

    // Collect subtitles from the file(s)
    println!("Loading subtitles from mkv files...");
    let files = process_input_path(&mkv_path, num_subtitles)?;

    // If we couldn't find any subtitles, exit
    if files.is_empty() {
        println!("No English subtitles found!");
        return Ok(());
    }

    // If we have a second param, use it to compare the subtitles. Otherwise,
    // print the summary and complete.
    if args.len() <= 2 {
        print_subtitles(&files);
        return Ok(());
    }
    let srt_path = &args[2];

    // Load reference data
    println!("Loading reference data...");
    let ref_files = process_reference_path(&srt_path, num_subtitles)?;

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
    let mut unmapped = HashSet::<String>::new();
    let mut mapped_ref_files = HashMap::<String, usize>::new();
    let mut unmapped_ref_files = HashSet::<String>::new();
    ref_subtitles.iter().for_each(|(ref_file, _)| {
        unmapped_ref_files.insert(ref_file.clone());
    });
    for (mkv_path, file_distances) in &distances {
        // First will be the loweset
        let (ref_file, distance) = &file_distances[0];
        // TODO: Make min distance configurable
        if *distance < 3 * (num_subtitles + 1) {
            mappings.push((mkv_path.clone(), ref_file.clone()));
            unmapped_ref_files.remove(ref_file);
            let count = mapped_ref_files.entry(ref_file.clone()).or_insert(0);
            *count += 1;
        } else {
            unmapped.insert(mkv_path.clone());
        }
    }

    // Find the closest mkv files for our unmapped reference files.
    let mut closest_to_unmapped_ref_files = HashMap::<String, (String, usize)>::new();
    let mut still_unmapped_ref_files = Vec::<String>::new();
    for unmapped_ref_file in &unmapped_ref_files {
        let mut closest_mkv_path = String::new();
        let mut closest_distance = usize::MAX;
        for (mkv_path, file_distances) in &distances {
            // TODO: What does it mean if the closest file is already mapped to
            //       something else?
            if unmapped.contains(mkv_path) {
                for (ref_file, distance) in file_distances {
                    if ref_file == unmapped_ref_file {
                        if *distance < closest_distance {
                            closest_distance = *distance;
                            closest_mkv_path = mkv_path.clone();
                            break;
                        }
                    }
                }
            }
        }

        if !closest_mkv_path.is_empty() {
            println!("woop");
            closest_to_unmapped_ref_files.insert(
                unmapped_ref_file.clone(),
                (closest_mkv_path, closest_distance),
            );
        } else {
            still_unmapped_ref_files.push(unmapped_ref_file.clone());
        }
    }

    // Generate final mapping
    let mut final_mapping = Vec::<(String, String)>::with_capacity(
        mappings.len() + closest_to_unmapped_ref_files.len(),
    );
    for (mkv_path, ref_file) in &mappings {
        final_mapping.push((mkv_path.clone(), ref_file.clone()));
    }
    for (ref_file, (file, _)) in &closest_to_unmapped_ref_files {
        final_mapping.push((file.clone(), ref_file.clone()));
    }

    // Check to see if we have high confidence the mapping is correct. High confidence means:
    //   * No unmapped reference files
    //   * Each reference file is mapped to only 1 other file
    //   * Mkv files can still be unmapped (e.g. extras)
    let is_high_confidence = is_mapping_high_confidence(&final_mapping, &still_unmapped_ref_files);

    // Output mapping
    print_mapping(&mappings);
    print_unmapped(&unmapped);
    print_ref_file_info(&mapped_ref_files, &unmapped_ref_files);
    println!("");
    print_second_try_mapping(&closest_to_unmapped_ref_files, &still_unmapped_ref_files);
    println!("");
    if is_high_confidence {
        print!("(High Confidence) ");
    }
    print_final_mapping(&final_mapping);
    println!("");
    if is_high_confidence {
        print_powershell_rename_script(&final_mapping);
    }

    Ok(())
}

fn process_input_path<P: AsRef<Path>>(
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
                    if ext == "mkv" {
                        if let Some(subtitles) =
                            load_first_n_english_subtitles(&path, num_subtitles).unwrap()
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
                    load_first_n_english_subtitles(&path, num_subtitles).unwrap()
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

fn is_mapping_high_confidence(
    mapping: &Vec<(String, String)>,
    still_unmapped_ref_files: &Vec<String>,
) -> bool {
    if still_unmapped_ref_files.is_empty() {
        let mut seen_ref_files = HashMap::<&str, usize>::new();
        for (_, ref_file) in mapping {
            let count = seen_ref_files.entry(ref_file).or_insert(0);
            *count += 1;
        }

        for (_, count) in seen_ref_files {
            if count != 1 {
                return false;
            }
        }

        return true;
    }
    false
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
        println!("Unmapped mkv files:");
        for mkv_path in unmapped {
            let mkv_path = Path::new(mkv_path);
            let mkv_file_name = mkv_path.file_name().unwrap().to_str().unwrap();
            println!("  {}", mkv_file_name);
        }
    }
}

fn print_ref_file_info(
    mapped_ref_files: &HashMap<String, usize>,
    unmapped_ref_files: &HashSet<String>,
) {
    println!("Mapped reference files:");
    for (ref_file, count) in mapped_ref_files {
        let ref_path = Path::new(ref_file);
        let ref_file_name = ref_path.file_name().unwrap().to_str().unwrap();
        println!("  {} - {}", count, ref_file_name);
    }
    if !unmapped_ref_files.is_empty() {
        println!("Unmapped reference files:");
        for ref_file in unmapped_ref_files {
            let ref_path = Path::new(ref_file);
            let ref_file_name = ref_path.file_name().unwrap().to_str().unwrap();
            println!("  {}", ref_file_name);
        }
    }
}

fn print_second_try_mapping(
    closest_to_unmapped_ref_files: &HashMap<String, (String, usize)>,
    still_unmapped_ref_files: &Vec<String>,
) {
    println!("Closest mappings:");
    for (ref_file, (file, distance)) in closest_to_unmapped_ref_files {
        let mkv_path = Path::new(file);
        let ref_path = Path::new(ref_file);
        let mkv_file_name = mkv_path.file_name().unwrap().to_str().unwrap();
        let ref_file_name = ref_path.file_name().unwrap().to_str().unwrap();
        println!("  {} - {} -> {}", distance, mkv_file_name, ref_file_name);
    }
    if !still_unmapped_ref_files.is_empty() {
        println!("Still unmapped reference files:");
        for ref_file in still_unmapped_ref_files {
            let ref_path = Path::new(ref_file);
            let ref_file_name = ref_path.file_name().unwrap().to_str().unwrap();
            println!("  {}", ref_file_name);
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

    use crate::{compute_distances, flatten_subtitles, process_input_path};

    #[test]
    fn popeye_basic() -> windows::Result<()> {
        let subtitles = process_input_path("data/popeye/mkv", 5)?;
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
        let subtitles = process_input_path("data/popeye/mkv", num_subtitles)?;
        let subtitles = flatten_subtitles(&subtitles);
        let ref_subtitles = process_input_path("data/popeye/srt", num_subtitles)?;
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
