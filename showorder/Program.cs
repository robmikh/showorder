using Matroska;
using Matroska.Models;
using MinimumEditDistance;
using System;
using System.Collections.Concurrent;
using System.Collections.Generic;
using System.IO;
using System.Linq;
using System.Threading.Tasks;
using Windows.Globalization;
using Windows.Media.Ocr;

namespace showorder
{
    class Program
    {
        static void Main(string[] args)
        {
            if (args.Length < 1)
            {
                throw new Exception("Invalid number of arguments!");
            }
            var path1 = args[0];

            // TODO: Make num subtitles configurable
            var numSubtitles = 5;

            // Collect subtitles from the file(s)
            Console.WriteLine("Loading subtitles from mkv files...");
            var files = ProcessInputPath(path1, numSubtitles);

            // If we couldn't find any subtitles, exit
            if (files.Count == 0)
            {
                Console.WriteLine("No English subtitles found!");
                return;
            }

            // If we have a second param, use it to compare the subtitles. Otherwise,
            // print the summary and complete.
            if (args.Length < 2)
            {
                PrintSubtitles(files);
                return;
            }
            var path2 = args[1];

            // Load reference data
            Console.WriteLine("Loading reference data...");
            var referenceFiles = ProcessReferencePath(path2, numSubtitles);

            // Flatten our data
            var subtitles = FlattenSubtitles(files);
            var refSubtitles = FlattenSubtitles(referenceFiles);

            // Compare subtitles
            Console.WriteLine("Comparing subtitles...");
            var distances = new Dictionary<string, List<(string, int)>>();
            foreach (var (file, subtitle) in subtitles)
            {
                Console.WriteLine($"  Inspecting \"{Path.GetFileName(file)}\"");
                foreach (var (refFile, refSubtitle) in refSubtitles)
                {
                    // Normalize to shortest
                    var length = Math.Min(subtitle.Length, refSubtitle.Length);
                    var normalizedSubtitle = subtitle.Substring(0, length);
                    var normalizedRefSubtitle = refSubtitle.Substring(0, length);

                    //Console.WriteLine("Comparing:");
                    //Console.WriteLine($"  {normalizedSubtitle}");
                    //Console.WriteLine("  to");
                    //Console.WriteLine($"  {normalizedRefSubtitle}");
                    //Console.WriteLine("");
                    var distance = Levenshtein.CalculateDistance(normalizedSubtitle, normalizedRefSubtitle, 1);
                    if (distances.ContainsKey(file))
                    {
                        var list = distances[file];
                        list.Add((refFile, distance));
                    }
                    else
                    {
                        var matches = new List<(string, int)>();
                        matches.Add((refFile, distance));
                        distances.Add(file, matches);
                    }
                }
            }

            // Sort distances
            foreach (var (_, fileDistances) in distances)
            {
                fileDistances.Sort((x, y) => x.Item2.CompareTo(y.Item2));
            }

            // Output distances
            PrintDistances(distances);

            // Map files to reference files
            var mappings = new List<(string, string)>();
            var unmapped = new List<string>();
            foreach (var (mkvPath, fileDistances) in distances)
            {
                // First will be the lowest
                var (refFile, distance) = fileDistances.First();
                // TODO: Make min distance configurable
                if (distance < 3 * numSubtitles)
                {
                    mappings.Add((mkvPath, refFile));
                }
                else
                {
                    unmapped.Add(mkvPath);
                }
            }

            // Output mappings
            PrintMappings(mappings);
            PrintUmapped(unmapped);
        }

        static void PrintUmapped(List<string> unmapped)
        {
            if (unmapped.Count > 0)
            {
                Console.WriteLine("Unmapped mkv files: ");
                foreach (var mkvPath in unmapped)
                {
                    var fileName = Path.GetFileName(mkvPath);
                    Console.WriteLine($"  {fileName}");
                }
            }
        }

        static void PrintMappings(IEnumerable<(string, string)> mappings)
        {
            Console.WriteLine("Results: ");
            foreach (var (mkvPath, refFile) in mappings)
            {
                var fileName = Path.GetFileName(mkvPath);
                var refFileName = Path.GetFileName(refFile);
                Console.WriteLine($"  {fileName} -> {refFileName}");
            }
        }

        static void PrintDistances(IDictionary<string, List<(string, int)>> distances)
        {
            Console.WriteLine("Distances: ");
            foreach (var (mkvPath, fileDistances) in distances)
            {
                var fileName = Path.GetFileName(mkvPath);
                Console.WriteLine($"{fileName} :");
                foreach (var (refFile, distance) in fileDistances)
                {
                    Console.WriteLine($"  {distance} - {Path.GetFileName(refFile)}");
                }
            }
        }

