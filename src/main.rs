use axum::{
    routing::get,
    Router,
    response::Json,
    http::{ HeaderValue, Method },
    extract::{ Query, Extension, ConnectInfo },
};
use std::collections::HashMap;
use serde_json::{ Value, json };
use sqlx::PgPool;
use dotenv::dotenv;
use tokio::fs::File;
use tokio::io::{ self, AsyncBufReadExt };
use url::Url;
use std::time::{ Duration, Instant, SystemTime, UNIX_EPOCH };
use tower_http::cors::CorsLayer;
use reqwest::Client;
use std::sync::Arc;
use tokio::sync::Mutex;
use std::net::SocketAddr;

mod lemmatise;
mod database;
mod ranking;

/// Struct to store timing information for various parts of the request processing
#[derive(Default, Clone)]
struct RequestTiming {
    start: Option<Instant>,
    lemmatisation: Option<Duration>,
    initial_database_query: Option<Duration>,
    tf_idf_calculation: Option<Duration>,
    link_fetching: Option<Duration>,
    results_formatting: Option<Duration>,
    total_search_function: Option<Duration>,
}

struct TokenCache {
    tokens: HashMap<String, (u64, String)>,
}

impl TokenCache {
    fn new() -> Self {
        TokenCache {
            tokens: HashMap::new(),
        }
    }

    fn is_valid(&mut self, token: &str, ip: &str) -> bool {
        let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs();
        if let Some((timestamp, stored_ip)) = self.tokens.get(token) {
            if now - timestamp <= 120 && stored_ip == ip {
                // 2 minutes
                return true;
            }
        }
        false
    }

    fn add_token(&mut self, token: String, ip: String) {
        let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs();
        self.tokens.insert(token, (now, ip));
    }

    fn clean_old_tokens(&mut self) {
        let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs();
        self.tokens.retain(|_, &mut (timestamp, _)| now - timestamp <= 120);
    }
}

