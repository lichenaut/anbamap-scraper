use std::{collections::HashMap, error::Error};
use reqwest::Client;
use serde_json::Value;
use super::super::scraper_util::get_iso_from_name;

pub async fn get_largest_billionaires_map(client: &Client) -> Result<HashMap<String, Vec<String>>, Box<dyn Error>> {
    let mut billionaires: HashMap<String, Vec<String>> = HashMap::new();
    let url = format!("https://forbes400.onrender.com/api/forbes400/getAllBillionaires");
    let response = client.get(&url).send().await?;
    let json: Value = response.json().await?;
    let data = match json.as_array() {
        Some(data) => data,
        None => return Ok(billionaires),
    };

    for billionaire in data {
        if let Some(final_worth) = billionaire["finalWorth"].as_f64() {
            if final_worth < 9900.0 { continue; }
        }

        let citizenship = match billionaire["countryOfCitizenship"].as_str() {
            Some(citizenship) => {
                match get_iso_from_name(citizenship) {
                    Some(iso) => iso.to_string(),
                    None => {
                        tracing::error!("Failed to get ISO code for country of citizenship: {}", citizenship);
                        continue;
                    },
                }
            },
            None => {
                tracing::error!("Failed to get country of citizenship for billionaire: {:?}", billionaire);
                continue;
            },
        };

        let name = match billionaire["personName"].as_str() {
            Some(name) => name.to_string().replace(" & family", ""),
            None => {
                tracing::error!("Failed to get name for billionaire: {:?}", billionaire);
                continue;
            },
        };

        billionaires.entry(citizenship).or_insert_with(Vec::new).push(name);
    }
    
    Ok(billionaires)
}