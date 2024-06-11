use std::process::Command;
use std::str::from_utf8;
use std::thread;
use std::time::Duration;

use crate::db::util::url_exists;
use crate::prelude::*;
use crate::scrape::util::{get_base_url, get_regions, look_between, strip_html, truncate_string};
use crate::service::var_service::is_source_enabled;
use chrono::Local;
use sqlx::SqlitePool;

pub async fn scrape_antiwar(
    pool: &SqlitePool,
    docker_volume: &str,
    media: &mut Vec<(String, String, String, Vec<String>)>,
) -> Result<()> {
    let antiwar_enabled: bool = is_source_enabled("ANTIWAR_B").await?;
    if !antiwar_enabled {
        return Ok(());
    }

    media.extend(
        scrape_antiwar_features(pool, docker_volume, "https://www.antiwar.com/latest.php").await?,
    );

    Ok(())
}

#[allow(unused_assignments)]
pub async fn scrape_antiwar_features(
    pool: &SqlitePool,
    docker_volume: &str,
    url: &str,
) -> Result<Vec<(String, String, String, Vec<String>)>> {
    let mut features: Vec<(String, String, String, Vec<String>)> = Vec::new();
    let response = reqwest::get(url).await?;
    if !response.status().is_success() {
        tracing::error!("Non-success response from Antiwar: {}", response.status());
        return Ok(features);
    }

    let mut response: String = response.text().await?;
    let today: String = Local::now().format("%B %d, %Y").to_string();
    let date = match look_between(
        &response,
        "<div align=\"right\">Updated ".to_string(),
        " -".to_string(),
    )
    .await?
    {
        Some(date) => date,
        None => return Ok(features),
    };

    if date != today {
        return Ok(features);
    }

    response = match look_between(
        &response,
        "<tr><td colspan=\"2\"><h1>".to_string(),
        "<tr><td colspan=\"2\"><h1>".to_string(),
    )
    .await?
    {
        Some(response) => response,
        None => return Ok(features),
    };

    let mut url_cache: Vec<String> = Vec::new();
    let delay = Duration::from_secs(10);
    url_cache.push(get_base_url(url).await?);
    let items: Vec<&str> = response
        .split("<td width=\"50%\">")
        .skip(1)
        .collect::<Vec<&str>>();
    for item in items {
        let url: String = match look_between(item, "href=\"".to_string(), "\"".to_string()).await? {
            Some(url) => url,
            None => continue,
        };

        if url_exists(pool, &url).await? {
            continue;
        }

        let title = match look_between(item, ">".to_string(), "<".to_string()).await? {
            Some(title) => strip_html(title).await?,
            None => continue,
        };

        let base_url = get_base_url(&url).await?;
        if url_cache.contains(&base_url) {
            url_cache.clear();
            thread::sleep(delay);
        } else {
            url_cache.push(base_url);
        }
        let mut body: Option<String> = None;
        if url.contains("antiwar.com") {
            let response = reqwest::get(&url).await?;
            if !response.status().is_success() {
                tracing::error!("Non-success response from Antiwar: {}", response.status());
                break;
            }

            let response: String = response.text().await?;
            body = Some(
                match look_between(
                    &response,
                    "<meta property=\"og:description\" content=\"".to_string(),
                    ">".to_string(),
                )
                .await?
                {
                    Some(body) => body,
                    None => continue,
                },
            );
        } else {
            let output = Command::new(format!("{}/p3venv/bin/python", docker_volume))
            .arg("-c")
            .arg(format!(
                "import sys; sys.path.append('{}'); from url_to_body import get_body; print(get_body('{}'))",
                docker_volume, url
            ))
            .output()?;
            if !output.status.success() {
                continue;
            }

            let stdout = from_utf8(&output.stdout)?;
            if stdout.is_empty() {
                continue;
            }

            body = Some(stdout.to_string());
        }
        if let Some(body) = body {
            let body = truncate_string(strip_html(&body).await?).await?;
            let regions = get_regions(&[&title, &body]).await?;
            features.push((url, title, body, regions));
        }
    }

    Ok(features)
}
