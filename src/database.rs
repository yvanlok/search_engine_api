use sqlx::{ PgPool, Row, postgres::PgRow };
use std::collections::HashMap;
use std::error::Error;

/// Represents a webpage with its associated metadata and keyword information
#[derive(Debug, Clone)]
pub struct Webpage {
    pub id: i32,
    pub title: String,
    pub url: String,
    pub description: String,
    pub word_count: i32,
    pub keywords: HashMap<Keyword, i32>,
    pub links_to_count: Option<usize>,
    pub links_from: Option<HashMap<String, i32>>,
}

/// Represents a keyword with its associated metadata
#[derive(Debug, Eq, Hash, PartialEq, Clone)]
pub struct Keyword {
    pub id: i32,
    pub word: String,
    pub documents_containing_word: i64,
}

pub async fn fetch_webpages(
    pool: &PgPool,
    keywords: &[String],
    include_links: bool
) -> Result<Vec<Webpage>, Box<dyn Error>> {
    // Return early if no keywords are provided
    if keywords.is_empty() {
        return Ok(vec![]);
    }

    // Prepare the SQL query to fetch all necessary data in a single round trip
    let query =
        r#"
        SELECT 
            w.id as website_id, 
            w.title, 
            w.url, 
            w.description, 
            w.word_count, 
            k.word, 
            k.documents_containing_word,
            k.id as keyword_id, 
            wk.keyword_occurrences
        FROM 
            websites w
        JOIN 
            website_keywords wk ON w.id = wk.website_id
        JOIN 
            keywords k ON wk.keyword_id = k.id
        WHERE 
            k.word = ANY($1::text[])
    "#;

    // Execute the query and fetch all rows
    let rows: Vec<PgRow> = sqlx::query(query).bind(keywords).fetch_all(pool).await?;

    // Use a HashMap to efficiently build Webpage structs
    let mut webpages_map: HashMap<i32, Webpage> = HashMap::new();

    // Process each row and populate the webpages_map
    for row in rows {
        let webpage_id: i32 = row.get("website_id");
        let keyword_occurrences: i32 = row.get("keyword_occurrences");

        let keyword = Keyword {
            id: row.get("keyword_id"),
            word: row.get("word"),
            documents_containing_word: row.get("documents_containing_word"),
        };

        // Use entry API for efficient map operations
        let webpage_struct = webpages_map.entry(webpage_id).or_insert_with(|| Webpage {
            id: webpage_id,
            title: row.get("title"),
            url: row.get("url"),
            description: row.get("description"),
            word_count: row.get("word_count"),
            keywords: HashMap::new(),
            links_to_count: None,
            links_from: None,
        });

        webpage_struct.keywords.insert(keyword, keyword_occurrences);
    }

    // Fetch and add link information if requested
    if include_links {
        let links = fetch_links(pool).await?;
        for (webpage_id, links_to_count, links_from) in links {
            if let Some(webpage) = webpages_map.get_mut(&webpage_id) {
                webpage.links_to_count = Some(links_to_count);
                webpage.links_from = Some(links_from);
            }
        }
    }

    Ok(webpages_map.into_values().collect())
}

pub async fn fetch_links(
    pool: &PgPool
) -> Result<Vec<(i32, usize, HashMap<String, i32>)>, Box<dyn Error>> {
    // Prepare the SQL query to fetch link information
    let query =
        r#"
        SELECT 
            w.id as website_id,
            COUNT(DISTINCT wt.target_website) as links_to_count,
            ws.url as source_website
        FROM 
            websites w
        LEFT JOIN 
            website_links wt ON w.id = wt.source_website_id
        LEFT JOIN 
            website_links wl ON wl.target_website = w.url
        LEFT JOIN 
            websites ws ON ws.id = wl.source_website_id
        GROUP BY w.id, ws.url
    "#;

    // Execute the query and fetch all rows
    let rows: Vec<PgRow> = sqlx::query(query).fetch_all(pool).await?;

    // Use a HashMap to efficiently build link information
    let mut links_map: HashMap<i32, (usize, HashMap<String, i32>)> = HashMap::new();

    // Process each row and populate the links_map
    for row in rows {
        let webpage_id: i32 = row.get("website_id");
        let links_to_count: i64 = row.get("links_to_count");
        let source_website: Option<String> = row.get("source_website");

        let entry = links_map
            .entry(webpage_id)
            .or_insert((links_to_count as usize, HashMap::new()));
        if let Some(source) = source_website {
            *entry.1.entry(source).or_insert(0) += 1;
        }
    }

    // Convert the HashMap into the desired Vec format
    Ok(
        links_map
            .into_iter()
            .map(|(id, (to_count, from))| (id, to_count, from))
            .collect()
    )
}

pub async fn fetch_links_for_ids(
    pool: &PgPool,
    webpage_ids: &[i32]
) -> Result<HashMap<i32, (usize, HashMap<String, i32>)>, Box<dyn Error>> {
    // Prepare the SQL query to fetch link information for specific webpage IDs
    let query =
        r#"
        SELECT 
            w.id as website_id,
            COUNT(DISTINCT wt.target_website) as links_to_count,
            ws.url as source_website
        FROM 
            websites w
        LEFT JOIN 
            website_links wt ON w.id = wt.source_website_id
        LEFT JOIN 
            website_links wl ON wl.target_website = w.url
        LEFT JOIN 
            websites ws ON ws.id = wl.source_website_id
        WHERE
            w.id = ANY($1::int[])
        GROUP BY w.id, ws.url
    "#;

    // Execute the query and fetch all rows
    let rows: Vec<PgRow> = sqlx::query(query).bind(webpage_ids).fetch_all(pool).await?;

    // Use a HashMap to efficiently build link information
    let mut links_map: HashMap<i32, (usize, HashMap<String, i32>)> = HashMap::new();

    // Process each row and populate the links_map
    for row in rows {
        let webpage_id: i32 = row.get("website_id");
        let links_to_count: i64 = row.get("links_to_count");
        let source_website: Option<String> = row.get("source_website");

        let entry = links_map
            .entry(webpage_id)
            .or_insert((links_to_count as usize, HashMap::new()));
        if let Some(source) = source_website {
            *entry.1.entry(source).or_insert(0) += 1;
        }
    }

    Ok(links_map)
}

pub async fn count_websites(pool: &PgPool) -> Result<i64, Box<dyn Error>> {
    // Execute a simple COUNT query to get the total number of websites
    let query = "SELECT COUNT(*) FROM websites";
    let count: i64 = sqlx::query_scalar(query).fetch_one(pool).await?;
    Ok(count)
}
