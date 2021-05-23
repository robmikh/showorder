using System.Collections.Generic;
using System.IO;
using System.Text.RegularExpressions;

namespace showorder
{
    static class SrtParser
    {
        public static List<string> ParseNSubtitles(string path, int numSubtitles)
        {
            var data = File.ReadAllText(path);
            var chunks = data.Split("\n\n");

            var subtitles = new List<string>();
            foreach (var chunk in chunks)
            {
                if (!string.IsNullOrEmpty(chunk))
                {
                    var parts = chunk.Split('\n', 3);
                    var text = parts[2].Replace('\n', ' ');
                    // We also need to remove tags
                    subtitles.Add(Regex.Replace(text, "<.*?>", string.Empty));
                    if (subtitles.Count >= numSubtitles)
                    {
                        return subtitles;
                    }
                }
            }

            return subtitles;
        }
    }
}
