//! The idea inbox: an idea typed or said at any time lands in the focus of the project it is
//! about (`docs/FOCUS.md`), under the ideas of the stage it fits or under Parked. Hub guesses the
//! project: an idea that names one goes there, otherwise a small model reads the projects' stages,
//! and without an answer it goes to the project you were looking at. The last ideas are kept in
//! the store, so the inbox can show where each went and move or take it back. An idea no project
//! could be guessed for waits there until you pick one.

use std::path::Path;
use std::sync::Arc;

use anyhow::{Context, Result, bail};

use crate::focus::{self, Focus};
use crate::folders::same_folder;
use crate::model::{InboxIdea as Idea, IdeaState, MoveIdea, NewIdea, StreamMsg, now_ms};

const KEY: &str = "ideas";
/// Ideas kept in the inbox; older ones are only in their focus files.
const KEEP: usize = 60;
/// Longest idea that goes into a focus file as it is; a model shortens longer ones.
const SHORT: usize = 140;

/// One read-modify-write of the list at a time.
static LIST: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

/// A project an idea can go to.
#[derive(Debug, Clone)]
pub struct Candidate {
    pub path: String,
    pub name: String,
    pub focus: Focus,
}

/// Where an idea goes: the project, the stage (None is Parked), the line to write, why and who chose.
#[derive(Debug, Clone, PartialEq)]
pub struct Place {
    pub project: usize,
    pub stage: Option<usize>,
    pub text: String,
    pub why: Option<String>,
    pub by: &'static str,
}

