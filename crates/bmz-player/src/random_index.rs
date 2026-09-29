/// Select an index without clock-resolution or modulo bias.
pub(crate) fn random_index(len: usize) -> Option<usize> {
    random_index_with(len, || {
        let mut bytes = [0_u8; 8];
        match getrandom::getrandom(&mut bytes) {
            Ok(()) => u64::from_le_bytes(bytes),
            Err(error) => {
                use std::hash::BuildHasher;

                tracing::warn!(%error, "failed to obtain OS randomness for random selection");
                std::collections::hash_map::RandomState::new().hash_one(len)
            }
        }
    })
}

fn random_index_with(len: usize, mut next: impl FnMut() -> u64) -> Option<usize> {
    if len == 0 {
        return None;
    }
    loop {
        if let Some(index) = uniform_random_index(next(), len) {
            return Some(index);
        }
    }
}

pub(crate) fn uniform_random_index(value: u64, len: usize) -> Option<usize> {
    if len == 0 {
        return None;
    }
    let range = u64::MAX as u128 + 1;
    let bound = len as u128;
    let limit = range - range % bound;
    ((value as u128) < limit).then_some((value as u128 % bound) as usize)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_selection_does_not_request_randomness() {
        assert_eq!(random_index_with(0, || panic!("empty selection")), None);
    }

    #[test]
    fn selection_retries_rejected_tail() {
        let mut values = [u64::MAX, u64::MAX - 5, 9].into_iter();
        assert_eq!(random_index_with(10, || values.next().unwrap()), Some(9));
        assert_eq!(values.next(), None);
        assert_eq!(uniform_random_index(u64::MAX - 6, 10), Some(9));
    }

    #[test]
    fn power_of_two_accepts_entire_range() {
        assert_eq!(uniform_random_index(u64::MAX, 4), Some(3));
        assert_eq!(uniform_random_index(u64::MAX, 1), Some(0));
    }
}
