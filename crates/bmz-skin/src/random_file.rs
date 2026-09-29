/// Select a skin file using OS randomness without modulo bias.
///
/// Empty candidate lists and failure to obtain randomness return an error.
pub fn random_file_index(len: usize) -> anyhow::Result<usize> {
    random_file_index_with(len, || {
        let mut bytes = [0_u8; 8];
        getrandom::getrandom(&mut bytes)
            .map_err(|error| anyhow::anyhow!("skin file randomness: {error}"))?;
        Ok(u64::from_le_bytes(bytes))
    })
}

fn random_file_index_with(
    len: usize,
    mut next: impl FnMut() -> anyhow::Result<u64>,
) -> anyhow::Result<usize> {
    anyhow::ensure!(len > 0, "skin file candidates must not be empty");
    let bound = len as u128;
    let range = 1_u128 << 64;
    let limit = range - range % bound;
    loop {
        let value = u128::from(next()?);
        if value < limit {
            return Ok((value % bound) as usize);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn selection_rejects_tail_and_can_reach_each_candidate() {
        let mut values = [u64::MAX, u64::MAX - 5, u64::MAX - 6].into_iter();
        assert_eq!(random_file_index_with(10, || Ok(values.next().unwrap())).unwrap(), 9);
        assert_eq!(values.next(), None);
        for len in [2, 4, 10, 100] {
            for value in 0..len {
                assert_eq!(random_file_index_with(len, || Ok(value as u64)).unwrap(), value);
            }
        }
        assert_eq!(random_file_index_with(4, || Ok(u64::MAX)).unwrap(), 3);
    }

    #[test]
    fn selection_reports_empty_candidates_and_randomness_failure() {
        assert!(random_file_index_with(0, || panic!("empty candidates")).is_err());
        let error = random_file_index_with(2, || anyhow::bail!("unavailable")).unwrap_err();
        assert_eq!(error.to_string(), "unavailable");
    }
}