/// The idea in one line, spaces tidied.
fn one_line(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// The project the idea names, when exactly one is named as a word ("Hub", "knegarn").
pub fn named(candidates: &[Candidate], text: &str) -> Option<usize> {
    let words: Vec<String> =
        text.to_lowercase().split(|c: char| !c.is_alphanumeric() && c != '-' && c != '_').map(str::to_owned).collect();
    let mut hits = candidates.iter().enumerate().filter(|(_, c)| {
        let name = c.name.to_lowercase();
        let folder = Path::new(&c.path).file_name().map(|n| n.to_string_lossy().to_lowercase()).unwrap_or_default();
        [name, folder].iter().any(|n| n.chars().count() >= 3 && words.iter().any(|w| w == n))
    });
    let first = hits.next()?.0;
    hits.next().is_none().then_some(first)
}

const ASK: &str = "The user just had an idea and wants it kept in the focus file of the project \
it is about, so it does not get lost. Do not use tools. Below are the user's projects, each with \
its stages in order. Pick the project the idea is about; when the idea does not say, prefer the \
one the user is looking at. Then pick the stage: one whose work the idea would be part of (the \
active stage or a later one), or 0 for Parked when it fits no stage. When unsure, Parked. Also \
write the idea as one short line in the user's own language and words, at most 15 words, no \
quotes. Answer with one line of JSON and nothing else: {\"project\": number, or 0 when no project \
fits, \"stage\": number, \"text\": \"the idea\", \"why\": \"at most 10 words\"}.";

/// What a small model is asked about an idea.
pub fn ask_prompt(candidates: &[Candidate], here: Option<usize>, said: &str) -> String {
    let mut text = format!("{ASK}\n\n<projects>\n");
    for (i, c) in candidates.iter().enumerate() {
        text.push_str(&format!("{}. {}{}\n", i + 1, c.name, if here == Some(i) { " (the user is looking at it)" } else { "" }));
        if let Some(d) = &c.focus.direction {
            text.push_str(&format!("   Direction: {}\n", d.title));
        }
        for (j, s) in c.focus.stages.iter().enumerate() {
            let mark = if c.focus.active == Some(j) {
                " (active)"
            } else if s.delivered() {
                " (delivered)"
            } else {
                ""
            };
            let items: Vec<&str> = s.done.iter().take(6).map(|c| c.text.as_str()).collect();
            let about: String = s.about.chars().take(160).collect();
            text.push_str(&format!("   Stage {}: {}{mark}. {about} {}\n", j + 1, s.title, items.join("; ")));
        }
        if !c.focus.exists {
            text.push_str("   (no stages yet: only Parked)\n");
        }
    }
    let said: String = said.chars().take(2_000).collect();
    text.push_str(&format!("</projects>\n\n<idea>\n{said}\n</idea>"));
    text
}

/// The model's answer as a place, checked against the projects. None when it is not usable.
pub fn read_answer(candidates: &[Candidate], answer: &str) -> Option<Place> {
    let (a, b) = (answer.find('{')?, answer.rfind('}')?);
    let v: serde_json::Value = serde_json::from_str(answer.get(a..=b)?).ok()?;
    let project = (v["project"].as_u64()? as usize).checked_sub(1)?;
    let c = candidates.get(project)?;
    let stage = (v["stage"].as_u64().unwrap_or(0) as usize).checked_sub(1).filter(|&s| s < c.focus.stages.len());
    let text = one_line(v["text"].as_str().unwrap_or_default()).trim_matches(['"', '.']).to_owned();
    let why = v["why"].as_str().map(str::trim).filter(|w| !w.is_empty()).map(str::to_owned);
    Some(Place { project, stage, text, why, by: "model" })
}

/// Where an idea goes without a model: the project it names, the one you were on, or the only
/// one, under Parked. None when there is no telling.
pub fn fallback(candidates: &[Candidate], here: Option<usize>, said: &str) -> Option<Place> {
    let (project, by) = match (named(candidates, said), here) {
        (Some(p), _) => (p, "name"),
        (None, Some(h)) => (h, "here"),
        (None, None) if candidates.len() == 1 => (0, "only"),
        _ => return None,
    };
    Some(Place { project, stage: None, text: idea_line(said), why: None, by })
}

/// The idea cut to a line that fits a focus file.
fn idea_line(said: &str) -> String {
    let line = one_line(said);
    if line.chars().count() <= SHORT {
        return line;
    }
    let cut: String = line.chars().take(SHORT).collect();
    let cut = cut.rfind(' ').map_or(cut.as_str(), |i| &cut[..i]).trim_end_matches([',', '.', ' ']);
    format!("{cut} …")
}

/// Writes the idea into the project's focus file, making one with only Parked when it has none.
fn write(path: &str, stage: Option<&str>, text: &str) -> Result<Option<String>> {
    let root = Path::new(path);
    if !root.join(focus::FOCUS).exists() {
        std::fs::create_dir_all(root.join("docs"))?;
        std::fs::write(root.join(focus::FOCUS), "# Focus\n\n## Parked\n")?;
    }
    let f = focus::read(root);
    let edit = |op: &str, line: Option<u32>| focus::FocusEdit {
        path: path.to_owned(),
        op: op.into(),
        line,
        done: None,
        list: None,
        text: Some(text.to_owned()),
    };
    match stage.and_then(|t| f.stages.iter().find(|s| s.title == t)) {
        Some(s) => {
            focus::edit(root, &edit("idea", Some(s.line)))?;
            Ok(Some(s.title.clone()))
        }
        None => {
            focus::edit(root, &edit("park", None))?;
            Ok(None)
        }
    }
}

impl crate::hub::Hub {
    /// The inbox, newest first.
    pub async fn ideas(&self) -> Result<Vec<Idea>> {
        Ok(match self.store.get_value(KEY).await? {
            Some(json) => serde_json::from_str(&json).unwrap_or_default(),
            None => Vec::new(),
        })
    }

    /// Changes the inbox and tells the surfaces; `change` returns the idea it touched.
    async fn change_ideas<T>(&self, change: impl FnOnce(&mut Vec<Idea>) -> Result<T>) -> Result<T> {
        let _held = LIST.lock().await;
        let mut list = self.ideas().await?;
        let out = change(&mut list)?;
        list.truncate(KEEP);
        self.store.set_value(KEY, &serde_json::to_string(&list)?).await?;
        Ok(out)
    }

    async fn put_idea(&self, idea: Idea) -> Result<()> {
        let sent = idea.clone();
        self.change_ideas(|list| {
            match list.iter_mut().find(|i| i.id == idea.id) {
                Some(i) => *i = idea,
                None => list.insert(0, idea),
            }
            Ok(())
        })
        .await?;
        self.publish(StreamMsg::Idea { idea: sent });
        Ok(())
    }

    /// The projects an idea can go to: added projects and folders threads work in, on this
    /// machine, with their focus.
    pub async fn idea_candidates(&self) -> Result<Vec<Candidate>> {
        let mut found: Vec<(String, String)> = self.store.projects().await?.into_iter().map(|p| (p.path, p.name)).collect();
        for t in self.store.threads().await? {
            if !t.archived && !found.iter().any(|(p, _)| same_folder(p, &t.cwd)) {
                let name = Path::new(&t.cwd).file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
                found.push((t.cwd, name));
            }
        }
        tokio::task::spawn_blocking(move || {
            found
                .into_iter()
                .filter(|(p, _)| Path::new(p).is_dir())
                .map(|(path, name)| Candidate { focus: focus::read(Path::new(&path)), path, name })
                .collect()
        })
        .await
        .context("reading the projects")
    }

    /// Keeps an idea and places it. With `wait` the answer says where it went; without, it says
    /// it is being sorted and the stream tells when it is placed.
    pub async fn capture_idea(self: &Arc<Self>, new: NewIdea, wait: bool) -> Result<Idea> {
        let said = new.text.trim().to_owned();
        if said.is_empty() {
            bail!("say or write the idea first");
        }
        let idea = Idea {
            id: uuid::Uuid::now_v7().to_string(),
            text: idea_line(&said),
            said,
            at: now_ms(),
            state: IdeaState::Sorting,
            project: None,
            name: None,
            stage: None,
            why: None,
            by: None,
        };
        self.put_idea(idea.clone()).await?;
        if wait {
            return self.place_idea(idea, new.here).await;
        }
        let hub = self.clone();
        let sorting = idea.clone();
        tokio::spawn(async move {
            if let Err(e) = hub.place_idea(idea, new.here).await {
                tracing::warn!("placing an idea: {e:#}");
            }
        });
        Ok(sorting)
    }

    async fn place_idea(self: &Arc<Self>, mut idea: Idea, here: Option<String>) -> Result<Idea> {
        let candidates = self.idea_candidates().await?;
        let here = here.and_then(|h| candidates.iter().position(|c| same_folder(&c.path, &h)));
        let mut place = None;
        if !candidates.is_empty() {
            let name = self.config.tidy.engine.as_ref().unwrap_or(&self.config.default_engine);
            if let Some(engine) = self.config.engines.get(name) {
                let cwd = here
                    .or(named(&candidates, &idea.said))
                    .map_or_else(|| std::env::temp_dir().to_string_lossy().into_owned(), |i| candidates[i].path.clone());
                let prompt = ask_prompt(&candidates, here, &idea.said);
                let answer = crate::engine::complete("idea_place", engine, &cwd, self.config.tidy.model.as_deref(), prompt);
                match tokio::time::timeout(std::time::Duration::from_secs(90), answer).await {
                    Ok(Ok(text)) => place = read_answer(&candidates, &text),
                    Ok(Err(e)) => tracing::info!("asking where an idea goes: {e:#}"),
                    Err(_) => tracing::info!("asking where an idea goes took too long"),
                }
            }
        }
        // A project the idea names wins over the model's pick.
        if let (Some(p), Some(n)) = (place.as_mut(), named(&candidates, &idea.said))
            && p.project != n {
                *p = Place { project: n, stage: None, by: "name", why: None, text: std::mem::take(&mut p.text) };
            }
        let place = place.or_else(|| fallback(&candidates, here, &idea.said));
        match place {
            Some(p) => {
                let c = &candidates[p.project];
                if !p.text.is_empty() {
                    idea.text = p.text;
                }
                let stage = p.stage.map(|s| c.focus.stages[s].title.clone());
                let (path, text) = (c.path.clone(), idea.text.clone());
                idea.stage = tokio::task::spawn_blocking(move || write(&path, stage.as_deref(), &text)).await??;
                idea.project = Some(c.path.clone());
                idea.name = Some(c.name.clone());
                idea.why = p.why;
                idea.by = Some(p.by.to_owned());
                idea.state = IdeaState::Placed;
            }
            None => idea.state = IdeaState::Unsorted,
        }
        self.put_idea(idea.clone()).await?;
        Ok(idea)
    }

    /// Puts an idea somewhere else: out of the focus file it is in, into `to`.
    pub async fn move_idea(&self, id: &str, to: MoveIdea) -> Result<Idea> {
        let mut idea = self.ideas().await?.into_iter().find(|i| i.id == id).context("that idea is no longer in the inbox")?;
        if idea.state == IdeaState::Sorting {
            bail!("the idea is still being sorted");
        }
        let name = self
            .idea_candidates()
            .await?
            .into_iter()
            .find(|c| same_folder(&c.path, &to.project))
            .map(|c| c.name)
            .with_context(|| format!("no project at {}", to.project))?;
        let (old, text, project, stage) = (idea.project.clone(), idea.text.clone(), to.project.clone(), to.stage.clone());
        idea.stage = tokio::task::spawn_blocking(move || {
            if let Some(old) = old {
                focus::remove_idea(Path::new(&old), &text)?;
            }
            write(&project, stage.as_deref(), &text)
        })
        .await??;
        idea.project = Some(to.project);
        idea.name = Some(name);
        idea.state = IdeaState::Placed;
        idea.by = Some("you".into());
        idea.why = None;
        self.put_idea(idea.clone()).await?;
        Ok(idea)
    }

    /// Takes an idea back: out of its focus file and out of the inbox.
    pub async fn drop_idea(&self, id: &str) -> Result<()> {
        let idea = self.ideas().await?.into_iter().find(|i| i.id == id).context("that idea is no longer in the inbox")?;
        if let Some(project) = idea.project.clone() {
            let text = idea.text.clone();
            tokio::task::spawn_blocking(move || focus::remove_idea(Path::new(&project), &text)).await??;
        }
        self.change_ideas(|list| {
            list.retain(|i| i.id != id);
            Ok(())
        })
        .await?;
        self.publish(StreamMsg::IdeaRemoved { id: id.to_owned() });
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn candidates() -> Vec<Candidate> {
        let hub = focus::parse(
            "## Direction: Daily driver\n- [ ] A week\n## Stage 1: PoC\n- [x] Runs\n## Stage 2: Daily (active)\n- [ ] Queue\n## Parked\n- Duck\n",
        );
        vec![
            Candidate { path: "/code/hub".into(), name: "Hub".into(), focus: hub },
            Candidate { path: "/code/knegarn".into(), name: "knegarn".into(), focus: Focus::default() },
        ]
    }

    #[test]
    fn a_named_project_is_found_as_a_word() {
        let c = candidates();
        assert_eq!(named(&c, "Hub ska kunna visa recept"), Some(0));
        assert_eq!(named(&c, "i Knegarn: bättre loggar"), Some(1));
        assert_eq!(named(&c, "a hubcap"), None, "only whole words");
        assert_eq!(named(&c, "hub and knegarn"), None, "two named, no telling");
    }

    #[test]
    fn asks_with_every_project_and_its_stages() {
        let c = candidates();
        let p = ask_prompt(&c, Some(1), "recept för jobb");
        assert!(p.contains("1. Hub\n   Direction: Daily driver\n   Stage 1: PoC (delivered)."));
        assert!(p.contains("Stage 2: Daily (active).  Queue\n"));
        assert!(p.contains("2. knegarn (the user is looking at it)\n   (no stages yet: only Parked)\n"));
        assert!(p.ends_with("<idea>\nrecept för jobb\n</idea>"));
    }

    #[test]
    fn reads_the_answer_against_the_projects() {
        let c = candidates();
        let p = read_answer(&c, "{\"project\": 1, \"stage\": 2, \"text\": \" Recept  för jobb.\", \"why\": \"fits daily\"}").unwrap();
        assert_eq!((p.project, p.stage, p.text.as_str(), p.why.as_deref(), p.by), (0, Some(1), "Recept för jobb", Some("fits daily"), "model"));
        let p = read_answer(&c, "{\"project\": 2, \"stage\": 3, \"text\": \"x\"}").unwrap();
        assert_eq!(p.stage, None, "a stage the project does not have is Parked");
        assert!(read_answer(&c, "{\"project\": 0, \"stage\": 0}").is_none());
        assert!(read_answer(&c, "{\"project\": 9}").is_none());
        assert!(read_answer(&c, "no").is_none());
    }

    #[test]
    fn without_a_model_it_goes_to_the_named_project_or_the_one_you_are_on() {
        let c = candidates();
        assert_eq!(fallback(&c, Some(1), "Hub: fokusläge").map(|p| (p.project, p.by)), Some((0, "name")));
        assert_eq!(fallback(&c, Some(1), "fokusläge").map(|p| (p.project, p.by)), Some((1, "here")));
        assert_eq!(fallback(&c, None, "fokusläge"), None);
        assert_eq!(fallback(&c[..1], None, "fokusläge").map(|p| p.by), Some("only"));
        let long = "ord ".repeat(60);
        let p = fallback(&c, Some(0), &long).unwrap();
        assert!(p.text.chars().count() <= SHORT + 2 && p.text.ends_with(" …"));
    }

    #[test]
    fn writes_under_a_stage_or_parked_and_makes_a_file_when_there_is_none() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().to_string_lossy().into_owned();
        assert_eq!(write(&root, Some("Daily"), "Ett").unwrap(), None, "no file: Parked");
        let f = focus::read(dir.path());
        assert_eq!(f.parked[0].text, "Ett");
        std::fs::write(dir.path().join(focus::FOCUS), "## Stage 1: Daily (active)\n- [ ] Queue\n\n## Parked\n").unwrap();
        assert_eq!(write(&root, Some("Daily"), "Två").unwrap().as_deref(), Some("Daily"));
        assert_eq!(write(&root, Some("Gone"), "Tre").unwrap(), None);
        let f = focus::read(dir.path());
        assert_eq!(f.stages[0].ideas[0].text, "Två");
        assert_eq!(f.parked[0].text, "Tre");
        assert!(focus::remove_idea(dir.path(), "Två").unwrap());
    }
}
