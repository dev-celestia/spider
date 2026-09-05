//! Port of the reference crawler `pkg/engine` — engine selection and dispatch
//! (standard / headless / hybrid).

pub mod common;
pub mod headless;
pub mod hybrid;
pub mod parser;
pub mod standard;

use std::sync::Arc;
use std::sync::atomic::AtomicBool;

use crate::output::StandardWriter;
use crate::types::options::Options;

use common::Crawler;

/// Run the crawl for all seed URLs choosing the engine from options
/// (reference crawler `engine.New` + `Runner` input loop with `-p` parallelism).
pub async fn execute(
    options: Arc<Options>,
    writer: Arc<StandardWriter>,
    cancel: Arc<AtomicBool>,
    seeds: Vec<String>,
) -> Vec<Arc<Crawler>> {
    let mut results = Vec::new();
    let parallelism = options.parallelism.max(1);

    // Process inputs with bounded parallelism (celestia parallelism).
    for chunk in seeds.chunks(parallelism) {
        let mut handles = Vec::new();
        for seed in chunk {
            let options = Arc::clone(&options);
            let writer = Arc::clone(&writer);
            let cancel = Arc::clone(&cancel);
            let seed = seed.clone();
            handles.push(tokio::spawn(async move {
                if options.headless {
                    headless::crawl_headless(Arc::clone(&options), writer, cancel, &seed).await
                } else if options.headless_hybrid {
                    hybrid::crawl_hybrid(Arc::clone(&options), writer, cancel, &seed).await
                } else {
                    standard::crawl_standard(Arc::clone(&options), writer, cancel, &seed).await
                }
            }));
        }
        for handle in handles {
            if let Ok(Ok(crawler)) = handle.await {
                results.push(crawler);
            }
        }
    }
    results
}
