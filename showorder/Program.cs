using Matroska;
using Matroska.Models;
using MinimumEditDistance;
using System;
using System.Collections.Generic;
using System.IO;
using Windows.Globalization;
using Windows.Media.Ocr;

namespace showorder
{
    class Program
    {
        static TextWriter DummyWriter = new DummyWriter();

        static void Main(string[] args)
        {
            if (args.Length < 1)
            {
                throw new Exception("Invalid number of arguments!");
            }
            var path1 = args[0];

            // TODO: Make num subtitles configurable
            var numSubtitles = 5;
            var engine = OcrEngine.TryCreateFromLanguage(new Language("en-US"));

            // Collect subtitles from the file(s)
            Console.WriteLine("Loading subtitles from mkv files...");
            var files = new List<(string, List<string>)>();
            if (Directory.Exists(path1))
            {
                foreach (var file in Directory.GetFiles(path1, "*.mkv"))
                {
                    if (GetFirstFewSubtitiles(file, engine, numSubtitles) is List<string> subtitles)
                    {
                        // Sometimes there's a subtitle track with no subtitles in it...
                        if (subtitles.Count > 0)
                        {
                            files.Add((file, subtitles));
                        }
                    }
                }
            }
            else if (File.Exists(path1) && Path.GetExtension(path1) == ".mkv")
            {
                if (GetFirstFewSubtitiles(path1, engine, numSubtitles) is List<string> subtitles)
                {
                    // Sometimes there's a subtitle track with no subtitles in it...
                    if (subtitles.Count > 0)
                    {
                        files.Add((path1, subtitles));
                    }
                }
            }
            else
            {
                throw new Exception($"Invalid input: \"{path1}\"");
            }

            // If we couldn't find any subtitles, exit
            if (files.Count == 0)
            {
                Console.WriteLine("No english subtitles found!");
                return;
            }

            // If we have a second param, use it to compare the subtitles. Otherwise,
            // print the summary and complete.
            if (args.Length < 2)
            {
                foreach (var (file, subtitles) in files)
                {
                    Console.WriteLine(file);
                    foreach (var subtitle in subtitles)
                    {
                        Console.WriteLine($"  \"{subtitle}\"");
                    }
                }
                return;
            }
            var path2 = args[1];

            // Load reference data
            Console.WriteLine("Loading reference data...");
            var referenceFiles = new List<(string, List<string>)>();
            if (Directory.Exists(path2))
            {
                foreach (var file in Directory.GetFiles(path2, "*.srt"))
                {
                    var subtitles = SrtParser.ParseNSubtitles(file, numSubtitles);
                    referenceFiles.Add((file, subtitles));
                }
            }
            else if (File.Exists(path2) && Path.GetExtension(path2) == ".srt")
            {
                var subtitles = SrtParser.ParseNSubtitles(path2, numSubtitles);
                referenceFiles.Add((path2, subtitles));
            }
            else
            {
                throw new Exception($"Invalid input: \"{path2}\"");
            }

            // Compare subtitles
            Console.WriteLine("Comparing subtitles...");
            var mapping = new Dictionary<string, List<string>>();
            foreach (var (file, subtitles) in files)
            {
                Console.WriteLine($"  Inspecting \"{Path.GetFileName(file)}\"");
                foreach (var (refFile, refSubtitles) in referenceFiles)
                {
                    var match = true;
                    for (var i = 0; i < subtitles.Count; i++)
                    {
                        var subtitle = subtitles[i];
                        var refSubtitle = refSubtitles[i];

                        // TODO: Make min distance configurable
                        var distance = Levenshtein.CalculateDistance(subtitle, refSubtitle, 1);
                        if (distance >= 3)
                        {
                            match = false;
                            break;
                        }
                    }
                    if (match)
                    {
                        if (mapping.ContainsKey(file))
                        {
                            var list = mapping[file];
                            list.Add(refFile);
                        }
                        else
                        {
                            var matches = new List<string>();
                            matches.Add(refFile);
                            mapping.Add(file, matches);
                        }
                    }
                }
            }

            // Output results
            Console.WriteLine("Results: ");
            foreach (var (key, value) in mapping)
            {
                var fileName = Path.GetFileName(key);
                Console.WriteLine($"{fileName} :");
                foreach (var entry in value)
                {
                    Console.WriteLine($"  {Path.GetFileName(entry)}");
                }
            }
        }

        static List<string>? GetFirstFewSubtitiles(string path, OcrEngine engine, int num)
        {
            // Matroska writes to the console :(
            // This is a dirty trick to stop that
            var oldOut = Console.Out;
            Console.SetOut(DummyWriter);
            var doc = MatroskaSerializer.Deserialize(new FileStream(path, FileMode.Open, FileAccess.Read));
            Console.SetOut(oldOut);
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
