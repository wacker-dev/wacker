use rand::{distributions::Alphanumeric, thread_rng, Rng};

pub fn generate_random_string(length: usize) -> String {
    thread_rng()
        .sample_iter(&Alphanumeric)
        .take(length)
        .map(char::from)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generate_random_string() {
        assert_eq!(generate_random_string(5).len(), 5);
    }
}
