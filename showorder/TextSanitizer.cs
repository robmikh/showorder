using System.Text.RegularExpressions;

namespace showorder
{
    static class TextSanitizer
    {
        public static string Sanitize(string text)
        {
            var text1 = Regex.Replace(text, "<.*?>", string.Empty);
            var text2 = Regex.Replace(text1, "\\[.*?\\]", string.Empty);
            return text2;
        }
    }
}
