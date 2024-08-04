use serde_json::{ Value, json };
use std::collections::HashMap;
use url::Url;
use crate::database::Webpage;

pub fn format_result(
    score: &f32,
    webpage: &Webpage,
    top_domains: &HashMap<String, usize>,
    include_links: bool
) -> Value {
    // Extract domain and get top website rank
    let domain = extract_domain_from_string(&webpage.url);
    let top_website_rank = domain.as_ref().and_then(|d| top_domains.get(d).cloned());

    // Create the base result JSON
    let mut result =
        json!({
        "title": webpage.title,
        "url": webpage.url,
        "description": webpage.description,
        "score": score,
        "keywords": webpage.keywords.iter().map(|(keyword, &occurrences)| {
            json!({ "keyword": keyword.word, "occurrences": occurrences })
        }).collect::<Vec<_>>(),
        "top_website_rank": top_website_rank,
    });
    
    // Add link information if requested
    if include_links {
        if let Some(links_to_count) = webpage.links_to_count {
            result["links_to_count"] = json!(links_to_count);
        }
        if let Some(links_from) = &webpage.links_from {
            result["links_from"] = json!(
                links_from
                    .iter()
                    .map(|(link, &count)| { json!({ "link": link, "occurrences": count }) })
                    .collect::<Vec<_>>()
            );
        }
    }

    result
}

pub fn extract_domain_from_string(url: &str) -> Option<String> {
    // Parse the URL and extract the host (domain)
    Url::parse(url)
        .ok()
        .and_then(|parsed_url| parsed_url.host_str().map(String::from))
}
