//! The Plugins page: GitHub repositories made for agents (MCP servers, skills, tools), what is
//! rising, what is new, search, and one repository's own page. Everything is read through the
//! GitHub CLI the machine is logged in with, so no keys are needed.
//!
//! GitHub has no trending list, so "rising" is counted from the stars themselves: the stargazers
//! list, newest last, carries when each star was given. Hub reads its last pages and counts the
//! stars of the last 7 days; when there are more than it reads, it projects from the rate.
//!
//! This is step 1: look and read. Installing, Customs' review of a repository and switching one
//! on in a thread come later (see /mnt/project-files/plugins/pluginsida-forslag.md).

use crate::locked::{LockExt, ResultExt};
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::{LazyLock, Mutex};
use std::time::{Duration, Instant};

use anyhow::{Result, anyhow, bail};
use futures::stream::{self, StreamExt};
use serde_json::Value;

use crate::ctx::Ctx;
use crate::model::{CustomsItem, PluginCheck, PluginDetail, PluginDiscover, PluginRepo, PluginSearch, PluginWorker, WorkerReady, now_ms};
use crate::proc::{GhError, gh_within};
use crate::triage::parse_time;
use crate::github_name::{CUSTOMS_KIND, check_name};
// `mcp/talk.rs` still names it here; moving that caller waits for the typed-events work on `mcp/`.
pub use crate::github_name::repo_from_link;

/// GitHub topics that mark a repository as made for agents: MCP's, the engines' own
/// (`connections::MADE_FOR`) and the general skills one beside the engines' skills topics. One
/// search each, in this order (ties in stars keep it).
pub static TOPICS: LazyLock<Vec<&'static str>> = LazyLock::new(|| {
    let mut topics = vec!["mcp-server", "model-context-protocol"];
    topics.extend(crate::connections::MADE_FOR.topics);
    let at = topics.iter().rposition(|t| t.ends_with("-skills")).map_or(topics.len(), |i| i + 1);
    topics.insert(at, "agent-skills");
    topics
});

const DAY: i64 = 86_400_000;
/// How long the Discover lists count as new; older ones are shown while Hub counts again.
const DISCOVER_FOR: i64 = 60 * 60 * 1000;
const DISCOVER_FILE: &str = "plugins-discover.json";
const DETAIL_FOR: Duration = Duration::from_secs(30 * 60);
/// GitHub stops listing stargazers after 400 pages.
const MAX_STAR_PAGE: u64 = 400;
/// The most a README may be; longer ones are cut.
const MAX_README: usize = 80 * 1024;
const MAX_PICTURE: usize = 1_500_000;

/// The Discover lists as last counted, and whether a count is running now. The lists are also kept
/// on disk (`DISCOVER_FILE`), so the page has something to show at once after a restart.
static BOARD: LazyLock<Mutex<Board>> = LazyLock::new(|| Mutex::new(Board::default()));
/// GitHub calls for star pages running at once, over every repository being counted.
static STAR_SLOTS: tokio::sync::Semaphore = tokio::sync::Semaphore::const_new(16);
static DETAILS: LazyLock<Mutex<HashMap<String, (Instant, PluginDetail)>>> = LazyLock::new(|| Mutex::new(HashMap::new()));
static AVATARS: LazyLock<Mutex<HashMap<String, Option<String>>>> = LazyLock::new(|| Mutex::new(HashMap::new()));
static HTTP: LazyLock<reqwest::Client> = LazyLock::new(|| {
    reqwest::Client::builder()
        .user_agent(concat!("hub/", env!("CARGO_PKG_VERSION")))
        .timeout(Duration::from_secs(15))
        .build()
        .unwrap_or_default()
});

fn gh_message(e: GhError) -> String {
    match e {
        GhError::Missing => "Install the GitHub CLI (gh) and run `gh auth login` to find plugins.".into(),
        GhError::Failed(msg) if msg.contains("rate limit") => "GitHub's search limit is reached for the moment; try again in a minute.".into(),
        GhError::Failed(msg) => msg,
    }
}

pub(crate) async fn gh(args: &[&str], secs: u64) -> std::result::Result<String, GhError> {
    gh_within(&std::env::temp_dir(), args, Duration::from_secs(secs)).await
}

