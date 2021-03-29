// Copyright 2021 Google LLC
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     https://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

use anyhow::Result;
use chrono::naive::NaiveDate;
use core::num::NonZeroU32;
use governor::clock::DefaultClock;
use governor::state::direct::NotKeyed;
use governor::state::InMemoryState;
use governor::{Quota, RateLimiter};
use log::error;
use reqwest::header::{self, HeaderMap};
use reqwest::IntoUrl;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use std::time;

#[derive(Debug, Clone, Hash, PartialEq, Deserialize)]
struct PuzzleInfoResponse {
    results: Vec<PuzzleMetadata>,
}

#[derive(Debug, Clone, Hash, PartialEq, Deserialize)]
struct PuzzleMetadata {
    print_date: NaiveDate,
    puzzle_id: u32,
    // other fields don't contain accurate solve data. don't trust them.
}

#[derive(Debug, Clone, Copy, Hash, PartialEq, Deserialize)]
struct PuzzleStatsResponse {
    calcs: RawStats,
    firsts: Option<RawFirsts>,
}

impl PuzzleStatsResponse {
    fn collect_stats(&self) -> Option<SolvedPuzzleStats> {
        let mut stats = SolvedPuzzleStats::default();

        if let Some(firsts) = self.firsts {
            stats.opened = firsts.opened;
            stats.solved = firsts.solved;
            match (firsts.checked, firsts.revealed) {
                (None, None) => (),
                // Skip over puzzles where the check or reveal assists were used
                _ => return None,
            }
        }

        if let Some(true) = self.calcs.solved {
            stats.solve_time = if let Some(solve_time) = self.calcs.seconds_spent_solving {
                solve_time
            } else {
                error!("Response for solved puzzle did not contain solve time");
                return None;
            };
            Some(stats)
        } else {
            None
        }
    }
}

#[derive(Debug, Clone, Copy, Hash, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawStats {
    solved: Option<bool>,
    seconds_spent_solving: Option<u32>,
}

#[derive(Debug, Clone, Copy, Hash, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawFirsts {
    opened: Option<u32>,
    checked: Option<u32>,
    revealed: Option<u32>,
    solved: Option<u32>,
}

#[derive(Debug, Clone, Copy, Hash, PartialEq, Default, Serialize)]
pub struct SolvedPuzzleStats {
    pub solve_time: u32,
    pub opened: Option<u32>,
    pub solved: Option<u32>,
}

/// An HTTP client with a rate-limiting wrapper
#[derive(Debug, Clone)]
pub struct RateLimitedClient {
    client: reqwest::Client,
    governor: Arc<RateLimiter<NotKeyed, InMemoryState, DefaultClock>>,
}

impl RateLimitedClient {
    const API_BASE: &'static str = "https://nyt-games-prd.appspot.com/svc/crosswords";
    const PUZZLE_INFO_ENDPOINT: &'static str =
        "/v3/36569100/puzzles.json?publish_type=daily&date_start={start_date}&date_end={end_date}";
    const PUZZLE_STATS_ENDPOINT: &'static str = "/v6/game/{id}.json";

    /// Construct a new `RateLimitedClient`
    ///
    /// # Arguments
    ///
    /// * `nyt_s` - NYT subscription token extracted from web browser
    /// * `quota` - Outgoing request quota in requests per second
    pub fn new(nyt_s: &str, quota: NonZeroU32) -> Self {
        let mut headers = HeaderMap::new();
        headers.insert("nyt-s", nyt_s.parse().unwrap());
        headers.insert(header::ACCEPT, "application/json".parse().unwrap());
        headers.insert(header::DNT, "1".parse().unwrap());

        let client = reqwest::ClientBuilder::new()
            .user_agent("Scraping personal stats")
            .default_headers(headers)
            .timeout(time::Duration::from_secs(10))
            .build()
            .unwrap();
        let governor = Arc::new(RateLimiter::direct(Quota::per_second(quota)));

        Self { client, governor }
    }

    /// Make a rate-limited GET request
    async fn get<T: IntoUrl + Send>(&self, url: T) -> reqwest::Result<reqwest::Response> {
        self.governor.until_ready().await;
        self.client.get(url).send().await
    }

    fn api_url(endpoint: &str) -> String {
        [Self::API_BASE, endpoint].join("")
    }

    /// Get the crossword puzzle id for each crossword in the provided range. This id is needed to
    /// further query for solve stats.
    ///
    /// Returns a `HashMap` mapping `NaiveDate` dates to `u32` ids.
    pub async fn get_puzzle_ids(
        &self,
        start: NaiveDate,
        end: NaiveDate,
    ) -> Result<HashMap<NaiveDate, u32>> {
        let endpoint = Self::PUZZLE_INFO_ENDPOINT
            .replace("{start_date}", &start.format("%Y-%m-%d").to_string())
            .replace("{end_date}", &end.format("%Y-%m-%d").to_string());
        let url = Self::api_url(&endpoint);
        let response: PuzzleInfoResponse = self.get(&url).await?.json().await?;
        Ok(response
            .results
            .into_iter()
            .map(|metadata| (metadata.print_date, metadata.puzzle_id))
            .collect())
    }

    /// Get solve statistics for the crossword with the given id
    ///
    /// Returns a `Result` containing the statistics. If the provided `Option` is `None`, the puzzle
    /// was either unsolved or solved with aid.
    pub async fn get_solve_stats(&self, puzzle_id: u32) -> Result<Option<SolvedPuzzleStats>> {
        let endpoint = Self::PUZZLE_STATS_ENDPOINT.replace("{id}", &puzzle_id.to_string());
        let url = Self::api_url(&endpoint);
        let response: PuzzleStatsResponse = self.get(&url).await?.json().await?;
        Ok(response.collect_stats())
    }
}
