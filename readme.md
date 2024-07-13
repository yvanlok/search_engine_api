# Search Engine API

This repository contains the API for a fast, efficient, and open-source search engine built in Rust.

## Features

- Calculate TF-IDF at runtime using a database of crawled web pages.
- Return search results ranked by relevance.
- Provide detailed statistics for each result.

## Getting Started

### Prerequisites

- Rust (latest stable version)
- PostgreSQL

### Installation

1. **Clone the repository:**

   ```sh
   git clone https://github.com/yvanlok/search_engine_api.git
   cd search_engine_api
   ```

2. **Install dependencies:**

   ```sh
   cargo build
   ```

3. **Set up the database:**

   ```sh
   psql -U postgres -f schema.sql
   ```

4. **Run the API:**
   ```sh
   cargo run
   ```

### API Endpoints

- **GET /search**
  - Parameters: `query` (string)
  - Description: Returns search results based on the provided query. Results are ranked using a TF-IDF algorithm.

## Related Projects

- [Search Engine Crawler](https://github.com/yvanlok/search_engine_crawler)
- [Search Engine UI](https://github.com/yvanlok/search-engine-ui)

## Contributing

1. Fork the repository.
2. Create your feature branch (`git checkout -b feature/new-feature`).
3. Commit your changes (`git commit -am 'Add new feature'`).
4. Push to the branch (`git push origin feature/new-feature`).
5. Create a new Pull Request.
