mod cli;
mod image;
mod interop;
mod mkv;
mod pgs;
mod srt;
mod text;
mod vob;
mod string;

use std::{
    collections::{HashMap, HashSet},
    fs::File,
    path::Path,
};

use clap::Parser;
use cli::{Args, Commands, DumpType, FileType};
use levenshtein::levenshtein;
use rayon::iter::{IntoParallelRefIterator, ParallelIterator};
use windows::{
    core::Result,
    Graphics::Imaging::{BitmapEncoder, BitmapPixelFormat},
    Storage::{CreationCollisionOption, FileAccessMode, FileIO, StorageFolder, Streams::Buffer},
    Win32::System::WinRT::{RoInitialize, RO_INIT_MULTITHREADED},
};

use crate::{mkv::{load_first_n_english_subtitles, KnownLanguage, MkvFile}, string::normalize_to_shortest_string};

fn main() -> Result<()> {
    let args = Args::parse();

    unsafe { RoInitialize(RO_INIT_MULTITHREADED)? };

    let num_subtitles = args.max_count;
    let track_number = args.track_number;
    let max_distance = args.max_distance;

    match args.command {
        Commands::ListTracks { mkv_path } => {
            list_tracks(&mkv_path)?;
        }
        Commands::List {
            file_type,
            input_path,
        } => match file_type {
            FileType::Mkv => {
                list_mkv_subtitles(&input_path, num_subtitles, track_number)?;
            }
            FileType::Srt => {
                list_srt_subtitles(&input_path, num_subtitles)?;
            }
        },
        Commands::Dump {
            dump_type,
            mkv_path,
            output_path,
        } => match dump_type {
            DumpType::Png => {
                dump_subtitle_images(
                    ImageDumpType::Png,
                    &mkv_path,
                    &output_path,
                    num_subtitles,
                    track_number,
                )?;
            }
            DumpType::Bgra8 => {
                dump_subtitle_images(
                    ImageDumpType::Raw,
                    &mkv_path,
                    &output_path,
                    num_subtitles,
                    track_number,
                )?;
            }
            DumpType::Block => {
                dump_subtitle_block_data(&mkv_path, &output_path, num_subtitles, track_number)?
            }
        },
        Commands::Match {
            mkv_path,
            reference_path,
        } => {
            match_subtitles(
                &mkv_path,
                &reference_path,
                num_subtitles,
                track_number,
                max_distance,
            )?;
        }
    }

    Ok(())
}

fn list_tracks(mkv_path: &str) -> Result<()> {
    let file = File::open(mkv_path).unwrap();
    let mkv = MkvFile::new(file);
    println!("Found subtitle tracks:");
    for track_info in mkv.tracks() {
        println!(
            "  {} - {} ({})",
            track_info.track_number,
            track_info.language.to_string(),
            track_info.encoding.to_string()
        );
    }
    Ok(())
}

enum ImageDumpType {
    Png,
    Raw,
}

fn dump_subtitle_images(
    dump_type: ImageDumpType,
    mkv_path: &str,
    output_path: &str,
    num_subtitles: usize,
    track_number: Option<u64>,
) -> Result<()> {
    let file = File::open(mkv_path).expect(&format!("Could not read from \"{}\"", mkv_path));
    let mkv = MkvFile::new(file);
    let iter = if let Some(track_number) = track_number {
        mkv.subtitle_iter_from_track_number(track_number)?
    } else {
        mkv.subtitle_iter(KnownLanguage::English)?
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
            match dump_type {
                ImageDumpType::Png => {
                    let file = folder
                        .CreateFileAsync(
                            format!("{}.png", i),
                            CreationCollisionOption::ReplaceExisting,
                        )?
                        .get()?;
                    let stream = file.OpenAsync(FileAccessMode::ReadWrite)?.get()?;
                    let encoder =
                        BitmapEncoder::CreateAsync(BitmapEncoder::PngEncoderId()?, stream)?
                            .get()?;
                    encoder.SetSoftwareBitmap(bitmap)?;
                    encoder.FlushAsync()?.get()?;
                }
                ImageDumpType::Raw => {
                    let width = bitmap.PixelWidth()?;
                    let height = bitmap.PixelHeight()?;
                    let format = bitmap.BitmapPixelFormat()?;
                    assert_eq!(format, BitmapPixelFormat::Bgra8);
                    let bytes_per_pixel = 4;
                    let bitmap_size = (width * height * bytes_per_pixel) as u32;
                    let buffer = Buffer::Create(bitmap_size)?;
                    bitmap.CopyToBuffer(&buffer)?;
                    let file = folder
                        .CreateFileAsync(
                            format!("{}size{}x{}.bin", i, width, height),
                            CreationCollisionOption::ReplaceExisting,
                        )?
                        .get()?;
                    FileIO::WriteBufferAsync(file, buffer)?.get()?;
                }
            }

            if i >= num_subtitles {
                break;
            }
        }
    } else {
        println!("No English subtitles found!");
    }
    Ok(())
}

