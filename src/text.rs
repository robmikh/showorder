static BANNED_WORDS: [&'static str; 6] = ["caption", "subtitle", "subbed", "corrections by", "corrected by", "correction by"];

trait ContainsAny {
    fn contains_any(&self, substrings: &[&str]) -> bool;
}

impl ContainsAny for String {
    fn contains_any(&self, substrings: &[&str]) -> bool {
        for substring in substrings {
            if self.contains(substring) {
                return true;
            }
        }
        false
    }
}

trait RegexRemove {
    fn regex_remove(&self, pattern: &str) -> String;
}

impl RegexRemove for String {
    fn regex_remove(&self, pattern: &str) -> String {
        let regex = regex::Regex::new(pattern).unwrap();
        let result = regex.replace_all(self, "");
        result.to_string()
    }
}

trait RemovePunctuation {
    fn remove_punctuation(&self) -> Self;
}

impl RemovePunctuation for String {
    fn remove_punctuation(&self) -> Self {
        let mut result = String::new();
        for c in self.chars() {
            if !c.is_ascii_punctuation() {
                result.push(c);
            }
        }
        result
    }
}

pub fn sanitize_text(text: &str) -> String {
    let lowered = text.to_lowercase();
    if lowered.contains_any(&BANNED_WORDS) {
        return String::new();
    }
    lowered
        .regex_remove(r"<.*?>")
        .regex_remove(r"\[.*?\]")
        .regex_remove(r"\(.*?\)")
        .regex_remove(r"[A-z]+:")
        .remove_punctuation()
        .trim()
        .to_string()
}
