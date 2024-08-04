use std::collections::HashMap;
use std::time::{ SystemTime, UNIX_EPOCH };

pub struct TokenCache {
    tokens: HashMap<String, (u64, String)>,
}

impl TokenCache {
    pub fn new() -> Self {
        TokenCache {
            tokens: HashMap::new(),
        }
    }

    pub fn is_valid(&mut self, token: &str, ip: &str) -> bool {
        let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs();
        if let Some((timestamp, stored_ip)) = self.tokens.get(token) {
            if now - timestamp <= 120 && stored_ip == ip {
                return true;
            }
        }
        false
    }

    pub fn add_token(&mut self, token: String, ip: String) {
        let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs();
        self.tokens.insert(token, (now, ip));
    }

    pub fn clean_old_tokens(&mut self) {
        let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs();
        self.tokens.retain(|_, &mut (timestamp, _)| now - timestamp <= 120);
    }
}