pub(crate) async fn gh_json(args: &[&str], secs: u64) -> Result<Value> {
    let out = gh(args, secs).await.map_err(|e| anyhow!(gh_message(e)))?;
    Ok(serde_json::from_str(&out)?)
}

/// One repository as a card shows it: GitHub's facts, its last week, picture and Customs' word.
pub async fn card(ctx: &Ctx<'_>, full: &str) -> Result<PluginRepo> {
    let full = check_name(full)?;
    let v = gh_json(&["api", &format!("repos/{full}")], 15).await?;
    let mut repo = repo_of(&v).ok_or_else(|| anyhow!("GitHub did not answer with a repository"))?;
    add_trend(&mut repo, now_ms(), 5).await;
    dress(ctx, std::slice::from_mut(&mut repo)).await;
    Ok(repo)
}

/// What a repository is, from its topics, name and description: "worker", "mcp", "skill" or "tool".
pub fn kind_of(name: &str, description: &str, topics: &[String]) -> &'static str {
    let name = name.to_lowercase();
    let desc = description.to_lowercase();
    let has = |w: &str| topics.iter().any(|t| t.contains(w));
    if topics.iter().any(|t| t == "hub-worker") {
        "worker"
    } else if has("mcp") || has("model-context-protocol") || name.split(['-', '_', '.']).any(|p| p == "mcp") || desc.contains("mcp server") || desc.contains("model context protocol") {
        "mcp"
    } else if has("skill") || name.contains("skill") || desc.contains("agent skill") || crate::connections::MADE_FOR.skill_phrases.iter().any(|p| desc.contains(p)) {
        "skill"
    } else {
        "tool"
    }
}

/// A repository as GitHub's search and repository answers carry it.
pub fn repo_of(v: &Value) -> Option<PluginRepo> {
    let full = v["full_name"].as_str()?.to_owned();
    let s = |k: &str| v[k].as_str().unwrap_or_default().to_owned();
    let topics: Vec<String> = v["topics"].as_array().map(|a| a.iter().filter_map(|t| t.as_str().map(str::to_owned)).collect()).unwrap_or_default();
    let description = s("description");
    let name = s("name");
    let license = v["license"]["spdx_id"].as_str().filter(|l| !l.is_empty() && *l != "NOASSERTION").map(str::to_owned);
    Some(PluginRepo {
        kind: kind_of(&name, &description, &topics).into(),
        owner: v["owner"]["login"].as_str().unwrap_or_default().to_owned(),
        full_name: full,
        name,
        description,
        url: s("html_url"),
        stars: v["stargazers_count"].as_u64().unwrap_or(0),
        language: v["language"].as_str().map(str::to_owned),
        topics,
        license,
        created_at: v["created_at"].as_str().and_then(parse_time).unwrap_or(0),
        pushed_at: v["pushed_at"].as_str().and_then(parse_time).unwrap_or(0),
        archived: v["archived"].as_bool().unwrap_or(false),
        ..Default::default()
    })
}

/// A picture from GitHub as a data URL, only from GitHub's own picture hosts.
async fn picture(url: &str) -> Option<String> {
    let host = url.strip_prefix("https://")?.split('/').next()?;
    if !matches!(host, "avatars.githubusercontent.com" | "repository-images.githubusercontent.com" | "opengraph.githubassets.com") {
        return None;
    }
    let resp = HTTP.get(url).send().await.ok()?.error_for_status().ok()?;
    let mime = resp.headers().get("content-type")?.to_str().ok()?.split(';').next()?.trim().to_owned();
    if !mime.starts_with("image/") || mime.contains("svg") {
        return None;
    }
    let bytes = resp.bytes().await.ok()?;
    if bytes.len() > MAX_PICTURE {
        return None;
    }
    Some(format!("data:{mime};base64,{}", crate::pages::base64(&bytes)))
}

async fn avatar(owner: &str) -> Option<String> {
    if let Some(hit) = AVATARS.locked().get(owner) {
        return hit.clone();
    }
    let got = picture(&format!("https://avatars.githubusercontent.com/{owner}?s=64")).await;
    AVATARS.locked().insert(owner.to_owned(), got.clone());
    got
}

