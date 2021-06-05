using System.Collections.Generic;
using System.IO;

namespace showorder
{
    static class SrtParser
    {
        public static List<string> ParseNSubtitles(string path, int numSubtitles)
        {
            var data = File.ReadAllText(path).Replace("\r\n", "\n");
            var chunks = data.Split("\n\n");

            var subtitles = new List<string>();
            foreach (var chunk in chunks)
            {
                if (!string.IsNullOrEmpty(chunk))
                {
                    var parts = chunk.Split('\n', 3);
                    var text = TextSanitizer.Sanitize(parts[2].Replace('\n', ' '));
                    if (!string.IsNullOrEmpty(text))
                    {
                        subtitles.Add(text);
                        if (subtitles.Count >= numSubtitles)
                        {
                            return subtitles;
                        }
                    }
                }
            }

            return subtitles;
        }
    }
}
