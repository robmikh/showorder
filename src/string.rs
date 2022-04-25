pub fn normalize_to_shortest_string<'a>(string1: &'a str, string2: &'a str) -> (&'a str, &'a str) {
    let (string1_len, _) = string1.char_indices().enumerate().last().unwrap();
    let (string2_len, _) = string2.char_indices().enumerate().last().unwrap();

    let len = string1_len.min(string2_len);

    let str1 = if string1_len == len {
        string1
    } else {
        substring(string1, len)
    };
    let str2 = if string2_len == len {
        string2
    } else {
        substring(string2, len)
    };

    (str1, str2)
}

fn substring(string: &str, len: usize) -> &str {
    let (end, _) = string.char_indices().nth(len).unwrap();
    &string[..end]
}