/// Puts the owner's picture and Customs' word on each repository.
async fn dress(ctx: &Ctx<'_>, repos: &mut [PluginRepo]) {
    let customs = crate::customs::list(ctx).await.unwrap_or_default();
    let owners: Vec<String> = repos.iter().map(|r| r.owner.clone()).collect::<HashSet<_>>().into_iter().collect();
    let pics: HashMap<String, Option<String>> = stream::iter(owners).map(|o| async move { (o.clone(), avatar(&o).await) }).buffer_unordered(12).collect().await;
    for r in repos {
        r.avatar = pics.get(&r.owner).cloned().flatten();
        stamp(&customs, r);
    }
}

fn stamp(customs: &[CustomsItem], r: &mut PluginRepo) {
    let name = r.full_name.to_lowercase();
    if let Some(item) = customs.iter().find(|c| c.kind == CUSTOMS_KIND && c.name.to_lowercase() == name) {
        r.customs = Some(item.status);
        r.customs_reason = item.reason.clone();
    }
}

async fn search_raw(q: &str, sort: Option<&str>, per_page: u32) -> Result<Vec<PluginRepo>> {
    let q = format!("q={q}");
    let per = format!("per_page={per_page}");
    let mut args = vec!["api", "-X", "GET", "search/repositories", "-f", &q, "-f", &per];
    let sort = sort.map(|s| format!("sort={s}"));
    if let Some(s) = &sort {
        args.extend(["-f", s.as_str()]);
    }
    let v = gh_json(&args, 20).await?;
    Ok(v["items"].as_array().map(|a| a.iter().filter_map(repo_of).collect()).unwrap_or_default())
}

fn merge_into(into: &mut Vec<PluginRepo>, seen: &mut HashSet<String>, from: Vec<PluginRepo>) {
    for r in from {
        if !r.archived && seen.insert(r.full_name.to_lowercase()) {
            into.push(r);
        }
    }
}

/// What is rising, what is new and what most use, kept for an hour. `fresh` asks GitHub again.
#[derive(Default)]
struct Board {
    data: Option<PluginDiscover>,
    /// The lists are only New and Most stars so far; Rising is still being counted.
    partial: bool,
    updating: bool,
}

/// The Discover lists, at once: the last ones counted (from memory, else from disk), while Hub
/// counts again in the background when they are over an hour old or `fresh` asks for it. With
/// nothing counted yet it waits for New and Most stars (a few seconds) and leaves Rising
/// counting; `updating` tells the page to ask again.
pub async fn discover(ctx: &Ctx<'_>, fresh: bool) -> PluginDiscover {
    let cached = last_counted(ctx).await;
    let old = cached.as_ref().is_none_or(|d| now_ms() - d.fetched_at > DISCOVER_FOR || d.message.is_some());
    if fresh || old {
        count_again(ctx.store.clone(), ctx.config.clone());
    }
    let mut d = match cached {
        Some(d) => d,
        None => first_lists().await,
    };
    d.updating = BOARD.locked().updating;
    let customs = crate::customs::list(ctx).await.unwrap_or_default();
    for r in d.rising.iter_mut().chain(d.fresh.iter_mut()).chain(d.top.iter_mut()) {
        stamp(&customs, r);
    }
    d
}

/// Like `discover`, but waits (up to two minutes) until Rising is counted: for agents, which ask once.
pub async fn discover_counted(ctx: &Ctx<'_>) -> PluginDiscover {
    let mut d = discover(ctx, false).await;
    let until = Instant::now() + Duration::from_secs(120);
    while d.updating && d.rising.is_empty() && Instant::now() < until {
        tokio::time::sleep(Duration::from_millis(500)).await;
        d = discover(ctx, false).await;
    }
    d
}

async fn last_counted(ctx: &Ctx<'_>) -> Option<PluginDiscover> {
    if let Some(d) = BOARD.locked().data.clone() {
        return Some(d);
    }
    let path = ctx.config.data_dir.join(DISCOVER_FILE);
    let saved: PluginDiscover = serde_json::from_slice(&tokio::fs::read(path).await.ok()?).ok()?;
    let mut board = BOARD.locked();
    Some(board.data.get_or_insert(saved).clone())
}

/// Waits for the count that is running to have something to show.
async fn first_lists() -> PluginDiscover {
    let until = Instant::now() + Duration::from_secs(90);
    loop {
        {
            let board = BOARD.locked();
            if let Some(d) = &board.data {
                return d.clone();
            }
            if !board.updating || Instant::now() > until {
                return PluginDiscover { message: Some("GitHub did not answer; try again in a minute.".into()), fetched_at: now_ms(), ..Default::default() };
            }
        }
        tokio::time::sleep(Duration::from_millis(150)).await;
    }
}

