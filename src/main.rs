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
use std::time::Instant;
use tower_http::cors::CorsLayer;
use reqwest::Client;
use std::sync::Arc;
use tokio::sync::Mutex;
use std::net::SocketAddr;

mod lemmatise;
mod database;
mod ranking;
mod token_cache;
mod timing;
mod turnstile;
mod result_formatter;

use token_cache::TokenCache;
use timing::RequestTiming;
use turnstile::validate_turnstile_token;
use result_formatter::format_result;

#[tokio::main]
async fn main() {
    // Load environment variables
    dotenv().ok();

    // Set up database connection
    let database_url = std::env::var("DATABASE_URL").expect("DATABASE_URL must be set");
    let pool = PgPool::connect(&database_url).await.expect("Failed to connect to database");
    let website_count = database::count_websites(&pool).await.expect("Failed to count websites");

    println!("Connected to database. Found {} websites.", website_count);

    // Load top domains
    let top_domains = load_top_domains("top-1m.txt").await.expect("Failed to load top domains");
    // println!("Top domains: {:?}", top_domains);
    // Get max results from environment variable
    let max_results: usize = std::env
        ::var("MAX_RESULTS")
        .unwrap_or_else(|_| "100".to_string())
        .parse()
        .expect("MAX_RESULTS must be a valid number");

    // Initialize token cache
    let token_cache = Arc::new(Mutex::new(TokenCache::new()));

    // Set up CORS
    let cors = create_cors_layer();

    // Set up the Axum router
    let app = create_router(pool, website_count, top_domains, max_results, token_cache, cors);

    // Start the server
    let port: u16 = std::env
        ::var("AXUM_PORT")
        .unwrap_or_else(|_| "3000".to_string())
        .parse()
        .expect("AXUM_PORT must be a valid number");

    let listener = tokio::net::TcpListener::bind(format!("0.0.0.0:{}", port)).await.unwrap();
    println!("Listening on: http://{}", listener.local_addr().unwrap());
    axum::serve(listener, app.into_make_service_with_connect_info::<SocketAddr>()).await.unwrap();
}

fn create_cors_layer() -> CorsLayer {
    CorsLayer::new()
        .allow_origin(
            vec![
                "http://localhost:3000".parse::<HeaderValue>().unwrap(),
                "http://localhost:3001".parse::<HeaderValue>().unwrap(),
                "http://search.ylokhmotov.dev".parse::<HeaderValue>().unwrap(),
                "https://search.ylokhmotov.dev".parse::<HeaderValue>().unwrap()
            ]
        )
        .allow_methods(vec![Method::GET])
        .allow_headers(vec![axum::http::header::CONTENT_TYPE])
}

fn create_router(
    pool: PgPool,
    website_count: i64,
    top_domains: HashMap<String, usize>,
    max_results: usize,
    token_cache: Arc<Mutex<TokenCache>>,
    cors: CorsLayer
) -> Router {
    Router::new()
        .route("/", get(search))
        .layer(Extension(pool))
        .layer(Extension(website_count))
        .layer(Extension(top_domains))
        .layer(Extension(max_results))
        .layer(Extension(Client::new()))
        .layer(Extension(token_cache))
        .layer(cors)
        .layer(axum::middleware::map_request(timing_middleware))
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
    Extension(max_results): Extension<usize>,
    Extension(client): Extension<Client>,
    Extension(token_cache): Extension<Arc<Mutex<TokenCache>>>,
    mut timing: Extension<RequestTiming>
) -> Json<Value> {
    let search_start = Instant::now();

    // Extract query parameters
    let (query, include_links, num_results) = extract_query_params(&params, max_results);

    // Validate Turnstile token
    let turnstile_start = Instant::now();
    let turnstile_token = params.get("token").expect("Missing Turnstile token");
    let ip = addr.ip().to_string();
    if !validate_token(&client, turnstile_token, &ip, &token_cache).await {
        return Json(json!({ "error": "Invalid Turnstile token" }));
    }
    timing.turnstile_validation = Some(turnstile_start.elapsed());

    // Perform search
    let search_result = perform_search(
        &query,
        &pool,
        website_count,
        &top_domains,
        include_links,
        num_results,
        &mut timing
    ).await;

    timing.total_search_function = Some(search_start.elapsed());

    let total_request_time = timing.start.unwrap().elapsed();

    // Create the response JSON directly
    Json(
        json!({
        "query": query,
        "lemmatised_keywords": [], // Update this if you want to include lemmatized keywords
        "matching_webpages": search_result.len(),
        "time_taken": timing::format_timing_info(&timing, total_request_time),
        "website_count": website_count,
        "results": search_result.iter().map(|(score, webpage)| 
            format_result(score, webpage, &top_domains, include_links)).collect::<Vec<_>>(),
    })
    )
}

