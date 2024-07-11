use axum::{ routing::get, Router, response::Json };
use axum::extract::{ Query, Extension };
use std::collections::HashMap;
use serde_json::{ Value, json };
use sqlx::PgPool;
use dotenv::dotenv;
use tokio::fs::File;
use tokio::io::{ self, AsyncBufReadExt };
use url::Url;
use std::time::Instant;

mod lemmatise;
mod database;
mod ranking;

/// Struct to store timing information for various parts of the request processing
#[derive(Default, Clone)]
struct RequestTiming {
    start: Option<Instant>,
    lemmatisation: Option<std::time::Duration>,
    database_query: Option<std::time::Duration>,
    tf_idf_calculation: Option<std::time::Duration>,
    results_formatting: Option<std::time::Duration>,
    total_search_function: Option<std::time::Duration>,
}

#[tokio::main]
async fn main() {
    // Load environment variables from .env file
    dotenv().ok();

    // Connect to the database
    let database_url = std::env::var("DATABASE_URL").expect("DATABASE_URL must be set");
    let pool = PgPool::connect(&database_url).await.expect("Failed to connect to database");
    let website_count = database::count_websites(&pool).await.expect("Failed to count websites");

    println!("Connected to database. Found {} websites.", website_count);

    // Load top domains
    let top_domains = load_top_domains("top-1m.txt").await.expect("Failed to load top domains");

    // Set up the Axum router
    let app = Router::new()
        .route("/", get(search))
        .layer(Extension(pool))
        .layer(Extension(website_count))
        .layer(Extension(top_domains))
        .layer(axum::middleware::map_request(timing_middleware));

    // Start the server
    let listener = tokio::net::TcpListener::bind("0.0.0.0:3000").await.unwrap();
    println!("Listening on: http://{}", listener.local_addr().unwrap());
    axum::serve(listener, app).await.unwrap();
}

async fn timing_middleware(
    mut request: axum::http::Request<axum::body::Body>
) -> axum::http::Request<axum::body::Body> {
    // Add timing information to the request extensions
    let timing = RequestTiming {
        start: Some(Instant::now()),
        ..Default::default()
    };
    request.extensions_mut().insert(timing);
    request
}

async fn search(
    Query(params): Query<HashMap<String, String>>,
    Extension(pool): Extension<PgPool>,
    Extension(website_count): Extension<i64>,
    Extension(top_domains): Extension<HashMap<String, usize>>,
    mut timing: Extension<RequestTiming>
) -> Json<Value> {
    let search_start = Instant::now();

    // Extract query parameters
    let query = params.get("q").expect("Missing query parameter");
    let include_links = params
        .get("links")
        .map(|v| v == "true")
        .unwrap_or(false);
    let num_results = params
        .get("results")
        .and_then(|v| v.parse().ok())
        .unwrap_or(100);

    // Lemmatize the query
    let lemmatise_time = Instant::now();
    let keywords = lemmatise::lemmatise_string(query);
    timing.lemmatisation = Some(lemmatise_time.elapsed());

    // Fetch webpages from the database
    let db_time = Instant::now();
    let webpages = match database::fetch_webpages(&pool, &keywords, include_links).await {
        Ok(webpages) => webpages,
        Err(e) => {
            return Json(json!({ "error": e.to_string() }));
        }
    };
    timing.database_query = Some(db_time.elapsed());

    // Calculate TF-IDF scores and rank webpages
    let tfidf_time = Instant::now();
    let ranked_webpages = ranking::get_tf_idf_scores(website_count, &keywords, &webpages).await;

    // Limit the number of results
    let ranked_webpages: Vec<(f32, database::Webpage)> = ranked_webpages
        .into_iter()
        .take(num_results)
        .collect();

    timing.tf_idf_calculation = Some(tfidf_time.elapsed());

    // Format the results
    let results_time = Instant::now();
    let results: Vec<Value> = ranked_webpages
        .iter()
        .map(|(score, webpage)| format_result(score, webpage, &top_domains, include_links))
        .collect();
    timing.results_formatting = Some(results_time.elapsed());

    timing.total_search_function = Some(search_start.elapsed());

    let total_request_time = timing.start.unwrap().elapsed();

    // Return the JSON response
    Json(
        json!({
        "query": query,
        "lemmatised_keywords": keywords,
        "results_count": results.len(),
        "time_taken": format_timing_info(&timing, total_request_time),
        "website_count": website_count,
        "results": results,
    })
    )
}

fn format_result(
    score: &f32,
    webpage: &database::Webpage,
    top_domains: &HashMap<String, usize>,
    include_links: bool
) -> Value {
    // Extract domain and get top website rank
    let domain = extract_domain_from_string(&webpage.url);
    let top_website_rank = domain
        .as_ref()
        .and_then(|d| top_domains.get(d).cloned())
        .unwrap_or_default();

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

fn format_timing_info(timing: &RequestTiming, total_request_time: std::time::Duration) -> Value {
    json!({
        "total_request": format!("{:?}", total_request_time),
        "total_search_function": format!("{:?}", timing.total_search_function.unwrap()),
        "lemmatisation": format!("{:?}", timing.lemmatisation.unwrap()),
        "database_query": format!("{:?}", timing.database_query.unwrap()),
        "tf_idf_calculation": format!("{:?}", timing.tf_idf_calculation.unwrap()),
        "results_formatting": format!("{:?}", timing.results_formatting.unwrap()),
        "other_operations": format!("{:?}", total_request_time - timing.total_search_function.unwrap()),
    })
}

fn extract_domain_from_string(url: &str) -> Option<String> {
    // Parse the URL and extract the host (domain)
    Url::parse(url)
        .ok()
        .and_then(|parsed_url| parsed_url.host_str().map(String::from))
}

async fn load_top_domains(filename: &str) -> io::Result<HashMap<String, usize>> {
    // Open and read the file containing top domains
    let file = File::open(filename).await?;
    let reader = io::BufReader::new(file);
    let mut top_domains = HashMap::new();

    // Parse each line and insert into the HashMap
    let mut lines = reader.lines();
    let mut rank = 1;
    while let Some(line) = lines.next_line().await? {
        top_domains.insert(line, rank);
        rank += 1;
    }
    Ok(top_domains)
}
