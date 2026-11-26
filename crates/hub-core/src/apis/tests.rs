//! Guards how API connections hold their keys: an engine's key can never be a connection, the
//! address goes with the key, a failed check never shows a key sent in the address, and a project
//! gets key and address only where the connection is turned on.

use super::*;

fn jev() -> ApiConnection {
    let k = &catalog()[0];
    ApiConnection {
        name: "jev".into(),
        kind: k.kind.clone(),
        label: k.label.clone(),
        url: k.url.clone(),
        secret: k.secret.clone(),
        model: k.model.clone(),
        test_path: k.test_path.clone(),
        docs: k.docs.clone(),
        daily_tokens: 0,
        projects: vec![r"D:\Dev\atlas".into()],
        auth: ApiAuth::Bearer,
        client_id: None,
        credit: None,
    }
}

#[test]
fn engine_keys_can_not_be_connections() {
    assert!(valid(&jev()).is_ok());
    let mut c = jev();
    c.secret = "ANTHROPIC_API_KEY".into();
    assert!(valid(&c).is_err());
    c.secret = "typesafe key".into();
    assert!(valid(&c).is_err());
    let mut c = jev();
    c.name = "Jev".into();
    assert!(valid(&c).is_err());
}

#[test]
fn the_address_rides_next_to_the_key() {
    assert_eq!(url_var("TYPESAFE_API_KEY"), "TYPESAFE_BASE_URL");
    assert_eq!(url_var("FOO_TOKEN"), "FOO_BASE_URL");
    assert_eq!(url_var("BAR"), "BAR_BASE_URL");
    assert!(same_folder(r"D:\Dev\Atlas\", "d:/dev/atlas"));
}

#[tokio::test]
async fn an_unanswered_check_never_carries_a_key_sent_in_the_address() -> Result<()> {
    let data = tempfile::tempdir()?;
    let config = crate::Config { data_dir: data.path().to_owned(), ..Default::default() };
    let hub = Hub::new(crate::Store::in_memory()?, config);
    let mut c = jev();
    // A closed port: nothing answers, so the error is reqwest's own.
    c.url = "http://127.0.0.1:1".into();
    let signing = Signing::Query("appid".into(), "sekretnyckel1234567".into());
    let test = hub.try_key(&c, &signing).await;
    assert!(!test.ok);
    assert!(!test.message.contains("sekretnyckel"), "{}", test.message);
    Ok(())
}

#[tokio::test]
async fn projects_get_the_key_and_the_address_only_where_turned_on() -> Result<()> {
    let data = tempfile::tempdir()?;
    let config = crate::Config { data_dir: data.path().to_owned(), ..Default::default() };
    let hub = Hub::new(crate::Store::in_memory()?, config);
    hub.secrets.set("TYPESAFE_API_KEY", "ts-abcdefghijklmnop1234").await?;
    hub.set_api_prefs(ApiPrefs { connections: vec![jev()], helper: Some("jev".into()), off: vec!["focus".into(), "nope".into()], ..Default::default() }).await?;
    assert_eq!(hub.api_prefs().await?.off, vec!["focus".to_owned()]);
    assert_eq!(hub.helper("turns").await.as_deref(), Some("jev"));
    assert_eq!(hub.helper("focus").await, None);
    let env = hub.api_env("d:/dev/atlas/").await;
    assert!(env.contains(&("TYPESAFE_API_KEY".into(), "ts-abcdefghijklmnop1234".into())));
    assert!(env.contains(&("TYPESAFE_BASE_URL".into(), "https://api.typesafe.ai".into())));
    assert!(hub.api_env(r"D:\Dev\other").await.is_empty());

    let brief = hub.api_brief(r"D:\Dev\atlas").await?;
    assert_eq!(brief["connections"][0]["key_env"], "TYPESAFE_API_KEY");
    assert!(!brief.to_string().contains("abcdefghijklmnop"));

    let state = hub.api_state().await?;
    assert_eq!(state.status[0].key.as_deref(), Some("…1234"));
    assert!(state.status[0].check.is_none());
    hub.note_check("jev", true, "The key works.").await;
    assert!(hub.api_state().await?.status[0].check.as_ref().is_some_and(|c| c.ok));

    // What is left of the credit is counted from the tokens Hub had counted when it was entered.
    let answer = |tokens| ApiAnswer { model: "jev".into(), answers: json!({}), ms: 1, input_tokens: tokens, output_tokens: 0 };
    hub.count("jev", Some(&answer(1000)), None).await;
    let mut prefs = hub.api_prefs().await?;
    prefs.connections[0].credit = Some(ApiCredit { usd: 4.0, tokens_at: 7, at: 7 });
    let saved = hub.set_api_prefs(prefs.clone()).await?;
    let credit = saved.connections[0].credit.clone().unwrap();
    assert_eq!(credit.tokens_at, 1000);
    assert!(credit.at > 7);
    hub.count("jev", Some(&answer(500)), None).await;
    let total = hub.api_state().await?.status[0].total.clone().unwrap();
    assert_eq!((total.calls, total.input_tokens), (2, 1500));
    // The same amount saved again keeps where it started; a new one starts again from now.
    let again = hub.set_api_prefs(saved.clone()).await?;
    assert_eq!(again.connections[0].credit.as_ref().unwrap().tokens_at, 1000);
    let mut topped = saved.clone();
    topped.connections[0].credit = Some(ApiCredit { usd: 9.0, ..Default::default() });
    assert_eq!(hub.set_api_prefs(topped).await?.connections[0].credit.as_ref().unwrap().tokens_at, 1500);
    let mut bad = saved;
    bad.connections[0].credit = Some(ApiCredit { usd: -1.0, ..Default::default() });
    assert!(hub.set_api_prefs(bad).await.is_err());

    // Only a connection that answers questions can make Hub's decisions.
    let mut custom = jev();
    custom.name = "other".into();
    custom.kind = "custom".into();
    custom.secret = "OTHER_KEY".into();
    let refused = hub.set_api_prefs(ApiPrefs { connections: vec![custom], helper: Some("other".into()), off: Vec::new(), ..Default::default() }).await;
    assert!(refused.is_err());
    Ok(())
}