fn dump_subtitle_block_data(
    mkv_path: &str,
    output_path: &str,
    num_subtitles: usize,
    track_number: Option<u64>,
) -> Result<()> {
    let file = File::open(mkv_path).expect(&format!("Could not read from \"{}\"", mkv_path));
    let mkv = MkvFile::new(file);
    let iter = if let Some(track_number) = track_number {
        mkv.block_iter_from_track_number(track_number)
    } else {
        mkv.block_iter(KnownLanguage::English)
    };
    if let Some(iter) = iter {
        let mut path = Path::new(output_path).to_owned();
        path.push("something");
        for (i, block) in iter.enumerate() {
            path.set_file_name(&format!("{}.bin", i));
            std::fs::write(&path, &block.payload).unwrap();
            if i >= num_subtitles {
                break;
            }
        }
    } else {
        println!("No English subtitles found!");
    }
    Ok(())
}

fn list_mkv_subtitles(
    mkv_path: &str,
    num_subtitles: usize,
    track_number: Option<u64>,
) -> Result<()> {
    // Collect subtitles from the file(s)
    println!("Loading subtitles from mkv files...");
    let files = process_input_path(&mkv_path, num_subtitles, track_number)?;
    print_subtitles(&files);
    Ok(())
}

fn list_srt_subtitles(srt_path: &str, num_subtitles: usize) -> Result<()> {
    // Collect subtitles from the file(s)
    println!("Loading subtitles from srt files...");
    let files = process_reference_path(&srt_path, num_subtitles)?;
    print_subtitles(&files);
    Ok(())
}

fn match_subtitles(
    mkv_path: &str,
    ref_path: &str,
    num_subtitles: usize,
    track_number: Option<u64>,
    max_distance: Option<usize>,
) -> Result<()> {
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
) -> Result<Vec<(String, Vec<String>)>> {
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
) -> Result<Vec<(String, Vec<String>)>> {
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
        let mut ref_file_name = ref_path
            .file_stem()
            .unwrap()
            .to_str()
            .unwrap()
            .replace(".eng", "");
        ref_file_name.push_str(".mkv");
        if mkv_file_name != ref_file_name {
            println!(
                "Rename-Item -Path \"{}\" -NewName \"{}\"",
                mkv_file_name, ref_file_name
            );
        }
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
            let (normalized_subtitle, normalized_ref_subtitle) = normalize_to_shortest_string(&subtitle, &ref_subtitle);

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
    use windows::core::Result;

    use crate::{compute_distances, flatten_subtitles, process_input_path, process_reference_path};

    #[test]
    fn popeye_basic_pgs() -> Result<()> {
        popeye_basic_subfolder(5, "pgs")
    }

    #[test]
    fn popeye_match_pgs() -> Result<()> {
        popeye_match_subfolder(5, "pgs")
    }

    #[test]
    fn popeye_basic_vob() -> Result<()> {
        popeye_basic_subfolder(5, "vob")
    }

    #[test]
    fn popeye_match_vob() -> Result<()> {
        popeye_match_subfolder(5, "vob")
    }

    fn popeye_basic_subfolder(num_subtitles: usize, subfolder: &str) -> Result<()> {
        let subtitles = process_input_path(
            &format!("data/popeye/mkv/{}", subfolder),
            num_subtitles,
            None,
        )?;
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
        // TODO: Reconcile ocr differences between test data
        match subfolder {
            "pgs" => {
                assert_eq!(iter.next(), Some(&("Title T00-1.mkv", "oh oh wwhat happened ohh let me go let me go let me go nonono dont drop me now oh man the lifeboats")));
                assert_eq!(iter.next(), Some(&("Title T01-2.mkv", "whos the most phenominal extra ordinary fellow yous sinbad the sailor how do you like that stooges on one of my travels i ran into this now there was a thrill id be sorry to miss")));
                assert_eq!(iter.next(), Some(&("Title T02-3.mkv", "woah whats this hey let me down you big overgrown canary what are you doing taking me for a ride or something come back to me there you are with gravy")));
                assert_eq!(iter.next(), Some(&("Title T03-4.mkv", "im sinbad the sailor so hearty and hale i live on an island on the back ofa whale its a whale of an island thats not a bad joke its lord and its master is this handsom bloke")));
            }
            "vob" => {
                assert_eq!(iter.next(), Some(&("Title T00-1.mkv", "ohl ohl w what happened ohh let me go let me go let me go nonono dont drop me now oh man the lifeboats")));
                assert_eq!(iter.next(), Some(&("Title T01-2.mkv", "whos the most phenom inal extra ordinary how do you like that stooges on one of my travels i ran into this now there was a thrill id be sorry to miss spread out his wings and the sunlight grew dim")));
                assert_eq!(iter.next(), Some(&("Title T02-3.mkv", "woah whats this hey let me down you big overgrown canary what are you doing taking me for a ride or something there you are with gravy laughter")));
                assert_eq!(iter.next(), Some(&("Title T03-4.mkv", "i m sinbad the sailor so hearty and i live on an island on the back of a thats not a bad joke its lord and its master is this handsom bloke whos the most remarkable extraordinary")));
            }
            _ => panic!("Unknown subfolder!"),
        }

        Ok(())
    }

    fn popeye_match_subfolder(num_subtitles: usize, subfolder: &str) -> Result<()> {
        let subtitles = process_input_path(
            &format!("data/popeye/mkv/{}", subfolder),
            num_subtitles,
            None,
        )?;
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
        assert_eq!(closest.len(), 4);

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
