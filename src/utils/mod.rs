//! Utility modules ported from reference crawler `pkg/utils` and `pkg/similarity`.

pub mod dsl;
pub mod extensions;
pub mod filters;
pub mod formfill;
pub mod knownfiles;
pub mod knowledgebase;
pub mod queue;
pub mod regex;
pub mod similarity;
pub mod techdetect;

pub use extensions::ExtensionValidator;
pub use filters::{PathTrie, SimpleFilter, SimilarityIndex};
pub use queue::{Item as QueueItem, Queue};
pub use regex::{
    extract_body_endpoints, extract_relative_endpoints, parse_link_tag, parse_refresh_tag,
    parse_srcset_tag, web_user_agent,
};