        static void PrintSubtitles(IEnumerable<(string, List<string>)> files)
        {
            foreach (var (file, subtitles) in files)
            {
                Console.WriteLine(Path.GetFileName(file));
                foreach (var subtitle in subtitles)
                {
                    Console.WriteLine($"  \"{subtitle}\"");
                }
            }
        }

        static List<(string, string)> FlattenSubtitles(IEnumerable<(string, List<string>)> files)
        {
            var subtitles = new List<(string, string)>();
            foreach (var (file, fileSubtitles) in files)
            {
                var subtitle = string.Join(' ', fileSubtitles);
                subtitles.Add((file, subtitle));
            }
            return subtitles;
        }

        static ConcurrentBag<(string, List<string>)> ProcessReferencePath(string path, int numSubtitles)
        {
            var referenceFiles = new ConcurrentBag<(string, List<string>)>();
            if (Directory.Exists(path))
            {
                Parallel.ForEach(Directory.GetFiles(path, "*.srt"), file =>
                {
                    var subtitles = SrtParser.ParseNSubtitles(file, numSubtitles);
                    referenceFiles.Add((file, subtitles));
                });
            }
            else if (File.Exists(path) && Path.GetExtension(path) == ".srt")
            {
                var subtitles = SrtParser.ParseNSubtitles(path, numSubtitles);
                referenceFiles.Add((path, subtitles));
            }
            else
            {
                throw new Exception($"Invalid reference path: \"{path}\"");
            }
            return referenceFiles;
        }

        static ConcurrentBag<(string, List<string>)> ProcessInputPath(string path, int numSubtitles)
        {
            var files = new ConcurrentBag<(string, List<string>)>();
            if (Directory.Exists(path))
            {
                Parallel.ForEach(Directory.GetFiles(path, "*.mkv"), file =>
                {
                    ProcessMkvFile(file, files, numSubtitles);
                });
            }
            else if (File.Exists(path) && Path.GetExtension(path) == ".mkv")
            {
                ProcessMkvFile(path, files, numSubtitles);
            }
            else
            {
                throw new Exception($"Invalid input path: \"{path}\"");
            }
            return files;
        }

        static void ProcessMkvFile(string path, ConcurrentBag<(string, List<string>)> results, int numSubtitles)
        {
            var engine = OcrEngine.TryCreateFromLanguage(new Language("en-US"));
            if (GetFirstFewSubtitiles(path, engine, numSubtitles) is List<string> subtitles)
            {
                // Sometimes there's a subtitle track with no subtitles in it...
                if (subtitles.Count > 0)
                {
                    results.Add((path, subtitles));
                }
            }
        }

        static List<string>? GetFirstFewSubtitiles(string path, OcrEngine engine, int num)
        {
            var doc = MatroskaSerializer.Deserialize(new FileStream(path, FileMode.Open, FileAccess.Read));
            if (FindTrackNumber(doc) is ulong trackNumber)
            {
                return GetFirstFewSubtitles(doc, engine, trackNumber, 5);
            }
            else
            {
                return null;
            }
        }

        static List<string> GetFirstFewSubtitles(MatroskaDocument doc, OcrEngine engine, ulong trackNumber, int num)
        {
            var list = new List<string>(num);
            foreach (var cluster in doc.Segment.Clusters)
            {
                if (cluster.BlockGroups != null)
                {
                    foreach (var blockGroup in cluster.BlockGroups)
                    {
                        foreach (var block in blockGroup.Blocks)
                        {
                            if (block.TrackNumber == trackNumber)
                            {
                                var bitmap = PgsParser.ParseSegments(block.Data);
                                if (bitmap != null)
                                {
                                    var result = engine.RecognizeAsync(bitmap).AsTask().Result;
                                    if (result != null)
                                    {
                                        // Skip empty subtitles
                                        if (!string.IsNullOrEmpty(result.Text))
                                        {
                                            var text = TextSanitizer.Sanitize(result.Text);
                                            if (!string.IsNullOrEmpty(text))
                                            {
                                                list.Add(text);
                                                if (list.Count >= num)
                                                {
                                                    return list;
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
            return list;
        }

        static ulong? FindTrackNumber(MatroskaDocument doc)
        {
            ulong? trackNumber = null;
            foreach (var trackEntry in doc.Segment.Tracks.TrackEntries)
            {
                // We're only looking for subtitles
                if (trackEntry.TrackType == 0x11)
                {
                    // We currently only support pgs
                    if (trackEntry.CodecID == "S_HDMV/PGS")
                    {
                        var language = trackEntry.Language;
                        // For now we assume English
                        if (language == "eng")
                        {
                            trackNumber = trackEntry.TrackNumber;
                            break;
                        }
                    }
                }
            }
            return trackNumber;
        }
    }
}
