//! Port of the reference crawler `pkg/engine` — engine selection and dispatch
//! (standard / headless / hybrid).

/// Pure HTML/JS parsing utilities; the only engine module available on wasm32.
pub mod parser;

#[cfg(not(target_arch = "wasm32"))]
pub mod common;
#[cfg(not(target_arch = "wasm32"))]
pub mod headless;
#[cfg(not(target_arch = "wasm32"))]
pub mod hybrid;
#[cfg(not(target_arch = "wasm32"))]
pub mod standard;

#[cfg(not(target_arch = "wasm32"))]
mod dispatch {
    use std::sync::Arc;

    use crate::control::CrawlControl;
    use crate::output::StandardWriter;
    use crate::types::options::Options;

    use super::common::Crawler;

    /// Run the crawl for all seed URLs choosing the engine from options
    /// (reference crawler `engine.New` + `Runner` input loop with `-p` parallelism).
    /// Seeds are processed with a sliding-window of `parallelism` — a new
    /// input starts as soon as another finishes (sizedwaitgroup semantics).
    pub async fn execute(
        options: Arc<Options>,
        writer: Arc<StandardWriter>,
        control: Arc<CrawlControl>,
        seeds: Vec<String>,
        in_flight: crate::runner::InFlightUrls,
    ) -> Vec<Arc<Crawler>> {
        let mut results = Vec::new();
        let parallelism = options.parallelism.max(1);
        let mut pending: Vec<&String> = seeds.iter().collect();
        let mut active = tokio::task::JoinSet::new();

        // Refill the window as seeds complete (sizedwaitgroup semantics).
        loop {
            while active.len() < parallelism {
                let Some(seed) = pending.pop() else { break };
                let options = Arc::clone(&options);
                let writer = Arc::clone(&writer);
                let control = Arc::clone(&control);
                let seed = seed.clone();
                in_flight.lock().unwrap().insert(seed.clone());
                active.spawn(async move {
                    let crawler = if !options.chrome_ws_url.is_empty() || options.headless {
                        // -cwu alone forces the pure headless engine.
                        super::headless::crawl_headless(Arc::clone(&options), writer, control, &seed).await
                    } else if options.headless_hybrid {
                        super::hybrid::crawl_hybrid(Arc::clone(&options), writer, control, &seed).await
                    } else {
                        super::standard::crawl_standard(Arc::clone(&options), writer, control, &seed).await
                    };
                    (seed, crawler)
                });
            }
            if active.is_empty() && pending.is_empty() {
                break;
            }
            if let Some(join) = active.join_next().await {
                if let Ok((seed, Ok(crawler))) = join {
                    in_flight.lock().unwrap().remove(&seed);
                    results.push(crawler);
                }
            }
        }
        results
    }
}

#[cfg(not(target_arch = "wasm32"))]
pub use dispatch::execute;
