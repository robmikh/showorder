using System.Text;
using System.Text.RegularExpressions;

namespace showorder
{
    static class TextSanitizer
    {
        private static readonly string[] BannedWords = new string[] { "caption", "subtitle", "subbed" };

        private static string RegexReplace(this string text, string pattern, string replacement)
        {
            return Regex.Replace(text, pattern, replacement);
        }

        private static string RegexRemove(this string text, string pattern)
        {
            return text.RegexReplace(pattern, string.Empty);
        }

        private static string RemovePunctuation(this string text)
        {
            var builder = new StringBuilder(text.Length);
            foreach (var c in text)
            {
                if (!char.IsPunctuation(c))
                {
                    builder.Append(c);
                }
            }
            return builder.ToString();
        }

        private static bool ContainsAny(this string text, string[] substrings)
        {
            foreach (var substring in substrings)
            {
                if (text.Contains(substring))
                {
                    return true;
                }
            }
            return false;
        }

        public static string Sanitize(string text)
        {
            var lowered = text.ToLower();
            if (lowered.ContainsAny(BannedWords))
            {
                return string.Empty;
            }
            return lowered.RegexRemove("<.*?>").RegexRemove("\\[.*?\\]").RegexRemove("[A-z]+:").RemovePunctuation().Trim();
        }
    }
}
