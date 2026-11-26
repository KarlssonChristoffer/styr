//! Tests of dictation: the `Voice` service, and the text that goes into and comes out of whisper.

use super::text::PROMPT_CHARS;
use super::*;

#[test]
fn cleans_markers_and_joins_lines() {
    assert_eq!(clean(" Hej, kan du fixa\n testet?\n[BLANK_AUDIO]\n"), "Hej, kan du fixa testet?");
    assert_eq!(clean("(music) Okay [inaudible] go"), "Okay go");
    assert_eq!(clean("[BLANK_AUDIO]"), "");
}

#[test]
fn drops_what_whisper_says_over_silence() {
    assert_eq!(clean(" Textning.nu\n"), "");
    assert_eq!(clean("Tack för att ni tittade!"), "");
    assert_eq!(clean("..."), "");
    assert_eq!(clean("Tack för hjälpen."), "Tack för hjälpen.");
}

#[test]
fn silence_is_a_valid_wav() {
    let wav = silence_wav(16_000);
    assert_eq!(wav.len(), 44 + 32_000);
    assert_eq!(&wav[0..4], b"RIFF");
    assert_eq!(&wav[36..40], b"data");
    assert_eq!(u32::from_le_bytes(wav[24..28].try_into().unwrap()), 16_000);
    assert_eq!(audio_ctx(wav.len()), 256);
}

#[test]
fn auto_language_means_swedish() {
    assert_eq!(pinned_language("auto"), "sv");
    assert_eq!(pinned_language(""), "sv");
    assert_eq!(pinned_language("en"), "en");
}

#[test]
fn says_why_whisper_server_stopped() {
    let said = "whisper_init_from_file: loading model\nerror: failed to bind to 127.0.0.1:5000\nmain: exiting\n";
    assert_eq!(why_it_stopped(said).as_deref(), Some("error: failed to bind to 127.0.0.1:5000"));
    assert_eq!(why_it_stopped("loading\nbye\n").as_deref(), Some("bye"));
    assert_eq!(why_it_stopped("\n  \n"), None);
}

#[test]
fn a_dead_server_is_not_held_against_the_card() {
    let dir = tempfile::tempdir().unwrap();
    let voice_dir = dir.path().join("voice");
    std::fs::create_dir_all(&voice_dir).unwrap();
    let failed = voice_dir.join(GPU_FAILED);
    std::fs::write(&failed, "curl: (7) Failed to connect to 127.0.0.1:52027 after 2043 ms").unwrap();
    Voice::new(dir.path(), VoiceConfig::default());
    assert!(!failed.exists());
    std::fs::write(&failed, "whisper-server exited: exit code: 0xc0000135").unwrap();
    Voice::new(dir.path(), VoiceConfig::default());
    assert!(failed.exists(), "a build that will not run stays marked");
}

#[tokio::test]
async fn hints_name_the_folder_its_branch_and_changed_files() {
    let dir = tempfile::tempdir().unwrap();
    let repo = dir.path().join("Hub");
    std::fs::create_dir_all(repo.join("src")).unwrap();
    let git = |args: &[&str]| assert!(std::process::Command::new("git").arg("-C").arg(&repo).args(args).output().unwrap().status.success());
    git(&["init", "-q", "-b", "dictation-fixes"]);
    git(&["-c", "user.name=t", "-c", "user.email=t@t", "commit", "-q", "--allow-empty", "-m", "start"]);
    std::fs::write(repo.join("src/voice.rs"), "x").unwrap();
    assert_eq!(folder_line(&repo.display().to_string()).await, "Hub, dictation fixes, src.");
    git(&["add", "."]);
    assert_eq!(folder_line(&repo.display().to_string()).await, "Hub, dictation fixes, voice.rs.");
    // A thread's own branch, with its made-up suffix, is left out.
    git(&["checkout", "-q", "-b", "claude/project-thread-a2nlce"]);
    assert_eq!(folder_line(&repo.display().to_string()).await, "Hub, voice.rs.");
    assert_eq!(folder_line("/no/such/folder").await, "");
}

#[test]
fn drops_an_echo_of_the_prompt() {
    assert!(echoes(VOCABULARY, "Claude ska göra en commit, pusha branchen"));
    assert!(!echoes(VOCABULARY, "Ja."));
}

#[test]
fn previews_encode_only_what_was_recorded() {
    let second = 32_000;
    assert_eq!(audio_ctx(44 + second), 256);
    assert_eq!(audio_ctx(44 + 10 * second), 576);
    assert_eq!(audio_ctx(44 + 40 * second), 1500);
}

#[test]
fn prompts_with_the_end_of_the_text_before() {
    assert_eq!(prompt(None, ""), VOCABULARY);
    assert_eq!(prompt(Some("  "), ""), VOCABULARY);
    assert_eq!(prompt(Some("Fixa testet i auth."), ""), "Fixa testet i auth.");
    let long = "ord ".repeat(200);
    let p = prompt(Some(&long), "");
    assert!(p.chars().count() <= PROMPT_CHARS && p.starts_with("ord"));
    // The folder's names go first, and the whole stays inside whisper's window.
    let p = prompt(Some(&long), "Hub, voice.rs, Composer.svelte.");
    assert!(p.starts_with("Hub, voice.rs, Composer.svelte. ord"));
    assert!(p.chars().count() <= PROMPT_CHARS + 1);
    assert_eq!(prompt(None, "Hub."), format!("Hub. {VOCABULARY}"));
}

#[cfg(unix)]
#[tokio::test]
async fn transcribes_with_a_configured_command() {
    let dir = tempfile::tempdir().unwrap();
    let cli = crate::janitor::test_script(dir.path(), "fake-whisper", "#!/bin/sh\necho ' Fixa testet i auth.'\necho '[BLANK_AUDIO]'\n");
    let config = VoiceConfig { command: Some(cli), ..Default::default() };
    let voice = Voice::new(dir.path(), config);
    assert_eq!(voice.status().state, VoiceState::Missing);
    std::fs::create_dir_all(dir.path().join("voice")).unwrap();
    std::fs::write(dir.path().join("voice/kb-whisper-small-q5_0.bin"), b"model").unwrap();
    assert_eq!(voice.transcribe(b"RIFF", &Hints::default()).await.unwrap(), "Fixa testet i auth.");
    assert_eq!(voice.status().state, VoiceState::Ready);
    // A long download in progress does not hold up dictation once something is installed.
    let _busy = voice.setup.lock().await;
    let hints = Hints::default();
    let quick = tokio::time::timeout(std::time::Duration::from_secs(5), voice.transcribe(b"RIFF", &hints));
    assert_eq!(quick.await.expect("waited on the setup lock").unwrap(), "Fixa testet i auth.");
}
