CREATE TABLE websites (
    id SERIAL PRIMARY KEY,
    title TEXT NOT NULL,
    description TEXT NOT NULL,
    url TEXT UNIQUE NOT NULL,
    word_count INT NOT NULL,
    CONSTRAINT unique_url UNIQUE (url) 
);

CREATE TABLE keywords (
    id SERIAL PRIMARY KEY,
    word TEXT UNIQUE NOT NULL,
    documents_containing_word BIGINT,
    CONSTRAINT unique_word UNIQUE (word) 
);

CREATE TABLE website_keywords (
    id BIGSERIAL PRIMARY KEY,
    keyword_id INT NOT NULL REFERENCES keywords(id),
    website_id INT NOT NULL REFERENCES websites(id),
    keyword_occurrences INT NOT NULL,
    CONSTRAINT unique_keyword_website UNIQUE (keyword_id, website_id, keyword_occurrences) 
);

CREATE TABLE website_links (
    id SERIAL PRIMARY KEY,
    source_website_id INT NOT NULL REFERENCES websites(id),
    target_website TEXT NOT NULL,
    CONSTRAINT unique_source_target UNIQUE (source_website_id, target_website) 
);

CREATE INDEX idx_keywords_documents ON keywords (documents_containing_word);
CREATE INDEX idx_website_keywords_keyword_id ON website_keywords (keyword_id);
CREATE INDEX idx_website_keywords_website_id ON website_keywords (website_id);
CREATE INDEX idx_website_keywords_occurrences ON website_keywords (keyword_occurrences);
CREATE INDEX idx_website_links_source ON website_links (source_website_id);
CREATE INDEX idx_website_links_target ON website_links (target_website);
CREATE INDEX idx_keywords_word ON keywords (word);
CREATE INDEX idx_website_keywords_keyword_id_website_id ON website_keywords (keyword_id, website_id);
