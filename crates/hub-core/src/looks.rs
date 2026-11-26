//! Icons the person picked for machines and projects, over the ones Hub gives them. Hub tells a
//! machine's kind from its hardware (`host.rs`) and gives a project a coloured icon from its name
//! (`app/src/lib/looks.ts`); this keeps only the choices that differ, in the store, together with
//! the projects pinned to the top of the rail.

use anyhow::{Result, bail};

use crate::hub::Hub;
use crate::model::Looks;

const KEY: &str = "looks";
/// The largest picture a project may have as its icon, as a data URL. The app shrinks a picture
/// to 96 pixels before it sends it, which is a few kilobytes.
pub const LOOK_IMAGE_MAX: usize = 96 * 1024;
const MAX_ENTRIES: usize = 1000;
/// More pins than this would not fit on any rail.
const MAX_PINNED: usize = 100;

impl Hub {
    pub async fn looks(&self) -> Result<Looks> {
        Ok(match self.store.get_value(KEY).await? {
            Some(json) => serde_json::from_str(&json).unwrap_or_default(),
            None => Looks::default(),
        })
    }

    pub async fn set_looks(&self, looks: Looks) -> Result<Looks> {
        check(&looks)?;
        self.store.set_value(KEY, &serde_json::to_string(&looks)?).await?;
        Ok(looks)
    }
}

/// Refuses what no icon needs: huge pictures, pictures that are not pictures, long names.
pub fn check(looks: &Looks) -> Result<()> {
    if looks.machines.len() + looks.projects.len() > MAX_ENTRIES {
        bail!("too many icons");
    }
    if looks.pinned.len() > MAX_PINNED {
        bail!("too many pinned projects");
    }
    if looks.pinned.iter().any(|k| k.len() > 4096) {
        bail!("project key too long");
    }
    for (key, look) in &looks.projects {
        if key.len() > 4096 {
            bail!("project key too long");
        }
        for name in [&look.icon, &look.color].into_iter().flatten() {
            if name.is_empty() || name.len() > 32 || !name.chars().all(|c| c.is_ascii_alphanumeric() || c == '-') {
                bail!("{name:?} is not an icon or colour name");
            }
        }
        if let Some(image) = &look.image {
            let ok = ["data:image/png;base64,", "data:image/jpeg;base64,", "data:image/webp;base64,", "data:image/gif;base64,"]
                .iter()
                .any(|p| image.starts_with(p));
            if !ok {
                bail!("an icon picture must be a PNG, JPEG, WebP or GIF data URL");
            }
            if image.len() > LOOK_IMAGE_MAX {
                bail!("an icon picture can be at most {} KB", LOOK_IMAGE_MAX / 1024);
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{MachineKind, ProjectLook};

    fn with(look: ProjectLook) -> Looks {
        let mut looks = Looks::default();
        looks.projects.insert("|/home/me/hub".into(), look);
        looks
    }

    #[test]
    fn takes_names_and_small_pictures() {
        let mut looks = with(ProjectLook { icon: Some("rocket".into()), color: Some("violet".into()), image: None });
        looks.machines.insert("tincan".into(), MachineKind::Server);
        assert!(check(&looks).is_ok());
        let pic = with(ProjectLook { image: Some("data:image/png;base64,iVBORw0KGgo=".into()), ..Default::default() });
        assert!(check(&pic).is_ok());
    }

    #[test]
    fn refuses_big_or_odd_pictures_and_names() {
        let big = format!("data:image/png;base64,{}", "A".repeat(LOOK_IMAGE_MAX));
        assert!(check(&with(ProjectLook { image: Some(big), ..Default::default() })).is_err());
        let svg = "data:image/svg+xml;base64,PHN2Zz4=".to_owned();
        assert!(check(&with(ProjectLook { image: Some(svg), ..Default::default() })).is_err());
        let odd = ProjectLook { icon: Some("<script>".into()), ..Default::default() };
        assert!(check(&with(odd)).is_err());
    }

    #[test]
    fn keeps_pins_in_order_and_old_stores_read_without_them() {
        let mut looks = Looks { pinned: vec!["|/home/me/hub".into(), "tincan|/srv/app".into()], ..Default::default() };
        assert!(check(&looks).is_ok());
        let back: Looks = serde_json::from_str(&serde_json::to_string(&looks).unwrap()).unwrap();
        assert_eq!(back.pinned, looks.pinned);
        let old: Looks = serde_json::from_str(r#"{"machines":{},"projects":{}}"#).unwrap();
        assert!(old.pinned.is_empty());
        looks.pinned = vec!["x".into(); MAX_PINNED + 1];
        assert!(check(&looks).is_err());
    }
}
