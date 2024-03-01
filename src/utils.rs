use std::collections::HashSet;

use url::Url;

pub fn dedup_urls(input: impl IntoIterator<Item = Url>) -> impl Iterator<Item = Url> {
    let mut dedup = HashSet::new();

    input.into_iter().filter(move |url| {
        if !dedup.contains(url) {
            dedup.insert(url.clone());
            true
        }
        else {
            false
        }
    })
}
