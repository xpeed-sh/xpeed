//! Durable chat content, separate from provider credentials and live tool authority.
use super::{ChatMessage, Runtime, chat, session_tools};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    path::PathBuf,
    time::{Duration, Instant},
};

#[derive(Default)]
pub(super) struct State {
    path: Option<PathBuf>,
    last: Option<Instant>,
    snapshot: Vec<u8>,
}

#[derive(Serialize, Deserialize, Default)]
#[serde(default)]
struct Saved {
    version: u32,
    returning_account: bool,
    model: String,
    approve_task: bool,
    next: u64,
    archive_next: u64,
    thread_id: Option<String>,
    title: String,
    draft: String,
    home_draft: String,
    messages: Vec<ChatMessage>,
    actions: Vec<session_tools::Action>,
    archives: Vec<(String, Vec<ChatMessage>, Vec<session_tools::Action>)>,
    threads: BTreeMap<String, String>,
    titles: BTreeMap<String, String>,
    drafts: BTreeMap<String, String>,
}

fn deactivate(actions: &mut [session_tools::Action]) {
    for action in actions {
        // Historical display only. Pending approvals must never be replayed.
        action.request = serde_json::Value::Null;
        if matches!(
            action.state.as_str(),
            "running" | "awaitingApproval" | "cancelling"
        ) {
            action.state = "cancelled".into();
        }
    }
}

impl Runtime {
    pub(super) fn load_chat(&mut self, path: PathBuf) {
        if !self
            .manifest
            .capabilities
            .iter()
            .any(|c| c == "sessionTools")
        {
            return;
        }
        self.chat_state.path = Some(path.clone());
        self.auto_connect = true;
        match std::fs::read(&path) {
            Ok(bytes) => match serde_json::from_slice::<Saved>(&bytes) {
                Ok(mut saved) if saved.version == 1 => {
                    deactivate(&mut saved.actions);
                    for (_, _, actions) in &mut saved.archives {
                        deactivate(actions);
                    }
                    self.returning_account = saved.returning_account;
                    self.model = saved.model;
                    self.tools.approve_task = saved.approve_task;
                    self.next = saved.next;
                    self.archive_next = saved.archive_next;
                    self.thread_id = saved.thread_id;
                    self.chat_title = saved.title;
                    self.draft = saved.draft;
                    self.home_draft = saved.home_draft;
                    self.messages = saved.messages;
                    self.tools.actions = saved.actions;
                    self.archives = saved.archives;
                    self.saved_threads = saved.threads;
                    self.saved_titles = saved.titles;
                    self.saved_drafts = saved.drafts;
                }
                _ => {
                    self.error = Some("Could not read saved conversations.".into());
                    self.chat_state.path = None;
                }
            },
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => {
                self.error = Some(e.to_string());
                self.chat_state.path = None;
            }
        }
    }

