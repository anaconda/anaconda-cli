use crate::auth;
use crate::context::CommandContext;

pub async fn api_fetch(
    ctx: &CommandContext,
    method: &str,
    url: &str,
    query_args: Option<&str>,
    data: Option<&str>,
    json: Option<&str>,
) -> Result<(), Box<dyn std::error::Error>> {
    if auth::get_api_key(&ctx.config)?.is_none() {
        return Err("Not logged in. Run `ana login` first.".into());
    }

    let method_upper = method.to_uppercase();
    let mut request = match method_upper.as_str() {
        "GET" => ctx.client().get(url),
        "POST" => ctx.client().post(url),
        "PUT" => ctx.client().put(url),
        "PATCH" => ctx.client().patch(url),
        "DELETE" => ctx.client().delete(url),
        _ => return Err(format!("Unsupported HTTP method: {}", method).into()),
    };
    request = request.header("X-Ana-Raw-Request", "true");
    if let Some(args) = query_args {
        let pairs: Vec<(&str, &str)> = args
            .split(',')
            .filter_map(|pair| pair.split_once('='))
            .collect();
        request = request.query(&pairs);
    }
    if let Some(body) = data {
        request = request.body(body.to_string());
    }
    if let Some(body) = json {
        let parsed: serde_json::Value =
            serde_json::from_str(body).map_err(|e| format!("Invalid JSON: {}", e))?;
        request = request.json(&parsed);
    }
    let response = request.send().await?;
    let status = response.status();
    let body = response.text().await?;
    println!("{}", status);
    println!("{}", body);
    Ok(())
}
