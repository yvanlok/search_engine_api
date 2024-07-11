use std::collections::HashMap;
use crate::database::Webpage;

pub async fn get_tf_idf_scores(
    document_count: i64,
    lemmatized_query: &[String],
    websites: &[Webpage]
) -> Vec<(f32, Webpage)> {
    // Calculate query term frequencies
    let query_term_tfs = calculate_query_term_frequencies(lemmatized_query);

    // Calculate TF-IDF scores and similarities for each website
    let mut website_similarities: Vec<(f32, Webpage)> = websites
        .iter()
        .map(|website| {
            let similarity = calculate_similarity(website, &query_term_tfs, document_count);
            (similarity, website.clone())
        })
        .collect();

    // Sort websites by similarity score in descending order
    website_similarities.sort_unstable_by(|a, b|
        b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal)
    );

    website_similarities
}

fn calculate_query_term_frequencies(lemmatized_query: &[String]) -> HashMap<String, f32> {
    let mut query_word_occurrences = HashMap::new();
    let total_query_terms = lemmatized_query.len() as f32;

    // Count occurrences of each query term
    for word in lemmatized_query {
        *query_word_occurrences.entry(word).or_insert(0) += 1;
    }

    // Calculate term frequencies
    query_word_occurrences
        .into_iter()
        .map(|(word, count)| (word.to_string(), (count as f32) / total_query_terms))
        .collect()
}

fn calculate_similarity(
    website: &Webpage,
    query_term_tfs: &HashMap<String, f32>,
    document_count: i64
) -> f32 {
    let mut query_vector_sum = 0.0;
    let mut document_vector_sum = 0.0;
    let mut dot_product = 0.0;

    for (word, occurrences) in &website.keywords {
        let tf = (*occurrences as f32) / (website.word_count as f32);
        let idf = ((document_count as f32) / (word.documents_containing_word as f32)).ln().max(0.0);
        let tf_idf = tf * idf;

        if let Some(&query_tf) = query_term_tfs.get(&word.word) {
            let query_tf_idf = query_tf * idf;
            query_vector_sum += query_tf_idf.powi(2);
            document_vector_sum += tf_idf.powi(2);
            dot_product += query_tf_idf * tf_idf;
        }
    }

    let query_vector = query_vector_sum.sqrt();
    let document_vector = document_vector_sum.sqrt();

    // Calculate cosine similarity
    if query_vector > 0.0 && document_vector > 0.0 {
        dot_product / (query_vector * document_vector)
    } else {
        0.0
    }
}
