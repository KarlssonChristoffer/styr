//! Agent keys: how an agent outside Hub gets into `/mcp`. Making one (a `hub_` token shown once,
//! stored only as its SHA-256 with a name, access and expiry, 30 days by default), listing and
//! revoking them, and finding the caller a token belongs to (noting when it was last used).

use super::*;

#[derive(serde::Serialize, serde::Deserialize)]
pub(super) struct Stored {
    #[serde(flatten)]
    key: AgentKey,
    hash: String,
}

pub(super) async fn stored(hub: &Hub) -> Result<Vec<Stored>> {
    match hub.store.get_value(KEYS).await? {
        Some(json) => Ok(serde_json::from_str(&json)?),
        None => Ok(Vec::new()),
    }
}

pub(super) async fn save(hub: &Hub, keys: &[Stored]) -> Result<()> {
    hub.store.set_value(KEYS, &serde_json::to_string(keys)?).await
}

pub(super) fn hash(token: &str) -> String {
    Sha256::digest(token.as_bytes()).iter().map(|b| format!("{b:02x}")).collect()
}

/// The keys that still work, newest first.
pub async fn keys(hub: &Hub) -> Result<Vec<AgentKey>> {
    let now = now_ms();
    let mut keys: Vec<AgentKey> = stored(hub).await?.into_iter().map(|s| s.key).filter(|k| k.expires_at > now).collect();
    keys.sort_by_key(|k| std::cmp::Reverse(k.created_at));
    Ok(keys)
}

pub async fn create_key(hub: &Hub, new: NewAgentKey) -> Result<CreatedAgentKey> {
    let name = new.name.trim();
    if name.is_empty() {
        bail!("give the key a name, e.g. the agent that will use it");
    }
    let days = new.days.unwrap_or(DAYS).clamp(1, 365);
    let now = now_ms();
    let token = format!("hub_{}", uuid::Uuid::new_v4().simple());
    let key = AgentKey {
        id: uuid::Uuid::now_v7().to_string(),
        name: name.to_owned(),
        access: new.access,
        created_at: now,
        expires_at: now + i64::from(days) * 86_400_000,
        last_used_at: None,
    };
    let mut all = stored(hub).await?;
    all.retain(|s| s.key.expires_at > now);
    all.push(Stored { key: key.clone(), hash: hash(&token) });
    save(hub, &all).await?;
    let url = crate::auth::tools_endpoint(&hub.config, "").0.trim_end_matches('/').to_owned();
    let command = crate::connections::mcp_add_command(&url, &token);
    Ok(CreatedAgentKey { key, token, url, command })
}

pub async fn revoke_key(hub: &Hub, id: &str) -> Result<()> {
    let mut all = stored(hub).await?;
    let before = all.len();
    all.retain(|s| s.key.id != id);
    if all.len() == before {
        bail!("no agent key {id}");
    }
    save(hub, &all).await
}

/// The agent a token belongs to, if it is a key that still works. Noted as used.
pub async fn agent(hub: &Hub, token: &str) -> Result<Option<Caller>> {
    let wanted = hash(token.trim());
    let now = now_ms();
    let mut all = stored(hub).await?;
    let Some(found) = all.iter_mut().find(|s| s.hash == wanted && s.key.expires_at > now) else {
        return Ok(None);
    };
    let caller = Caller::Agent { name: found.key.name.clone(), access: found.key.access };
    // Once a minute is often enough to say when it was last used.
    if found.key.last_used_at.is_none_or(|t| now - t > 60_000) {
        found.key.last_used_at = Some(now);
        save(hub, &all).await?;
    }
    Ok(Some(caller))
}