/// Starts counting the lists again in the background, unless a count is running already.
fn count_again(store: crate::store::Store, config: crate::config::Config) {
    {
        let mut board = BOARD.locked();
        if board.updating {
            return;
        }
        board.updating = true;
    }
    tokio::spawn(async move {
        let d = count(&Ctx::new(&store, &config)).await;
        let keep = d.message.is_none();
        {
            let mut board = BOARD.locked();
            // A failed count does not hide lists that worked before.
            if keep || board.data.is_none() || board.partial {
                board.data = Some(d.clone());
            }
            board.partial = false;
            board.updating = false;
        }
        if keep && let Ok(json) = serde_json::to_vec(&d) {
            tokio::fs::write(config.data_dir.join(DISCOVER_FILE), json).await.warn_on_err("write file");
        }
    });
}

/// Counts the lists: New and Most stars first (shown as soon as they are there when nothing was
/// counted before), then Rising from the stars of the last two weeks.
async fn count(ctx: &Ctx<'_>) -> PluginDiscover {
    let now = now_ms();
    let month = crate::activity::date_of(now - 30 * DAY, 0);
    let queries: Vec<(bool, String)> = TOPICS
        .iter()
        .flat_map(|t| [(false, format!("topic:{t} pushed:>{month} stars:>50")), (true, format!("topic:{t} created:>{month} stars:>5"))])
        .collect();
    let answers: Vec<(bool, Result<Vec<PluginRepo>>)> =
        stream::iter(queries).map(|(new, q)| async move { (new, search_raw(&q, Some("stars"), 25).await) }).buffered(6).collect().await;
    let (mut known, mut new, mut seen_k, mut seen_n) = (Vec::new(), Vec::new(), HashSet::new(), HashSet::new());
    let mut message = None;
    for (is_new, got) in answers {
        match got {
            Ok(list) if is_new => merge_into(&mut new, &mut seen_n, list),
            Ok(list) => merge_into(&mut known, &mut seen_k, list),
            Err(e) => message = Some(e.to_string()),
        }
    }
    known.sort_by_key(|x| std::cmp::Reverse(x.stars));
    new.sort_by_key(|x| std::cmp::Reverse(x.stars));
    if known.is_empty() && new.is_empty() {
        return PluginDiscover { message: Some(message.unwrap_or_else(|| "GitHub found nothing; try again in a minute.".into())), fetched_at: now, ..Default::default() };
    }
    let mut fresh_list: Vec<PluginRepo> = new.iter().take(12).cloned().collect();
    let mut top: Vec<PluginRepo> = known.iter().take(12).cloned().collect();
    dress(ctx, &mut fresh_list).await;
    dress(ctx, &mut top).await;
    {
        let mut board = BOARD.locked();
        if board.data.is_none() {
            board.data = Some(PluginDiscover { fresh: fresh_list.clone(), top: top.clone(), fetched_at: now, ..Default::default() });
            board.partial = true;
        }
    }

    // Rising: the stars of the last week, read for the best known and every new one.
    let mut pool: Vec<PluginRepo> = known.iter().take(40).cloned().collect();
    let mut in_pool: HashSet<String> = pool.iter().map(|r| r.full_name.to_lowercase()).collect();
    merge_into(&mut pool, &mut in_pool, new.iter().take(30).cloned().collect());
    let pool: Vec<PluginRepo> = stream::iter(pool)
        .map(|mut r| async move {
            add_trend(&mut r, now, 5).await;
            r
        })
        .buffer_unordered(16)
        .collect()
        .await;
    let mut trend: HashMap<String, PluginRepo> = pool.iter().map(|r| (r.full_name.to_lowercase(), r.clone())).collect();
    let mut rising: Vec<PluginRepo> = pool.into_iter().filter(|r| r.week.is_some_and(|w| w > 0)).collect();
    rising.sort_by_key(|x| std::cmp::Reverse(x.week));
    rising.truncate(12);
    // The fastest have more than five pages a week: read further back so their line has days.
    let mut rising: Vec<PluginRepo> = stream::iter(rising)
        .map(|mut r| async move {
            if r.spark_span < 7 * DAY {
                add_trend(&mut r, now, 15).await;
            }
            r
        })
        .buffered(12)
        .collect()
        .await;
    rising.sort_by_key(|x| std::cmp::Reverse(x.week));
    for r in &rising {
        trend.insert(r.full_name.to_lowercase(), r.clone());
    }
    let with_trend = |r: PluginRepo| {
        let t = trend.get(&r.full_name.to_lowercase());
        PluginRepo { week: t.and_then(|t| t.week), week_estimated: t.is_some_and(|t| t.week_estimated), spark: t.map(|t| t.spark.clone()).unwrap_or_default(), spark_span: t.map_or(0, |t| t.spark_span), ..r }
    };
    let fresh_list: Vec<PluginRepo> = fresh_list.into_iter().map(with_trend).collect();
    let top: Vec<PluginRepo> = top.into_iter().map(with_trend).collect();
    dress(ctx, &mut rising).await;
    PluginDiscover { rising, fresh: fresh_list, top, message, fetched_at: now, updating: false }
}

