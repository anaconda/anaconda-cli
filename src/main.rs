const VERSION: &str = env!("PKG_VERSION");

fn greeting() -> &'static str {
    "Hello, world!"
}

fn main() {
    println!("{} (v{})", greeting(), VERSION);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_greeting() {
        assert_eq!(greeting(), "Hello, world!");
    }
}
