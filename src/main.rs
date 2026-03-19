const VERSION: &str = env!("PKG_VERSION");

fn greeting() -> &'static str {
    "Hello, world!"
}

fn main() {
    let args: Vec<String> = std::env::args().collect();

    if args.iter().any(|a| a == "--version" || a == "-V") {
        println!("{}", VERSION);
        return;
    }

    println!("{}", greeting());
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_greeting() {
        assert_eq!(greeting(), "Hello, world!");
    }

    #[test]
    fn test_version_is_set() {
        assert!(!VERSION.is_empty());
    }
}