/// A search over all of GitHub, best match first.
pub async fn search(ctx: &Ctx<'_>, query: &str) -> PluginSearch {
    let words: String = query.chars().filter(|c| !c.is_control()).take(200).collect();
    let words = words.trim();
    if words.is_empty() {
        return PluginSearch::default();
    }
    match search_raw(&format!("{words} fork:false archived:false"), None, 30).await {
        Ok(mut repos) => {
            dress(ctx, &mut repos).await;
            PluginSearch { repos, message: None }
        }
        Err(e) => PluginSearch { repos: Vec::new(), message: Some(e.to_string()) },
    }
}

fn span(days: i64) -> String {
    match days {
        0 => "under a day".into(),
        1 => "1 day".into(),
        d if d < 60 => format!("{d} days"),
        d if d < 730 => format!("{} months", d / 30),
        d => format!("{} years", d / 365),
    }
}

pub(crate) async fn raw_file(full: &str, path: &str) -> Option<String> {
    let api = format!("repos/{full}/contents/{path}");
    gh(&["api", "-H", "Accept: application/vnd.github.raw", &api], 15).await.ok()
}

/// The README without its pictures, cut to `MAX_README` at a character boundary.
fn trimmed_readme(raw: &str) -> String {
    let mut readme = without_pictures(raw);
    if readme.len() > MAX_README {
        let mut cut = MAX_README;
        while !readme.is_char_boundary(cut) {
            cut -= 1;
        }
        readme.truncate(cut);
        readme.push_str("\n\n…");
    }
    readme
}

/// Fills in the repository's star week and sparkline from when it was starred. Returns the
/// 30-day curve and its span.
fn star_curves(repo: &mut PluginRepo, stars: Option<&[i64]>, now: i64) -> (Vec<u32>, i64) {
    match stars {
        Some(times) => {
            let complete = times.len() as u64 >= repo.stars;
            if let Some((week, est)) = week_of(times, now, complete) {
                repo.week = Some(week);
                repo.week_estimated = est;
            }
            (repo.spark, repo.spark_span) = curve(times, now, 14 * DAY, 14, complete);
            curve(times, now, 30 * DAY, 30, complete)
        }
        None => (Vec::new(), 0),
    }
}

/// The repository's own banner picture, when it set one.
async fn banner_of(og: Result<Value>) -> Option<String> {
    match og {
        Ok(g) if g["data"]["repository"]["usesCustomOpenGraphImage"].as_bool() == Some(true) => match g["data"]["repository"]["openGraphImageUrl"].as_str() {
            Some(url) => picture(url).await,
            None => None,
        },
        _ => None,
    }
}

/// Similar: the most starred with the same topic, the agent topics first.
async fn similar_repos(ctx: &Ctx<'_>, repo: &mut PluginRepo, full: &str) -> Vec<PluginRepo> {
    let topic = repo.topics.iter().find(|t| TOPICS.contains(&t.as_str())).or_else(|| repo.topics.first()).cloned();
    let mut similar = match topic {
        Some(t) => search_raw(&format!("topic:{t} fork:false archived:false"), Some("stars"), 6).await.unwrap_or_default(),
        None => Vec::new(),
    };
    similar.retain(|r| !r.full_name.eq_ignore_ascii_case(full));
    similar.truncate(3);
    dress(ctx, &mut similar).await;
    dress(ctx, std::slice::from_mut(repo)).await;
    similar
}

