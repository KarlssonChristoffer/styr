//! Reading across the other machines: their threads for this core's list, the names and
//! addresses each goes by, the tailnet matched to the machines in Hub, and the machines a phone
//! can follow by itself (`phone_machines`).

use super::*;

impl Remotes {
    /// The other machines a phone can follow by itself: those online with an HTTPS address on the
    /// tailnet, each with a fresh phone code made there unless the phone already has a token for
    /// it (`have`, by machine id). A machine on a hubd from before machine ids is left out; the
    /// phone keeps seeing its threads through this machine.
    pub async fn phone_machines(&self, have: &[String]) -> Vec<crate::model::PhoneMachine> {
        let hosts: Vec<Arc<Remote>> = self.hosts.locked().values().cloned().collect();
        let mut out = Vec::new();
        for remote in hosts {
            let Some(conn) = remote.conn.locked().clone() else { continue };
            let routes: crate::model::MachineRoutes = match conn.get(&self.client, "/api/machine/routes").await {
                Ok(routes) => routes,
                Err(e) => {
                    tracing::debug!("{}: no machine routes: {e:#}", remote.config.name);
                    continue;
                }
            };
            let configured = remote.config.url.clone().filter(|u| u.starts_with("https://"));
            let Some(url) = routes.url.or(configured) else { continue };
            let code = if have.contains(&routes.id) {
                None
            } else {
                match conn.post_json::<crate::model::PairCode>(&self.client, "/api/pair/phone-code", &serde_json::json!({})).await {
                    Ok(code) => Some(code.code),
                    Err(e) => {
                        tracing::warn!("{}: no phone code: {e:#}", remote.config.name);
                        continue;
                    }
                }
            };
            out.push(crate::model::PhoneMachine {
                id: routes.id,
                name: remote.config.name.clone(),
                url: url.trim_end_matches('/').to_owned(),
                code,
            });
        }
        out
    }

    /// Threads on every other host, with `host` set and `project`, `display` filled for an older hubd.
    pub fn threads(&self) -> Vec<Thread> {
        let hosts: Vec<Arc<Remote>> = self.hosts.locked().values().cloned().collect();
        hosts.iter().flat_map(|r| r.threads.locked().values().cloned().map(crate::thread_display::filled).collect::<Vec<_>>()).collect()
    }

    /// Each machine with the names and addresses it goes by: its name, its SSH target and what
    /// `~/.ssh/config` says that is, and its URL's host. For telling a machine found on the tailnet
    /// from one already added.
    pub fn identities(&self, ssh: &[crate::model::SshHost]) -> Vec<(String, Vec<String>)> {
        let hosts = self.hosts.locked();
        hosts
            .values()
            .map(|r| {
                let mut ids = vec![r.config.name.clone()];
                if let Some(target) = &r.config.ssh {
                    let target = target.rsplit('@').next().unwrap_or(target).to_owned();
                    for h in ssh.iter().filter(|h| h.alias == target || h.aliases.contains(&target)) {
                        ids.extend(h.hostname.clone());
                        ids.extend(h.aliases.iter().cloned());
                        ids.push(h.alias.clone());
                    }
                    ids.push(target);
                }
                if let Some(url) = &r.config.url
                    && let Ok(url) = url.parse::<Uri>()
                    && let Some(host) = url.host()
                {
                    ids.push(host.to_owned());
                }
                (r.config.name.clone(), ids)
            })
            .collect()
    }

    /// The tailnet as this machine sees it (`tailnet::scan`), each machine on it matched to the
    /// one it is in Hub by its names here and in `~/.ssh/config`.
    pub async fn tailnet(&self, store: &crate::store::Store) -> crate::model::Tailnet {
        let ssh = tokio::task::spawn_blocking(|| crate::prefs::ssh().hosts).await.unwrap_or_default();
        let hosts = self.identities(&ssh);
        crate::tailnet::scan(store, &ssh, &hosts).await
    }
}
