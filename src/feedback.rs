use crate::context::CommandContext;

const FEEDBACK_BASE_URL: &str = "https://docs.google.com/forms/d/e/1FAIpQLSeGd9p7pQSHvjIc6RNShjTQCGmM-5_3xkPNpNfYk102-HZB8Q/viewform";

/// Feedback type for the feedback form
#[derive(Clone, Copy)]
pub enum FeedbackType {
    Bug,
    Feature,
}

pub fn parse_feedback_type(bug: bool, feature: bool) -> Option<FeedbackType> {
    if bug {
        Some(FeedbackType::Bug)
    } else if feature {
        Some(FeedbackType::Feature)
    } else {
        None
    }
}

pub fn open_feedback(
    _ctx: &mut CommandContext,
    feedback_type: Option<FeedbackType>,
    description: Option<String>,
) {
    let mut params = vec![("usp", "pp_url".to_string())];

    if let Some(ft) = feedback_type {
        let type_value = match ft {
            FeedbackType::Bug => "Bug",
            FeedbackType::Feature => "Feature / Enhancement request",
        };
        params.push(("entry.1875536722", type_value.to_string()));
    }

    if let Some(desc) = description {
        params.push(("entry.949440629", desc));
    }

    let query_string: String = params
        .iter()
        .map(|(k, v)| format!("{}={}", k, urlencoding::encode(v)))
        .collect::<Vec<_>>()
        .join("&");

    let url = format!("{}?{}", FEEDBACK_BASE_URL, query_string);

    println!("Opening feedback form: {}", url);
    if let Err(e) = webbrowser::open(&url) {
        eprintln!("Failed to open browser: {}", e);
    }
}