    pub(super) fn persist_chat(&mut self, force: bool) {
        let Some(path) = self.chat_state.path.clone() else {
            return;
        };
        if !force
            && self
                .chat_state
                .last
                .is_some_and(|last| last.elapsed() < Duration::from_secs(2))
        {
            return;
        }
        self.chat_state.last = Some(Instant::now());
        let bytes = serde_json::to_vec(&serde_json::json!({
            "version":1,"returning_account":self.returning_account,"model":self.model,
            "approve_task":self.tools.approve_task,"next":self.next,"archive_next":self.archive_next,
            "thread_id":self.thread_id,"title":self.chat_title,"draft":self.draft,"home_draft":self.home_draft,
            "messages":self.messages,"actions":self.tools.actions,"archives":self.archives,
            "threads":self.saved_threads,"titles":self.saved_titles,"drafts":self.saved_drafts
        })).unwrap_or_default();
        if bytes == self.chat_state.snapshot {
            return;
        }
        let result = (|| -> std::io::Result<()> {
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent)?;
            }
            let temporary = path.with_extension("json.tmp");
            std::fs::write(&temporary, &bytes)?;
            std::fs::rename(temporary, path)
        })();
        match result {
            Ok(()) => self.chat_state.snapshot = bytes,
            Err(e) => self.error = Some(format!("Could not save conversations: {e}")),
        }
    }

    pub(super) fn resume_saved(&mut self, index: usize) {
        if self.busy || index >= self.archives.len() || !self.deferred_actions.is_empty() {
            return;
        }
        let (key, messages, mut actions) = self.archives.remove(index);
        let thread = self.saved_threads.remove(&key);
        let title = self.saved_titles.remove(&key).unwrap_or_default();
        let draft = self.saved_drafts.remove(&key).unwrap_or_default();
        self.request("conversation/reset", serde_json::json!({}));
        self.archive_conversation();
        deactivate(&mut actions);
        self.messages = messages;
        self.tools.actions = actions;
        self.thread_id = thread;
        self.chat_title = title;
        self.draft = draft;
        self.error = None;
        self.last_config.clear();
        self.chat_page = chat::Page::Chat;
    }

    pub(super) fn delete_chat(&mut self, saved: Option<usize>) {
        if self.busy || !self.deferred_actions.is_empty() {
            return;
        }
        if let Some(index) = saved {
            if index >= self.archives.len() {
                return;
            }
            let (key, _, _) = self.archives.remove(index);
            self.saved_threads.remove(&key);
            self.saved_titles.remove(&key);
            self.saved_drafts.remove(&key);
        } else {
            if self.ready {
                self.request("conversation/reset", serde_json::json!({}));
            }
            self.tools.reset();
            self.messages.clear();
            self.thread_id = None;
            self.chat_title.clear();
            self.draft.clear();
            self.last_config.clear();
            self.chat_page = chat::Page::Conversations;
        }
        self.persist_chat(true);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn runtime() -> Runtime {
        Runtime::new(
            serde_json::from_str(include_str!("../../extensions/codex/extension.json")).unwrap(),
            PathBuf::new(),
            true,
            std::path::Path::new("."),
            String::new(),
        )
    }
    #[test]
    fn restart_restores_history_preferences_and_threads_without_live_authority() {
        let path = std::env::temp_dir().join(format!("xpeed-chat-{}.json", rand::random::<u64>()));
        let mut original = runtime();
        original.load_chat(path.clone());
        original.returning_account = true;
        original.model = "chosen-model".into();
        original.tools.approve_task = true;
        original.tools.selected.insert(42);
        original.tools.token = "never-restore-this-authority".into();
        original.thread_id = Some("thread-one".into());
        original.chat_title = "My server".into();
        original.message("user-1", "user", "first conversation", false);
        original.archive_conversation();
        original.thread_id = Some("thread-two".into());
        original.message("user-2", "user", "second conversation", false);
        original.draft = "draft text".into();
        original.home_draft = "new conversation draft".into();
        original.tools.actions.push(session_tools::Action {
            turn: "user-2".into(),
            request: serde_json::json!({"id":12,"params":{"scopeToken":"never-restore-this-authority"}}),
            tool: "xpeed_exec".into(),
            args: serde_json::json!({"command":"ls"}),
            target: "server".into(),
            state: "awaitingApproval".into(),
            result: None,
        });
        original.message("answer-2", "assistant", "Saved **answer**", false);
        original.messages.last_mut().unwrap().elapsed_ms = Some(197_000);
        original.persist_chat(true);
        assert!(
            !std::fs::read_to_string(&path)
                .unwrap()
                .contains("never-restore-this-authority")
        );
        let mut restored = runtime();
        restored.load_chat(path.clone());
        assert!(restored.returning_account && restored.auto_connect && restored.tools.approve_task);
        assert_eq!(restored.model, "chosen-model");
        assert_eq!(restored.draft, "draft text");
        assert_eq!(restored.home_draft, "new conversation draft");
        assert_eq!(restored.thread_id.as_deref(), Some("thread-two"));
        assert!(restored.tools.selected.is_empty() && restored.tools.token.is_empty());
        assert_eq!(restored.tools.actions[0].state, "cancelled");
        assert_eq!(restored.tools.actions[0].turn, "user-2");
        assert_eq!(restored.messages[1].turn, "user-2");
        assert!(restored.messages[1].created_at > 0);
        assert_eq!(restored.messages[1].elapsed_ms, Some(197_000));
        assert!(restored.tools.actions[0].request.is_null());
        restored.resume_saved(0);
        assert_eq!(restored.chat_page, chat::Page::Chat);
        assert_eq!(restored.thread_id.as_deref(), Some("thread-one"));
        assert_eq!(restored.chat_title, "My server");
        assert!(restored.draft.is_empty());
        assert_eq!(restored.messages[0].text, "first conversation");
        assert_eq!(restored.archives[0].1[0].text, "second conversation");
        restored.persist_chat(true);
        let mut reopened = runtime();
        reopened.load_chat(path.clone());
        assert_eq!(reopened.thread_id.as_deref(), Some("thread-one"));
        let _ = std::fs::remove_file(path);
    }
    #[test]
    fn deleting_chat_removes_its_saved_data_and_stays_deleted_after_restart() {
        let path =
            std::env::temp_dir().join(format!("xpeed-delete-chat-{}.json", rand::random::<u64>()));
        let mut original = runtime();
        original.load_chat(path.clone());
        original.thread_id = Some("saved-thread".into());
        original.chat_title = "Delete this".into();
        original.draft = "saved draft".into();
        original.message("1", "user", "saved chat", false);
        original.archive_conversation();
        original.message("2", "user", "current chat", false);
        original.persist_chat(true);
        original.busy = true;
        original.delete_chat(Some(0));
        assert_eq!(original.archives.len(), 1);
        original.busy = false;
        original.delete_chat(Some(0));
        assert!(
            original.archives.is_empty()
                && original.saved_threads.is_empty()
                && original.saved_titles.is_empty()
                && original.saved_drafts.is_empty()
        );
        let mut restored = runtime();
        restored.load_chat(path.clone());
        assert!(restored.archives.is_empty());
        assert_eq!(restored.messages[0].text, "current chat");
        restored.delete_chat(None);
        let mut reopened = runtime();
        reopened.load_chat(path.clone());
        assert!(reopened.messages.is_empty() && reopened.archives.is_empty());
        let _ = std::fs::remove_file(path);
    }
    #[test]
    fn existing_account_skips_onboarding_and_busy_chats_cannot_switch_threads() {
        let mut runtime = runtime();
        runtime.account_result(serde_json::json!({"account":{"type":"chatgpt"}}));
        assert_eq!(runtime.chat_page, chat::Page::Conversations);
        runtime.message("first", "user", "first", false);
        runtime.archive_conversation();
        runtime.message("second", "user", "second", false);
        runtime.busy = true;
        runtime.resume_saved(0);
        assert_eq!(runtime.messages[0].text, "second");
    }
}