#[tokio::main]
async fn main() {
    dotenv().ok();

    let database_url = std::env::var("DATABASE_URL").expect("DATABASE_URL must be set");
    let pool = PgPool::connect(&database_url).await.expect("Failed to connect to database");
    let website_count = database::count_websites(&pool).await.expect("Failed to count websites");

    println!("Connected to database. Found {} websites.", website_count);

    let top_domains = load_top_domains("top-1m.txt").await.expect("Failed to load top domains");

    let max_results: usize = std::env
        ::var("MAX_RESULTS")
        .unwrap_or_else(|_| "100".to_string())
        .parse()
        .expect("MAX_RESULTS must be a valid number");

    let token_cache = Arc::new(Mutex::new(TokenCache::new()));

    // Create a CORS layer
    let cors = CorsLayer::new()
        .allow_origin("http://localhost:3001".parse::<HeaderValue>().unwrap())
        .allow_methods([Method::GET])
        .allow_headers(vec![axum::http::header::CONTENT_TYPE]);

    // Set up the Axum router with CORS
    let app = Router::new()
        .route("/", get(search))
        .layer(Extension(pool))
        .layer(Extension(website_count))
        .layer(Extension(top_domains))
        .layer(Extension(max_results))
        .layer(Extension(Client::new()))
        .layer(Extension(token_cache))
        .layer(cors)
        .layer(axum::middleware::map_request(timing_middleware));

    let listener = tokio::net::TcpListener::bind("0.0.0.0:3000").await.unwrap();
    println!("Listening on: http://{}", listener.local_addr().unwrap());
    axum::serve(listener, app.into_make_service_with_connect_info::<SocketAddr>()).await.unwrap();
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
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    Query(params): Query<HashMap<String, String>>,
    Extension(pool): Extension<PgPool>,
    Extension(website_count): Extension<i64>,
    Extension(top_domains): Extension<HashMap<String, usize>>,
    Extension(_max_results): Extension<usize>,
    Extension(client): Extension<Client>,
    Extension(token_cache): Extension<Arc<Mutex<TokenCache>>>,
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
    let num_results = num_results.min(_max_results);

    // Extract Turnstile token
    let turnstile_token = params.get("token").expect("Missing Turnstile token");

    // Check if the token is in the cache

    let ip = addr.ip().to_string();
    let mut cache = token_cache.lock().await;
    if !cache.is_valid(turnstile_token, ip.as_str()) {
        // If not in cache, validate with Turnstile
        if !validate_turnstile_token(&client, turnstile_token).await {
            println!("Token validation failed for IP: {}", addr.ip());
            return Json(json!({ "error": "Invalid Turnstile token" }));
        } else {
            println!("Token validated for IP: {}", ip);
        }
        // If valid, add to cache
        cache.add_token(turnstile_token.to_string(), ip);
    } else {
        println!("Token already in cache for IP: {}", ip);
    }
    // Clean old tokens from cache
    cache.clean_old_tokens();
    drop(cache);

    // Lemmatize the query
    let lemmatise_time = Instant::now();
    let keywords = lemmatise::lemmatise_string(query);
    timing.lemmatisation = Some(lemmatise_time.elapsed());

    // Fetch webpages from the database (without links initially)
    let db_time = Instant::now();
    let webpages = match database::fetch_webpages(&pool, &keywords, false).await {
        Ok(webpages) => webpages,
        Err(e) => {
            return Json(json!({ "error": e.to_string() }));
        }
    };

    let matching_webpages = webpages.len();

    timing.initial_database_query = Some(db_time.elapsed());

    // Calculate TF-IDF scores and rank webpages
    let tfidf_time = Instant::now();
    let mut ranked_webpages = ranking::get_tf_idf_scores(website_count, &keywords, &webpages).await;

    // Sort ranked_webpages by score in descending order
    ranked_webpages.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap());

    // Determine the number of results to return
    let results_to_return = if !ranked_webpages.is_empty() && ranked_webpages[0].0 == 1.0 {
        let count_score_one = ranked_webpages
            .iter()
            .take_while(|(score, _)| *score == 1.0)
            .count();
        count_score_one.max(num_results)
    } else {
        num_results
    };

    // Limit the number of results
    ranked_webpages.truncate(results_to_return);
    timing.tf_idf_calculation = Some(tfidf_time.elapsed());

    // Fetch links for top results if requested
    if include_links {
        let link_time = Instant::now();
        let webpage_ids: Vec<i32> = ranked_webpages
            .iter()
            .map(|(_, webpage)| webpage.id)
            .collect();

        let links = database::fetch_links_for_ids(&pool, &webpage_ids).await.unwrap_or_default();

        for (_score, webpage) in &mut ranked_webpages {
            if let Some((links_to_count, links_from)) = links.get(&webpage.id) {
                webpage.links_to_count = Some(*links_to_count);
                webpage.links_from = Some(links_from.clone());
            }
        }
        timing.link_fetching = Some(link_time.elapsed());
    }

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
        "matching_webpages": matching_webpages,
        "time_taken": format_timing_info(&timing, total_request_time),
        "website_count": website_count,
        "results": results,
    })
    )
}

async fn validate_turnstile_token(client: &Client, token: &str) -> bool {
    let secret_key = std::env
        ::var("CLOUDFLARE_TURNSTILE_SECRET_KEY")
        .expect("CLOUDFLARE_TURNSTILE_SECRET_KEY must be set");
    let url = "https://challenges.cloudflare.com/turnstile/v0/siteverify";

    let params = [
        ("secret", secret_key),
        ("response", token.to_string()),
    ];

    let response = client.post(url).form(&params).send().await;

    match response {
        Ok(res) => {
            if let Ok(json) = res.json::<serde_json::Value>().await {
                json["success"].as_bool().unwrap_or(false)
            } else {
                false
            }
        }
        Err(_) => false,
    }
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

fn format_timing_info(timing: &RequestTiming, total_request_time: Duration) -> Value {
    json!({
        "total_request": format!("{:?}", total_request_time),
        "total_search_function": format!("{:?}", timing.total_search_function.unwrap_or_default()),
        "lemmatisation": format!("{:?}", timing.lemmatisation.unwrap_or_default()),
        "initial_database_query": format!("{:?}", timing.initial_database_query.unwrap_or_default()),
        "tf_idf_calculation": format!("{:?}", timing.tf_idf_calculation.unwrap_or_default()),
        "link_fetching": format!("{:?}", timing.link_fetching.unwrap_or_default()),
        "results_formatting": format!("{:?}", timing.results_formatting.unwrap_or_default()),
        "other_operations": format!("{:?}", total_request_time - timing.total_search_function.unwrap_or_default()),
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
