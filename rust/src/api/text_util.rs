use rayon::prelude::*;
use std::collections::HashMap;

/// Count the number of duplicate characters in a string
pub async fn count_duplicate_characters(input: String) -> usize {
    let char_counts = input
        .par_chars() // Iterate characters in parallel using rayon
        .fold(HashMap::new, |mut acc, c| {
            *acc.entry(c).or_insert(0) += 1;
            acc
        })
        .reduce(HashMap::new, |mut acc, map| {
            for (k, v) in map {
                *acc.entry(k).or_insert(0) += v;
            }
            acc
        });

    char_counts.values().filter(|&&count| count > 1).sum()
}