/// One repository's own page, kept for half an hour.
pub async fn detail(ctx: &Ctx<'_>, full: &str) -> Result<PluginDetail> {
    let full = check_name(full)?.to_owned();
    let key = full.to_lowercase();
    let kept = DETAILS.locked().get(&key).filter(|(at, _)| at.elapsed() < DETAIL_FOR).map(|(_, d)| d.clone());
    if let Some(mut d) = kept {
        let customs = crate::customs::list(ctx).await.unwrap_or_default();
        stamp(&customs, &mut d.repo);
        place_worker(ctx, &mut d).await;
        return Ok(d);
    }
    let now = now_ms();
    let v = gh_json(&["api", &format!("repos/{full}")], 15).await?;
    let mut repo = repo_of(&v).ok_or_else(|| anyhow!("GitHub did not answer with a repository"))?;
    let full = repo.full_name.clone();
    let (owner, name) = full.split_once('/').map(|(o, n)| (o.to_owned(), n.to_owned())).unwrap_or_default();
    let since = crate::activity::date_of(now - 30 * DAY, 0);

    let readme_api = format!("repos/{full}/readme");
    let commits_api = format!("repos/{full}/commits?per_page=100&since={since}T00:00:00Z");
    let people_api = format!("repos/{full}/contributors?per_page=100");
    let files_api = format!("repos/{full}/contents");
    let user_api = format!("users/{owner}");
    let graph = "query($o:String!,$n:String!){repository(owner:$o,name:$n){openGraphImageUrl usesCustomOpenGraphImage}}";
    let (o_arg, n_arg, q_arg) = (format!("o={owner}"), format!("n={name}"), format!("query={graph}"));
    let readme_args = ["api", "-H", "Accept: application/vnd.github.raw", readme_api.as_str()];
    let (commits_args, people_args, files_args, user_args) =
        (["api", commits_api.as_str()], ["api", people_api.as_str()], ["api", files_api.as_str()], ["api", user_api.as_str()]);
    let graph_args = ["api", "graphql", "-f", q_arg.as_str(), "-f", o_arg.as_str(), "-f", n_arg.as_str()];
    let (readme, commits, people, files, user, og, stars) = tokio::join!(
        gh(&readme_args, 15),
        gh_json(&commits_args, 15),
        gh_json(&people_args, 15),
        gh_json(&files_args, 15),
        gh_json(&user_args, 10),
        gh_json(&graph_args, 15),
        recent_stars(&full, repo.stars, now - 30 * DAY, 20),
    );

    let readme = trimmed_readme(&readme.unwrap_or_default());
    let count = |v: &Result<Value>| v.as_ref().ok().and_then(|v| v.as_array().map(|a| a.len() as u32)).unwrap_or(0);
    let files: Vec<String> = files
        .ok()
        .and_then(|v| v.as_array().map(|a| a.iter().filter_map(|f| f["name"].as_str().map(str::to_owned)).collect()))
        .unwrap_or_default();
    let has = |n: &str| files.iter().any(|f| f == n);
    let (package, pyproject, manifest) = tokio::join!(
        async { if has("package.json") { raw_file(&full, "package.json").await } else { None } },
        async { if has("pyproject.toml") { raw_file(&full, "pyproject.toml").await } else { None } },
        async { if has(crate::own_tools::FILE) { raw_file(&full, crate::own_tools::FILE).await } else { None } },
    );
    let worker_label = manifest.as_deref().and_then(|t| crate::own_tools::parse(t).ok()).and_then(|m| m.worker.map(|w| w.label));
    if worker_label.is_some() {
        repo.kind = "worker".into();
    }
    let scripts = package.as_deref().map(install_scripts).unwrap_or_default();
    let owner_since = user.ok().and_then(|u| u["created_at"].as_str().and_then(parse_time));

    let (curve_30, curve_span) = star_curves(&mut repo, stars.as_deref(), now);

    let banner = banner_of(og).await;

    let similar = similar_repos(ctx, &mut repo, &full).await;

    let mut d = PluginDetail {
        worker: worker_label.map(|label| PluginWorker { label, installed: None, why_not: None }),
        inside: inside_of(&files, package.as_deref(), pyproject.as_deref()),
        checks: checks_of(&repo, owner_since, &scripts, now),
        homepage: v["homepage"].as_str().filter(|h| h.starts_with("https://") || h.starts_with("http://")).map(str::to_owned),
        forks: v["forks_count"].as_u64().unwrap_or(0),
        commits_month: count(&commits),
        contributors: count(&people),
        repo,
        readme,
        banner,
        curve: curve_30,
        curve_span,
        similar,
    };
    DETAILS.locked().insert(key, (Instant::now(), d.clone()));
    place_worker(ctx, &mut d).await;
    Ok(d)
}

