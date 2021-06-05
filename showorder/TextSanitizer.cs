using System.Text.RegularExpressions;

namespace showorder
{
    static class TextSanitizer
    {
        private static string RegexReplace(this string text, string pattern, string replacement)
        {
            return Regex.Replace(text, pattern, replacement);
        }
        private static string RegexRemove(this string text, string pattern)
        {
            return text.RegexReplace(pattern, string.Empty);
        }

        public static string Sanitize(string text)
        {
            return text.RegexRemove("<.*?>").RegexRemove("\\[.*?\\]").RegexRemove("[A-z]+:").Trim().ToLower();
        }
    }
}
