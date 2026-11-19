//! Patreon for the server fund (see fund.rs): the bot reads Patreon API v2 with the creator token (Creator's Access Token from patreon.com/portal,
//! in .env, variable name = `token_env`): campaign → its members, sum of `currently_entitled_amount_cents`
//! of active patrons (`patron_status = active_patron`). v2 has no ready-made total field, and Patreon disabled
//! Goals (the Goals resource is always empty) — so we compute it ourselves.

use serde_json::Value;

const API: &str = "https://www.patreon.com/api/oauth2/v2";

#[derive(Debug, Clone, PartialEq)]
pub struct Funding {
    pub cents: u64,
    pub patrons: u32,
    pub currency: String,
    pub url: Option<String>,
}

/// Total and number of active paying members from one /members response page
pub fn sum_page(page: &Value) -> (u64, u32) {
    let mut cents = 0;
    let mut n = 0;
    for m in page["data"].as_array().into_iter().flatten() {
        let a = &m["attributes"];
        if a["patron_status"].as_str() == Some("active_patron") {
            let c = a["currently_entitled_amount_cents"].as_u64().unwrap_or(0);
            if c > 0 {
                cents += c;
                n += 1;
            }
        }
    }
    (cents, n)
}

/// 150_00 USD → "$150", 12_50 EUR → "€12.50", unknown currency → by its code
pub fn money(cents: u64, currency: &str) -> String {
    let amount = if cents % 100 == 0 { format!("{}", cents / 100) } else { format!("{}.{:02}", cents / 100, cents % 100) };
    match currency {
        "USD" => format!("${}", amount),
        "EUR" => format!("€{}", amount),
        "GBP" => format!("£{}", amount),
        other => format!("{} {}", amount, other),
    }
}

/// A bar of 10 cells and a percentage (may exceed 100)
pub fn bar(cents: u64, goal_cents: u64) -> String {
    if goal_cents == 0 {
        return String::new();
    }
    let pct = cents * 100 / goal_cents;
    let filled = (pct.min(100) / 10) as usize;
    format!("{}{} {}%", "▓".repeat(filled), "░".repeat(10 - filled), pct)
}

async fn get(http: &reqwest::Client, token: &str, url: &str) -> Result<Value, String> {
    let r = http
        .get(url)
        .bearer_auth(token)
        .header(reqwest::header::USER_AGENT, "DungeonborneCommunityBot (discord, 1)")
        .send()
        .await
        .map_err(|e| e.to_string())?;
    let status = r.status();
    if !status.is_success() {
        return Err(format!("{} → {}", url.split('?').next().unwrap_or(url), status));
    }
    r.json::<Value>().await.map_err(|e| e.to_string())
}

pub async fn fetch(http: &reqwest::Client, token: &str) -> Result<Funding, String> {
    let camps = get(http, token, &format!("{}/campaigns?fields%5Bcampaign%5D=currency,url", API)).await?;
    let camp = camps["data"].get(0).ok_or("the token has no campaign")?;
    let id = camp["id"].as_str().ok_or("campaign without id")?;
    let currency = camp["attributes"]["currency"].as_str().unwrap_or("USD").to_string();
    let url = camp["attributes"]["url"].as_str().map(str::to_string);
    let mut next = Some(format!(
        "{}/campaigns/{}/members?fields%5Bmember%5D=currently_entitled_amount_cents,patron_status&page%5Bcount%5D=500",
        API, id
    ));
    let (mut cents, mut patrons) = (0u64, 0u32);
    let mut pages = 0;
    while let Some(u) = next.take() {
        let page = get(http, token, &u).await?;
        let (c, n) = sum_page(&page);
        cents += c;
        patrons += n;
        pages += 1;
        if pages < 50 {
            next = page["links"]["next"].as_str().map(str::to_string);
        }
    }
    Ok(Funding { cents, patrons, currency, url })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sums_only_active_paying_members() {
        let page = serde_json::json!({"data": [
            {"attributes": {"patron_status": "active_patron", "currently_entitled_amount_cents": 500}},
            {"attributes": {"patron_status": "active_patron", "currently_entitled_amount_cents": 1000}},
            {"attributes": {"patron_status": "former_patron", "currently_entitled_amount_cents": 2000}},
            {"attributes": {"patron_status": "declined_patron", "currently_entitled_amount_cents": 300}},
            {"attributes": {"patron_status": "active_patron", "currently_entitled_amount_cents": 0}},
            {"attributes": {"patron_status": null, "currently_entitled_amount_cents": 0}}
        ], "links": {}});
        assert_eq!(sum_page(&page), (1500, 2));
        assert_eq!(sum_page(&serde_json::json!({})), (0, 0));
    }

    #[test]
    fn money_and_bar() {
        assert_eq!(money(15000, "USD"), "$150");
        assert_eq!(money(1250, "EUR"), "€12.50");
        assert_eq!(money(999, "PLN"), "9.99 PLN");
        assert_eq!(bar(15000, 30000), "▓▓▓▓▓░░░░░ 50%");
        assert_eq!(bar(45000, 30000), "▓▓▓▓▓▓▓▓▓▓ 150%");
        assert_eq!(bar(0, 30000), "░░░░░░░░░░ 0%");
        assert_eq!(bar(100, 0), "");
    }
}