/// Where a Worker would be installed on this machine: its name inside the projects folder.
async fn worker_folder(ctx: &Ctx<'_>, name: &str) -> PathBuf {
    let root = crate::projects_root::get(ctx.store).await.map(PathBuf::from).unwrap_or_else(|_| crate::folders::home());
    root.join(name)
}

/// The GitHub login this machine's gh is signed in as.
async fn gh_login() -> Option<String> {
    gh(&["api", "user", "--jq", ".login"], 10).await.ok().map(|s| s.trim().to_owned()).filter(|s| !s.is_empty())
}

/// Fills in where a Worker is installed here, and whether it may be: your own repositories may,
/// anyone else's only once Customs has cleared them.
async fn place_worker(ctx: &Ctx<'_>, d: &mut PluginDetail) {
    let Some(worker) = d.worker.as_mut() else { return };
    let folder = worker_folder(ctx, &d.repo.name).await;
    worker.installed = folder.join(crate::own_tools::FILE).is_file().then(|| folder.display().to_string());
    worker.why_not = None;
    if worker.installed.is_none() && d.repo.customs != Some(crate::model::CustomsStatus::Cleared) {
        let own = gh_login().await.is_some_and(|me| me.eq_ignore_ascii_case(&d.repo.owner));
        if !own {
            worker.why_not = Some("Customs has not cleared it yet. Your own repositories install straight away.".into());
        }
    }
}

/// Installs a Worker from GitHub: a clone inside the projects folder, where Hub finds its
/// `hub.toml` like every other tool of yours, then its ready check.
pub async fn install_worker(ctx: &Ctx<'_>, full: &str) -> Result<WorkerReady> {
    let d = detail(ctx, full).await?;
    let worker = d.worker.as_ref().ok_or_else(|| anyhow!("{} is not a Worker (no [worker] in its hub.toml)", d.repo.full_name))?;
    if let Some(why) = &worker.why_not {
        bail!("{why}");
    }
    let folder = worker_folder(ctx, &d.repo.name).await;
    if !folder.join(crate::own_tools::FILE).is_file() {
        if folder.exists() {
            bail!("{} is already there and is not this Worker", folder.display());
        }
        let target = folder.display().to_string();
        gh(&["repo", "clone", &d.repo.full_name, &target], 600).await.map_err(|e| anyhow!(gh_message(e)))?;
        // The user pressed Install (and Customs cleared it, or it is their own repository).
        crate::own_tools::allow(ctx, &target).await?;
    }
    DETAILS.locked().remove(&d.repo.full_name.to_lowercase());
    Ok(ready(&folder).await)
}

/// A Worker's ready check (`[worker] ready`): one JSON document `{ ok, checks: [{ section,
/// status, label, detail }] }`, the shape Knegarn's `doctor.py --json` prints.
pub async fn ready(folder: &std::path::Path) -> WorkerReady {
    let mut out = WorkerReady { folder: folder.display().to_string(), ok: None, checks: Vec::new(), error: None };
    let manifest = match std::fs::read_to_string(folder.join(crate::own_tools::FILE)).map_err(anyhow::Error::from).and_then(|t| crate::own_tools::parse(&t)) {
        Ok(m) => m,
        Err(e) => {
            out.error = Some(format!("{e:#}"));
            return out;
        }
    };
    let Some(run) = manifest.worker.and_then(|w| w.ready) else { return out };
    match crate::own_tools::document_for(folder, &manifest.env, &run, Duration::from_secs(180)).await {
        Ok(doc) => {
            out.ok = doc["ok"].as_bool();
            out.checks = serde_json::from_value(doc["checks"].clone()).unwrap_or_default();
        }
        Err(e) => out.error = Some(format!("{e:#}")),
    }
    out
}

mod stars;
pub use stars::*;

mod readme;
pub use readme::*;

#[cfg(test)]
mod tests;
