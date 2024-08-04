use reqwest::Client;

pub async fn validate_turnstile_token(client: &Client, token: &str) -> bool {
    let secret_key = std::env::var("CLOUDFLARE_TURNSTILE_SECRET_KEY")
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