// Helper functions (implement these in separate modules)

fn extract_query_params(
    params: &HashMap<String, String>,
    max_results: usize
) -> (String, bool, usize) {
    let query = params.get("q").expect("Missing query parameter").to_string();
    let include_links = params
        .get("links")
        .map(|v| v == "true")
        .unwrap_or(false);
    let num_results = params
        .get("results")
        .and_then(|v| v.parse().ok())
        .unwrap_or(100)
        .min(max_results);
    (query, include_links, num_results)
}

async fn validate_token(
    client: &Client,
    token: &str,
    ip: &str,
    token_cache: &Arc<Mutex<TokenCache>>
) -> bool {
    let mut cache = token_cache.lock().await;
    if !cache.is_valid(token, ip) {
        if !validate_turnstile_token(client, token).await {
            println!("Token validation failed for IP: {}", ip);
            return false;
        }
        cache.add_token(token.to_string(), ip.to_string());
    }
    cache.clean_old_tokens();
    true
}

async fn perform_search(
    query: &str,
    pool: &PgPool,
    website_count: i64,
    top_domains: &HashMap<String, usize>,
    include_links: bool,
    num_results: usize,
    timing: &mut RequestTiming
) -> Vec<(f32, database::Webpage)> {
    // Lemmatize the query
    let lemmatise_time = Instant::now();
    let keywords = lemmatise::lemmatise_string(query);
    timing.lemmatisation = Some(lemmatise_time.elapsed());

    // Fetch webpages from the database (without links initially)
    let db_time = Instant::now();
    let webpages = match database::fetch_webpages(pool, &keywords, false).await {
        Ok(webpages) => webpages,
        Err(e) => {
            eprintln!("Error fetching webpages: {}", e);
            return vec![];
        }
    };
    timing.initial_database_query = Some(db_time.elapsed());

    // Calculate TF-IDF scores and rank webpages
    let tfidf_time = Instant::now();
    let mut ranked_webpages = ranking::get_tf_idf_scores(website_count, &keywords, &webpages).await;

    // Sort ranked_webpages by score in descending order
    ranked_webpages.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap());

    // Count webpages with score >= 1.0
    let high_score_count = ranked_webpages
        .iter()
        .take_while(|(score, _)| *score >= 1.0)
        .count();

    // Sort webpages with score >= 1.0 by score first, then by website rank
    if high_score_count > 0 {
        ranked_webpages[..high_score_count].sort_by(|a, b| {
            b.0
                .partial_cmp(&a.0)
                .unwrap()
                .then_with(|| {
                    let domain_a = result_formatter::extract_domain_from_string(&a.1.url);
                    let domain_b = result_formatter::extract_domain_from_string(&b.1.url);
                    let rank_a = domain_a
                        .and_then(|d| top_domains.get(&d).cloned())
                        .unwrap_or(usize::MAX);
                    let rank_b = domain_b
                        .and_then(|d| top_domains.get(&d).cloned())
                        .unwrap_or(usize::MAX);
                    rank_a.cmp(&rank_b)
                })
        });
    }

    // Determine the number of results to return
    let results_to_return = high_score_count.min(num_results);

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

        let links = database::fetch_links_for_ids(pool, &webpage_ids).await.unwrap_or_default();

        for (_score, webpage) in &mut ranked_webpages {
            if let Some((links_to_count, links_from)) = links.get(&webpage.id) {
                webpage.links_to_count = Some(*links_to_count);
                webpage.links_from = Some(links_from.clone());
            }
        }
        timing.link_fetching = Some(link_time.elapsed());
    }

    ranked_webpages
}

async fn load_top_domains(filename: &str) -> io::Result<HashMap<String, usize>> {
    let file = File::open(filename).await?;
    let reader = io::BufReader::new(file);
    let mut top_domains = HashMap::new();

    let mut lines = reader.lines();
    let mut rank = 1;
    while let Some(line) = lines.next_line().await? {
        top_domains.insert(line, rank);
        rank += 1;
    }
    Ok(top_domains)
}
