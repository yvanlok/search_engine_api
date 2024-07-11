use std::io::{ BufRead, BufReader };
use std::fs::File;
use std::collections::HashMap;
use std::path::Path;
use once_cell::sync::Lazy;
use regex::Regex;

// Global static for storing the lemma mappings.
static LEMMA_MAP: Lazy<HashMap<String, String>> = Lazy::new(|| {
    load_lemma_map("lemmatised_words.txt").expect("Failed to load lemma map")
});

// Global static for the punctuation removal regex.
static PUNCTUATION_REGEX: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"[^a-zA-Z0-9\s]").expect("Failed to compile punctuation regex")
});

/// Loads the lemma map from a file.
///
/// # Arguments
///
/// * `filename` - The path to the file containing lemma mappings.
///
/// # Returns
///
/// A `Result` containing the `HashMap` of lemma mappings or an error.
fn load_lemma_map<P: AsRef<Path>>(filename: P) -> Result<HashMap<String, String>, std::io::Error> {
    let file = File::open(filename)?;
    let reader = BufReader::new(file);
    let mut map = HashMap::new();

    let re = Regex::new(r"^([^/]+)[^->]*->(.+)$").expect("Failed to compile lemma regex");
    for line in reader.lines() {
        let line = line?;
        if let Some(captures) = re.captures(&line) {
            let lemma = captures[1].trim().to_string();
            let words = captures[2].split(',').map(|word| word.trim().to_string());
            for word in words {
                map.insert(word, lemma.clone());
            }
        }
    }
    Ok(map)
}

/// Lemmatizes a given string using the global lemma map.
///
/// # Arguments
///
/// * `text` - The input string to lemmatize.
///
/// # Returns
///
/// A vector of lemmatized words.
pub fn lemmatise_string(text: &str) -> Vec<String> {
    let text = text.to_lowercase();
    let text_no_punct = PUNCTUATION_REGEX.replace_all(&text, "");
    text_no_punct
        .split_whitespace()
        .map(|word| LEMMA_MAP.get(word).unwrap_or(&word.to_string()).clone())
        .collect()
}

/// Lemmatizes a given string using a custom lemma map.
///
/// # Arguments
///
/// * `text` - The input string to lemmatize.
/// * `custom_map` - A reference to a custom HashMap for lemmatization.
///
/// # Returns
///
/// A vector of lemmatized words.
pub fn lemmatise_string_with_custom_map(
    text: &str,
    custom_map: &HashMap<String, String>
) -> Vec<String> {
    let text = text.to_lowercase();
    let text_no_punct = PUNCTUATION_REGEX.replace_all(&text, "");
    text_no_punct
        .split_whitespace()
        .map(|word| custom_map.get(word).unwrap_or(&word.to_string()).clone())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_lemmatise_string() {
        let input = "The quick brown foxes are jumping over the lazy dogs";
        let expected = vec![
            "the",
            "quick",
            "brown",
            "fox",
            "be",
            "jump",
            "over",
            "the",
            "lazy",
            "dog"
        ];
        assert_eq!(lemmatise_string(input), expected);
    }

    #[test]
    fn test_lemmatise_string_with_custom_map() {
        let mut custom_map = HashMap::new();
        custom_map.insert("running".to_string(), "run".to_string());
        custom_map.insert("jumped".to_string(), "jump".to_string());

        let input = "The fox is running and jumped over the fence";
        let expected = vec!["the", "fox", "is", "run", "and", "jump", "over", "the", "fence"];
        assert_eq!(lemmatise_string_with_custom_map(input, &custom_map), expected);
    }
}
