/// The EX score rank from AAA (0) to F (7). Charts with no scored notes have no rank.
pub fn score_rank_index(ex_score: u32, total_notes: u32) -> Option<usize> {
    let max_score = total_notes.saturating_mul(2);
    if max_score == 0 {
        return None;
    }

    // E starts at 2/9 of the maximum, and AAA starts at 8/9.
    let ninths = u64::from(ex_score.min(max_score)) * 9 / u64::from(max_score);
    Some(8 - ninths.clamp(1, 8) as usize)
}
